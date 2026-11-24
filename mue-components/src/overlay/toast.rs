//! Transient notifications for an input-transparent overlay.

use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::{Rc, Weak},
};

use mue_core::{
    prop::Prop,
    signal::{Access, ReadSignal, Signal, signal},
};
use mue_macroquad::{
    color::Color,
    math::{Matrix, vec2},
    node::{BuildNode, Component, Node, NodeContext, flexbox, map_keyed, text},
    shader::Brush,
    style::{Style, Styleable, percent},
};

use crate::{stack, style};

const WIDTH: f32 = 320.;
const MARGIN: f32 = 18.;
const MAX_VISIBLE: usize = 3;
const ENTER: f64 = 0.22;
const HOLD: f64 = 2.;
const EXIT: f64 = 0.32;

/// A queued message's tone. The application provides its text and optional label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn accent(self) -> Color {
        match self {
            Self::Info => Color::from_rgba(79, 153, 255, 255),
            Self::Success => Color::from_rgba(44, 170, 121, 255),
            Self::Warning => Color::from_rgba(220, 145, 45, 255),
            Self::Error => Color::from_rgba(230, 96, 105, 255),
        }
    }
}

/// Message content built in the lifetime of its retained card.
pub struct ToastContent {
    label: Option<Prop<Rc<str>>>,
    message: Prop<Rc<str>>,
}

impl ToastContent {
    pub fn new(message: impl Into<Prop<Rc<str>>>) -> Self {
        Self {
            label: None,
            message: message.into(),
        }
    }

    pub fn label(mut self, label: impl Into<Prop<Rc<str>>>) -> Self {
        self.label = Some(label.into());
        self
    }
}

type ContentBuilder = Rc<dyn Fn() -> ToastContent>;

#[derive(Clone)]
struct Pending {
    kind: ToastKind,
    content: ContentBuilder,
}

type Inbox = RefCell<VecDeque<Pending>>;

/// A sender for one ToastHost, usable from the UI thread. It does not keep the
/// host alive; after removal, sending has no effect.
#[derive(Clone)]
pub struct ToastSender(Weak<Inbox>);

impl ToastSender {
    /// Enqueue owned text, independent of the sending Scene's lifetime.
    pub fn show(&self, kind: ToastKind, message: impl Into<Rc<str>>) {
        let message = message.into();
        self.show_with(kind, move || ToastContent::new(message.clone()));
    }

    /// Build content when a slot opens, inside the card's reactive scope.
    /// The callback runs synchronously during retained child construction; use
    /// it to create text bindings, not to change application/layout state.
    /// Capture owned data or application-lived bindings, not Scene-local signals.
    pub fn show_with(&self, kind: ToastKind, content: impl Fn() -> ToastContent + 'static) {
        if let Some(inbox) = self.0.upgrade() {
            inbox.borrow_mut().push_back(Pending {
                kind,
                content: Rc::new(content),
            });
        }
    }
}

/// Colors for the card surface and its two text styles. Tone colors are kept
/// distinct from the theme and appear in the countdown bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToastPalette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
}

impl Default for ToastPalette {
    fn default() -> Self {
        Self {
            background: Color::from_rgba(28, 34, 48, 255),
            foreground: Color::WHITE,
            muted: Color::from_rgba(172, 182, 201, 255),
        }
    }
}

/// Upper-right stack displaying at most three notices. Each notice starts its
/// two-second countdown when it gets a slot, then slides out. Install this as
/// an [`mue_macroquad::App::set_overlay_root`] tree to keep it across Scenes.
/// The input policy belongs to the mounting root. Timers use host time, not
/// Scene time: already visible messages can expire during host suspension.
pub struct ToastHost {
    inbox: Rc<Inbox>,
    palette: Prop<ToastPalette>,
    style: Style,
}

/// Construct a reusable host and its independent sending handle.
///
/// ```no_run
/// use mue_components::{ToastKind, toast_host};
/// use mue_macroquad::App;
///
/// fn install(app: &mut App) {
///     let (host, toasts) = toast_host();
///     app.set_overlay_root(host);
///     toasts.show(ToastKind::Success, "Library ready");
/// }
/// ```
pub fn toast_host() -> (ToastHost, ToastSender) {
    let inbox = Rc::new(Inbox::default());
    let sender = ToastSender(Rc::downgrade(&inbox));
    (
        ToastHost {
            inbox,
            palette: Prop::default(),
            style: Style::new(),
        },
        sender,
    )
}

impl ToastHost {
    pub fn palette(mut self, palette: impl Into<Prop<ToastPalette>>) -> Self {
        self.palette = palette.into();
        self
    }
}

