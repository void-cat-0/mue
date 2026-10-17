//! Value-driven navigation with retained screens and interruptible transitions.
use crate::{
    animation::{self, Animation, Driver},
    layout::use_layout,
    math::Vector,
    node::{IntoNode, Node, NodeInner, NodeRef, flexbox},
    paint::use_background,
    style::{Style, Styleable, StyleableExt},
};
use mue_core::prelude::*;
use std::{cell::RefCell, rc::Rc};
const PARALLAX_RATIO: f32 = 0.22;

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
    /// Configure timing; slide progress is bounded to keep the viewport covered.
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
    width: Signal<f32>,
    left: Signal<f32>,
    position: Signal<Vector>,
}
impl Screen {
    fn new(build: impl FnOnce() -> Node) -> Rc<Self> {
        let scope = Scope::new();
        scope.run(|| {
            let width = signal(0.);
            let left = signal(0.);
            let position = signal(Vector::ZERO);
            let node = flexbox()
                .absolute()
                .height(taffy::Dimension::percent(1.))
                .width(width)
                .inset_left(computed(move |_| left.get() + position.get().x))
                .inset_top(computed(move |_| position.get().y))
                .children(build())
                .into_node();
            node.retain_scope(scope);
            Rc::new(Self {
                node,
                width,
                left,
                position,
            })
        })
    }
    fn position(&self) -> Vector {
        self.position.get_untracked()
    }
    fn set_position(&self, point: Vector) {
        self.position.set(point);
    }
    fn set_geometry(&self, left: f32, width: f32) {
        self.left.set(left);
        self.width.set(width);
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

struct Presentation {
    root: Rc<Screen>,
    stack: Vec<Rc<Screen>>,
    layers: Vec<Rc<Screen>>,
    visible: Signal<Rc<[NodeRef]>>,
    interactive: Signal<Rc<[NodeRef]>>,
    size: Vector,
    split: Option<(f32, f32)>,
    transition: NavigationTransition,
    curve: Animation,
    motion: Option<Transition>,
}

impl Presentation {
    fn wide(&self) -> bool {
        self.split
            .is_some_and(|(_, breakpoint)| self.size.x >= breakpoint)
    }

    fn sidebar(&self) -> f32 {
        if self.wide() && !self.stack.is_empty() {
            self.split.unwrap().0
        } else {
            0.
        }
    }

    fn top(&self) -> Rc<Screen> {
        self.stack.last().unwrap_or(&self.root).clone()
    }

    fn next(&self) -> Vec<Rc<Screen>> {
        let mut next = Vec::new();
        if self.wide() && !self.stack.is_empty() {
            next.push(self.root.clone());
        }
        next.push(self.top());
        next
    }

    fn set_geometry(&self) {
        let sidebar = self.sidebar();
        self.root
            .set_geometry(0., if sidebar > 0. { sidebar } else { self.size.x });
        if let Some(top) = self.stack.last() {
            top.set_geometry(sidebar, (self.size.x - sidebar).max(0.));
        }
    }

    fn publish(&self) {
        self.visible
            .set(self.layers.iter().map(|s| *s.node).collect());
        self.interactive
            .set(self.next().iter().map(|s| *s.node).collect());
    }

    fn settle(&mut self) {
        self.motion = None;
        self.set_geometry();
        self.layers = self.next();
        for screen in &self.layers {
            screen.set_position(Vector::ZERO);
        }
        self.publish();
    }

    fn resize(&mut self, size: Vector) -> bool {
        if self.size == size {
            return false;
        }
        self.size = size;
        self.settle();
        true
    }

    fn navigate(&mut self, stack: Vec<Rc<Screen>>, back: bool) {
        let old = self.top();
        self.stack = stack;
        let top = self.top();
        if Rc::ptr_eq(&old, &top) {
            return;
        }
        let animate = self.transition != NavigationTransition::None
            && self.curve.duration().is_finite()
            && self.curve.duration() > 0.
            && self.size.x > 0.
            && self.size.y > 0.;
        if !animate {
            self.settle();
            return;
        }
        if Rc::ptr_eq(&top, &self.root) {
            top.set_geometry(0., self.size.x);
        } else {
            let sidebar = self.sidebar();
            top.set_geometry(sidebar, (self.size.x - sidebar).max(0.));
        }
        let wide = self.wide();
        let distance = if self.transition == NavigationTransition::SlideVertical {
            self.size.y
        } else {
            (self.size.x - self.sidebar()).max(0.)
        };
        let vector = |value| {
            if self.transition == NavigationTransition::SlideVertical {
                Vector::new(0., value)
            } else {
                Vector::new(value, 0.)
            }
        };
        let index = self.layers.iter().position(|s| Rc::ptr_eq(s, &top));
        if let Some(index) = index {
            if !back {
                let screen = self.layers.remove(index);
                self.layers.push(screen);
            }
        } else {
            top.set_position(if wide && back {
                Vector::ZERO
            } else {
                vector(if back {
                    -distance * PARALLAX_RATIO
                } else {
                    distance
                })
            });
            if back {
                self.layers.insert(0, top.clone());
            } else {
                self.layers.push(top.clone());
            }
        }
        if wide {
            self.root.set_position(Vector::ZERO);
            self.layers.retain(|s| !Rc::ptr_eq(s, &self.root));
            self.layers.insert(0, self.root.clone());
        }
        let top_index = self
            .layers
            .iter()
            .position(|s| Rc::ptr_eq(s, &top))
            .unwrap();
        let motions = self
            .layers
            .iter()
            .enumerate()
            .map(|(index, screen)| {
                let previous_target = self.motion.as_ref().and_then(|t| {
                    t.motions
                        .iter()
                        .find(|m| Rc::ptr_eq(&m.screen, screen))
                        .map(|m| m.to)
                });
                let to = if Rc::ptr_eq(screen, &top) || (wide && Rc::ptr_eq(screen, &self.root)) {
                    Vector::ZERO
                } else if back && index > top_index {
                    vector(distance)
                } else if Rc::ptr_eq(screen, &old) {
                    vector(if wide { 0. } else { -distance * PARALLAX_RATIO })
                } else {
                    previous_target.unwrap_or_else(|| {
                        vector(if wide { 0. } else { -distance * PARALLAX_RATIO })
                    })
                };
                Motion {
                    screen: screen.clone(),
                    from: screen.position(),
                    to,
                }
            })
            .collect();
        self.motion = Some(Transition {
            elapsed: 0.,
            motions,
        });
        self.publish();
    }

    fn advance(&mut self, dt: f32) {
        let Some(active) = self.motion.as_mut() else {
            return;
        };
        active.elapsed = (active.elapsed + dt).min(self.curve.duration());
        let progress = active.elapsed / self.curve.duration();
        if progress >= 1. {
            self.settle();
        } else {
            // Overshooting a viewport-sized slide exposes the canvas at the opposite edge.
            let sample = self.curve.sample(progress).clamp(0., 1.);
            for motion in &active.motions {
                motion
                    .screen
                    .set_position(motion.from.lerp(motion.to, sample));
            }
        }
    }
}

impl Driver for RefCell<Presentation> {
    fn advance(&self, dt: f32) {
        self.borrow_mut().advance(dt);
    }
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
            let root = Screen::new(|| root().into_node());
            let visible = signal(Rc::<[NodeRef]>::default());
            let interactive = signal(Rc::<[NodeRef]>::default());
            let presentation = Rc::new(RefCell::new(Presentation {
                root,
                stack: Vec::new(),
                layers: Vec::new(),
                visible,
                interactive,
                size: Vector::ZERO,
                split,
                transition,
                curve,
                motion: None,
            }));
            presentation.borrow_mut().settle();
            let driver: Rc<dyn Driver> = presentation.clone();
            animation::register_driver(&driver);
            let mut screens: Vec<(T, Rc<Screen>)> = Vec::new();
            let mut update = {
                let presentation = presentation.clone();
                move |values: Vec<T>| {
                    let common = screens
                        .iter()
                        .zip(&values)
                        .take_while(|((a, _), b)| a == *b)
                        .count();
                    let back = values.len() < screens.len() && common == values.len();
                    screens.truncate(common);
                    for value in &values[common..] {
                        let screen = scope.run(|| Screen::new(|| destination(value).into_node()));
                        screens.push((value.clone(), screen));
                    }
                    presentation
                        .borrow_mut()
                        .navigate(screens.iter().map(|(_, s)| s.clone()).collect(), back);
                }
            };
            match path {
                Prop::Static(values) => update(values),
                Prop::Dynamic(path) => {
                    watch_immediate(path, move |path| update(path.get_clone_untracked()));
                }
            }
            NodeInner::with_mut(|node| {
                node.children = (*visible).owned();
                node.layout_sync =
                    Some(Rc::new(move |size| presentation.borrow_mut().resize(size)));
            });
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
        })
    }
}
