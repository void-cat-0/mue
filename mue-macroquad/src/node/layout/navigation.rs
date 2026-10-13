//! Value-driven navigation with retained screens and interruptible transitions.
use crate::{
    animation::Animation,
    layout::use_layout,
    math::Vector,
    node::{IntoNode, Node, NodeInner, NodeRef, flexbox},
    paint::use_background,
    style::{Style, Styleable, StyleableExt},
};
use mue_core::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationTransition {
    None,
    #[default]
    Slide,
    SlideVertical,
}

pub struct NavigationStackBuilder<T, R, F> {
    path: Prop<Vec<T>>,
    root: R,
    destination: F,
    split: Option<(f32, f32)>,
    style: Style,
    transition: NavigationTransition,
    curve: Animation,
}

/// Append/pop a typed path to navigate. Screens at equal path entries retain state.
/// Removed screens stay alive through their exit transition, then release their scope.
pub fn navigation_stack<T, R, F, Root, Destination>(
    path: impl Into<Prop<Vec<T>>>,
    root: R,
    destination: F,
) -> NavigationStackBuilder<T, R, F>
where
    T: Clone + PartialEq + 'static,
    R: FnOnce() -> Root,
    F: FnMut(&T) -> Destination + 'static,
    Root: IntoNode,
    Destination: IntoNode,
{
    NavigationStackBuilder {
        path: path.into(),
        root,
        destination,
        split: None,
        style: Style::new(),
        transition: NavigationTransition::Slide,
        curve: Animation::spring(0.45, 0.12),
    }
}

impl<T, R, F> NavigationStackBuilder<T, R, F> {
    pub fn split_view(mut self, sidebar_width: f32, breakpoint: f32) -> Self {
        assert!(sidebar_width.is_finite() && sidebar_width > 0.);
        assert!(breakpoint.is_finite() && breakpoint > sidebar_width);
        self.split = Some((sidebar_width, breakpoint));
        self
    }
    pub fn transition(mut self, transition: NavigationTransition) -> Self {
        self.transition = transition;
        self
    }
    pub fn transition_animation(mut self, curve: Animation) -> Self {
        self.curve = curve;
        self
    }
}
impl<T, R, F> Styleable for NavigationStackBuilder<T, R, F> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
    fn children(self, _: impl crate::node::IntoChildren) -> Self {
        panic!("navigation content is supplied through root and destination factories")
    }
}

struct Screen {
    node: Node,
    x: Signal<f32>,
    y: Signal<f32>,
}
impl Screen {
    fn new(
        build: impl FnOnce() -> Node,
        width: ReadSignal<f32>,
        left: ReadSignal<f32>,
    ) -> Rc<Self> {
        let scope = Scope::new();
        scope.run(|| {
            let x = signal(0.);
            let y = signal(0.);
            let node = flexbox()
                .absolute()
                .height(taffy::Dimension::percent(1.))
                .width(width)
                .inset_left(computed(move |_| left.get() + x.get()))
                .inset_top(*y)
                .children(build())
                .into_node();
            node.retain_scope(scope);
            Rc::new(Self { node, x, y })
        })
    }
    fn position(&self) -> Vector {
        Vector::new(self.x.get_untracked(), self.y.get_untracked())
    }
    fn set_position(&self, point: Vector) {
        self.x.set(point.x);
        self.y.set(point.y);
    }
}
struct Motion {
    screen: Rc<Screen>,
    from: Vector,
    to: Vector,
}
struct Transition {
    elapsed: f32,
    motions: Vec<Motion>,
}