impl Styleable for ToastHost {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ToastHost {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            inbox,
            palette,
            style,
        } = self;
        ctx.push_style(style);
        let visible = Signal::new_always(Rc::<[Toast]>::from([]));
        let time = signal(0.);
        let mut next_id = 0;
        let column = stack()
            .width(WIDTH)
            .max_width(percent(100.))
            .min_width(0.)
            .children(map_keyed(
                visible,
                |toast: &Toast| toast.id,
                move |toast| toast_card(toast, *time, palette),
            ));
        flexbox()
            .style(style! { width: 100%; height: 100%; justify-content: flex-end; align-items: flex-start; })
            .padding((MARGIN,))
            .on_update(move |_| {
                update(&inbox, visible, time, &mut next_id, macroquad::time::get_time());
            })
            .children(column)
            .build(ctx)
    }
}

#[derive(Clone)]
struct Toast {
    id: u64,
    started: f64,
    notice: Pending,
}

fn update(
    inbox: &Inbox,
    visible: Signal<Rc<[Toast]>>,
    time: Signal<f64>,
    next_id: &mut u64,
    now: f64,
) {
    let visible_now = visible.get_clone_untracked();
    let live = visible_now
        .iter()
        .filter(|toast| now < toast.started + HOLD + EXIT);
    let live_count = live.clone().count();
    let added = inbox.borrow().len().min(MAX_VISIBLE - live_count);
    if added != 0 || live_count != visible_now.len() {
        let next = live
            .cloned()
            .chain(inbox.borrow_mut().drain(..added).map(|notice| {
                let toast = Toast {
                    id: *next_id,
                    started: now,
                    notice,
                };
                *next_id += 1;
                toast
            }))
            .collect();
        visible.set(next);
        time.set(now);
    } else if live_count > 0 {
        time.set(now);
    }
}

fn toast_card(
    toast: &Toast,
    time: ReadSignal<f64>,
    palette: Prop<ToastPalette>,
) -> impl Component + use<> {
    let ToastContent { label, message } = (toast.notice.content)();
    let started = toast.started;
    let age = time.map(move |now| now - started);
    let slide = age.map(|age| {
        let entering = (1. - age / ENTER).clamp(0., 1.);
        let exiting = ((age - HOLD) / EXIT).clamp(0., 1.);
        let offset = (WIDTH + MARGIN) * (entering.powi(3) + exiting.powi(3)) as f32;
        Matrix::from_translation(vec2(offset, 0.))
    });
    let remaining = age.map(|age| {
        let fraction = (1. - age / HOLD).clamp(0., 1.) as f32;
        Matrix::from_scale(vec2(fraction, 1.))
    });
    stack()
        .style(style! { width: 100%; flex-shrink: 0; padding: 10px 14px 12px; gap: 4px; border-radius: 12px; })
        .background(palette.map(|p| Brush::Solid(p.background)))
        .transform(slide)
        .children((
            label.into_iter().map(|label| text(label)
                .font_size(12.)
                .line_height(16.)
                .color(palette.map(|p| Brush::Solid(p.muted))))
                .collect::<Vec<_>>(),
            text(message)
                .style(style! { font-size: 16px; line-height: 22px; white-space: nowrap; text-overflow: ellipsis; })
                .color(palette.map(|p| Brush::Solid(p.foreground))),
            flexbox()
                .style(style! { width: 100%; height: 2px; margin-top: 4px; transform-origin: left center; })
                .background(toast.notice.kind.accent())
                .transform(remaining),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mue_core::scope::OwnedScope;
    use std::cell::Cell;

    #[test]
    fn pending_toasts_keep_their_order_and_start_when_visible() {
        let scope = OwnedScope::new();
        let (visible, time) =
            scope.run(|| (Signal::new_always(Rc::<[Toast]>::from([])), signal(0.)));
        let (host, sender) = toast_host();
        for label in ["first", "second", "third", "fourth"] {
            sender.show(ToastKind::Info, label);
        }
        let mut next_id = 0;
        update(&host.inbox, visible, time, &mut next_id, 10.);
        let first = visible.get_clone();
        assert_eq!(
            first.iter().map(|toast| toast.id).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(host.inbox.borrow().len(), 1);

        update(&host.inbox, visible, time, &mut next_id, 12.1);
        assert_eq!(visible.get_clone().len(), 3); // Exit animation still owns the slots.
        update(&host.inbox, visible, time, &mut next_id, 12.5);
        let next = visible.get_clone();
        assert_eq!(next.len(), 1);
        assert_eq!((next[0].id, next[0].started), (3, 12.5));
        assert_eq!(
            (next[0].notice.content)().message.get_clone().as_ref(),
            "fourth"
        );
    }

    #[test]
    fn sender_does_not_keep_a_removed_host_or_pending_content_alive() {
        let (host, sender) = toast_host();
        let content_owner = Rc::new(Cell::new(0));
        sender.show_with(ToastKind::Warning, {
            let owner = content_owner.clone();
            move || {
                owner.set(1);
                ToastContent::new("queued")
            }
        });
        assert_eq!(Rc::strong_count(&content_owner), 2);
        drop(host);
        assert_eq!(Rc::strong_count(&content_owner), 1);
        assert_eq!(content_owner.get(), 0);
        sender.show(ToastKind::Info, "ignored");
    }
}
