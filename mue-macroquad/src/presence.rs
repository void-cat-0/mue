//! Enter and exit animations for conditional content (Vue's `<Transition>`).
//!
//! [`presence`] mounts its content when `present` becomes true, animates a
//! progress value from 0 to 1, and when `present` becomes false animates back
//! to 0 before unmounting. Built-in [`PresenceAnimation`]s fade, scale, and
//! slide the wrapper; content can read the same progress with [`use_presence`]
//! for custom effects.
//!
//! ```
//! use mue_core::prelude::*;
//! use mue_macroquad::{
//!     node::flexbox,
//!     presence::{PresenceAnimation, presence},
//!     style::Styleable,
//!     transition::Transition,
//! };
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let open = signal(false);
//! let _dialog = presence(open, || flexbox().width(240.).height(120.))
//!     .animation(PresenceAnimation::fade().scale(0.95))
//!     .transition(Transition::ease_out(0.18));
//! # });
//! ```

use std::{cell::RefCell, rc::Rc};

use mue_core::{
    Disposable as _,
    effect::{computed, watch_effect},
    prop::{Prop, PropValue},
    scope::{OwnedScope, inject, provide},
    signal::{Access, ReadSignal, Signal, signal},
};

use crate::{
    node::{BuildNode, IntoNode, Node, NodeContext, NodeRef, flexbox::use_container, set_children},
    style::{Style, Styleable, Transform, TransformOp, values::Display},
    transition::{Transition, animated},
};

/// Wrapper effects driven by presence progress (0 hidden, 1 shown).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresenceAnimation {
    /// Fade opacity with progress.
    pub fade: bool,
    /// Scale from this factor to 1 (1 disables scaling).
    pub scale: f32,
    /// Translate from this offset (logical pixels) to zero.
    pub offset: (f32, f32),
}
impl PropValue for PresenceAnimation {}

impl Default for PresenceAnimation {
    fn default() -> Self {
        Self::fade()
    }
}

impl PresenceAnimation {
    /// No wrapper effect; content can still use [`use_presence`].
    pub const fn none() -> Self {
        Self {
            fade: false,
            scale: 1.,
            offset: (0., 0.),
        }
    }

    pub const fn fade() -> Self {
        Self {
            fade: true,
            ..Self::none()
        }
    }

    /// Also scale from `from` (for example 0.95) to 1.
    pub const fn scale(mut self, from: f32) -> Self {
        self.scale = from;
        self
    }

    /// Also slide from `(dx, dy)`; `slide(0., 16.)` rises from below.
    pub const fn slide(mut self, dx: f32, dy: f32) -> Self {
        self.offset = (dx, dy);
        self
    }
}

/// Presence of the nearest [`presence`] ancestor.
#[derive(Clone, Copy)]
pub struct PresenceState {
    /// The requested state (true while open, including the enter animation).
    pub present: ReadSignal<bool>,
    /// Animated progress from 0 (hidden) to 1 (shown).
    pub progress: ReadSignal<f32>,
}

/// The nearest [`presence`] ancestor's state.
pub fn use_presence() -> Option<PresenceState> {
    inject::<PresenceState>()
}

/// Builder returned by [`presence`].
pub struct PresenceBuilder<F> {
    present: Prop<bool>,
    view: F,
    animation: PresenceAnimation,
    transition: Prop<Transition>,
    keep_mounted: bool,
    style: Style,
}

/// Mount `view` while `present`, animating in and out.
///
/// The wrapper is a flex container with `display: none` while unmounted.
/// With `keep_mounted(true)` the content is built once and kept (hidden) after
/// the exit animation, preserving its state.
pub fn presence<F, N>(present: impl Into<Prop<bool>>, view: F) -> PresenceBuilder<F>
where
    F: FnMut() -> N + 'static,
    N: IntoNode,
{
    PresenceBuilder {
        present: present.into(),
        view,
        animation: PresenceAnimation::fade(),
        transition: Prop::Static(Transition::ease_out(0.2)),
        keep_mounted: false,
        style: Style::new(),
    }
}

impl<F> PresenceBuilder<F> {
    pub fn animation(mut self, animation: PresenceAnimation) -> Self {
        self.animation = animation;
        self
    }

    /// Curve and duration for both directions.
    pub fn transition(mut self, transition: impl Into<Prop<Transition>>) -> Self {
        self.transition = transition.into();
        self
    }

    pub fn keep_mounted(mut self, keep: bool) -> Self {
        self.keep_mounted = keep;
        self
    }
}

impl<F> Styleable for PresenceBuilder<F> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

const SETTLED: f32 = 0.001;

impl<F, N> BuildNode for PresenceBuilder<F>
where
    F: FnMut() -> N + 'static,
    N: IntoNode,
{
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            present,
            mut view,
            animation,
            transition,
            keep_mounted,
            style,
        } = self;
        let target = present.map(|present| if present { 1_f32 } else { 0. });
        let progress = match animated(target, transition) {
            Prop::Static(value) => *signal(value),
            Prop::Dynamic(signal) => signal,
        };
        let present_signal = match present {
            Prop::Static(value) => *signal(value),
            Prop::Dynamic(signal) => signal,
        };
        provide(PresenceState {
            present: present_signal,
            progress,
        });
        // Mounted while present or while the exit animation runs.
        let mounted: Signal<bool> = signal(present.get_untracked());
        watch_effect(move || {
            let present = present_signal.get();
            let progress = progress.get();
            let next = present || progress > SETTLED;
            if mounted.get_untracked() != next {
                mounted.set(next);
            }
        });
        ctx.push_style(style);
        let mut defaults = Style::new().display(computed(move |_| {
            if mounted.get() {
                Display::Flex
            } else {
                Display::None
            }
        }));
        if animation.fade {
            defaults = defaults.opacity(computed(move |_| progress.get().clamp(0., 1.)));
        }
        if animation.scale != 1. || animation.offset != (0., 0.) {
            defaults = defaults.transform(computed(move |_| {
                let p = progress.get().clamp(0., 1.);
                let hidden = 1. - p;
                let mut ops = Vec::new();
                if animation.offset != (0., 0.) {
                    ops.push(TransformOp::Translate(
                        crate::style::Size::Px(animation.offset.0 * hidden),
                        crate::style::Size::Px(animation.offset.1 * hidden),
                    ));
                }
                if animation.scale != 1. {
                    let s = animation.scale + (1. - animation.scale) * p;
                    ops.push(TransformOp::Scale(s, s));
                }
                Transform(ops)
            }));
        }
        ctx.finish(defaults, move |style| {
            let scope = mue_core::scope::current_scope();
            // The current content and the scope that owns its resources.
            let current: Rc<RefCell<Option<(Node, OwnedScope)>>> = Rc::default();
            let children = computed(move |_| {
                let show = mounted.get();
                mue_core::effect::untracked(|| {
                    let mut current = current.borrow_mut();
                    if show && current.is_none() {
                        let owner = scope.run(OwnedScope::new_inherited);
                        let node = owner.run(|| view().into_node());
                        *current = Some((node, owner));
                    } else if !show && !keep_mounted {
                        // Drop the node before its owning scope.
                        if let Some((node, owner)) = current.take() {
                            drop(node);
                            drop(owner);
                        }
                    }
                    match current.as_ref() {
                        Some((node, _)) => Rc::<[NodeRef]>::from([**node]),
                        None => Rc::from([]),
                    }
                })
            })
            .owned();
            set_children(children);
            use_container(style);
        })
    }
}