impl<T, R, F, Root, Destination> IntoNode for NavigationStackBuilder<T, R, F>
where
    T: Clone + PartialEq + 'static,
    R: FnOnce() -> Root,
    F: FnMut(&T) -> Destination + 'static,
    Root: IntoNode,
    Destination: IntoNode,
{
    fn into_node(self) -> Node {
        let Self {
            path,
            root,
            mut destination,
            split,
            mut style,
            transition,
            curve,
        } = self;
        style.provide_defaults(Style::new().size_full());
        Node::build_with_style(style, move |style| {
            let scope = current_scope();
            let width = signal(0.);
            let height = signal(0.);
            let wide = signal(false);
            let sidebar = computed({
                let path = path.clone();
                move |_| {
                    if wide.get() && !path.get_clone().is_empty() {
                        split.map_or(0., |s| s.0)
                    } else {
                        0.
                    }
                }
            });
            let root_width = computed(move |_| {
                if sidebar.get() > 0. {
                    sidebar.get()
                } else {
                    width.get()
                }
            });
            let detail_width = computed(move |_| (width.get() - sidebar.get()).max(0.));
            let zero = signal(0.);
            let root = Screen::new(|| root().into_node(), root_width, *zero);
            let visible = signal(Rc::<[NodeRef]>::default());
            let settled = Rc::new(RefCell::new(Vec::<Rc<Screen>>::new()));
            let motion = Rc::new(RefCell::new(None::<Transition>));
            let interactive = signal(Rc::<[NodeRef]>::default());
            watch_effect({
                let settled = settled.clone();
                let motion = motion.clone();
                let root = root.clone();
                let mut screens: Vec<(T, Rc<Screen>)> = Vec::new();
                let mut previous = Vec::new();
                let mut previous_wide = false;
                move || {
                    let values = path.get_clone();
                    let is_wide = wide.get();
                    let common = screens
                        .iter()
                        .zip(&values)
                        .take_while(|((a, _), b)| a == *b)
                        .count();
                    let old_top = settled.borrow().last().cloned();
                    screens.truncate(common);
                    for value in &values[common..] {
                        let screen = scope.run(|| {
                            Screen::new(|| destination(value).into_node(), detail_width, sidebar)
                        });
                        screens.push((value.clone(), screen));
                    }
                    let top = screens
                        .last()
                        .map_or_else(|| root.clone(), |(_, screen)| screen.clone());
                    let mut next = Vec::new();
                    if is_wide && !screens.is_empty() {
                        next.push(root.clone());
                    }
                    next.push(top.clone());
                    let changed = old_top.as_ref().is_some_and(|old| !Rc::ptr_eq(old, &top));
                    let animate = changed
                        && previous_wide == is_wide
                        && transition != NavigationTransition::None
                        && curve.duration().is_finite()
                        && curve.duration() > 0.
                        && width.get_untracked() > 0.;
                    if animate {
                        let back = values.len() < previous.len();
                        let distance = if transition == NavigationTransition::SlideVertical {
                            height.get_untracked()
                        } else {
                            (width.get_untracked() - sidebar.get_untracked()).max(0.)
                        };
                        let vector = |value| {
                            if transition == NavigationTransition::SlideVertical {
                                Vector::new(0., value)
                            } else {
                                Vector::new(value, 0.)
                            }
                        };
                        let already_visible = motion
                            .borrow()
                            .as_ref()
                            .is_some_and(|t| t.motions.iter().any(|m| Rc::ptr_eq(&m.screen, &top)));
                        if is_wide && back {
                            top.set_position(Vector::ZERO);
                        } else if !already_visible {
                            top.set_position(vector(if back {
                                -distance * 0.22
                            } else {
                                distance
                            }));
                        }
                        let old = old_top.unwrap();
                        let mut motions = vec![Motion {
                            screen: top.clone(),
                            from: top.position(),
                            to: Vector::ZERO,
                        }];
                        // Only the front layer moves in split view. On a push the
                        // old detail stays behind it; on a pop the revealed page stays still.
                        if !next.iter().any(|s| Rc::ptr_eq(s, &old)) {
                            if is_wide && !back {
                                old.set_position(Vector::ZERO);
                            }
                            motions.push(Motion {
                                screen: old.clone(),
                                from: old.position(),
                                to: vector(if back {
                                    distance
                                } else if is_wide {
                                    0.
                                } else {
                                    -distance * 0.22
                                }),
                            });
                        }
                        let mut drawn = next.clone();
                        if motions.len() > 1 {
                            if back {
                                drawn.push(old);
                            } else {
                                drawn.insert(drawn.len() - 1, old);
                            }
                        }
                        visible.set(drawn.iter().map(|s| *s.node).collect());
                        *motion.borrow_mut() = Some(Transition {
                            elapsed: 0.,
                            motions,
                        });
                    } else {
                        *motion.borrow_mut() = None;
                        for screen in &next {
                            screen.set_position(Vector::ZERO);
                        }
                        visible.set(next.iter().map(|s| *s.node).collect());
                    }
                    interactive.set(next.iter().map(|s| *s.node).collect());
                    *settled.borrow_mut() = next;
                    previous = values;
                    previous_wide = is_wide;
                }
            });
            NodeInner::with_mut(|node| node.children = (*visible).owned());
            let layout = use_layout(style);
            NodeInner::with_mut(|node| {
                node.clip = Some(layout.rect);
                node.hit_test = Some(Rc::new(move |node, pos, result| {
                    if !layout.rect.get().contains(&pos) {
                        return;
                    }
                    for child in interactive.get_clone().iter().rev() {
                        crate::event::hit_test::do_hit_test(*child, pos, result);
                    }
                    result.insert(node);
                }));
            });
            use_background(style, layout.rect);
            style.on_render.append(move |_| {
                let rect = layout.rect.get();
                width.set(rect.w);
                height.set(rect.h);
                wide.set(split.is_some_and(|(_, breakpoint)| rect.w >= breakpoint));
                let mut transition = motion.borrow_mut();
                if let Some(active) = transition.as_mut() {
                    active.elapsed += macroquad::time::get_frame_time();
                    let progress = (active.elapsed / curve.duration()).min(1.);
                    for motion in &active.motions {
                        motion
                            .screen
                            .set_position(motion.from.lerp(motion.to, curve.sample(progress)));
                    }
                    if progress >= 1. {
                        visible.set(settled.borrow().iter().map(|s| *s.node).collect());
                        *transition = None;
                    }
                }
            });
        })
    }
}
