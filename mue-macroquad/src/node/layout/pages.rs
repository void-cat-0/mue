//! Ordered, retained pages selected by index, independent of navigation paths.
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PagesTransition {
    None,
    #[default]
    Slide,
    SlideVertical,
}

pub struct PagesBuilder<I> {
    selected: Prop<usize>,
    pages: I,
    style: Style,
    transition: PagesTransition,
    curve: Animation,
}

/// Display a fixed list of pages, retaining every page's state until disposal.
/// Increasing the index slides left (up vertically); decreasing it reverses direction.
/// Jumps pass through intermediate pages. Out-of-range indices select the last page;
/// an empty list displays nothing. The supplied selection is never modified.
pub fn pages<I>(selected: impl Into<Prop<usize>>, pages: I) -> PagesBuilder<I>
where
    I: IntoIterator,
    I::Item: IntoNode,
{
    PagesBuilder {
        selected: selected.into(),
        pages,
        style: Style::new(),
        transition: PagesTransition::Slide,
        curve: Animation::spring(0.45, 0.12),
    }
}

impl<I> PagesBuilder<I> {
    pub fn transition(mut self, transition: PagesTransition) -> Self {
        self.transition = transition;
        self
    }

    /// Configure timing; slide progress is bounded to keep the viewport covered.
    pub fn transition_animation(mut self, curve: Animation) -> Self {
        self.curve = curve;
        self
    }
}

impl<I> Styleable for PagesBuilder<I> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    fn children(self, _: impl crate::node::IntoChildren) -> Self {
        panic!("pages content is supplied through the fixed page list")
    }
}

struct Motion {
    from: f32,
    elapsed: f32,
}

struct Presentation {
    pages: Vec<Node>,
    selected: Option<usize>,
    position: Signal<f32>,
    size: Signal<Vector>,
    visible: Signal<Rc<[NodeRef]>>,
    interactive: Signal<Option<NodeRef>>,
    transition: PagesTransition,
    curve: Animation,
    motion: Option<Motion>,
}

impl Presentation {
    fn publish(&self) {
        let mut visible = Vec::new();
        if !self.pages.is_empty() {
            let position = self.position.get_untracked();
            let first = position.floor() as usize;
            let last = (position.ceil() as usize).min(self.pages.len() - 1);
            for index in first..=last {
                visible.push(*self.pages[index]);
            }
        }
        let interactive = self.selected.and_then(|index| {
            let page = *self.pages[index];
            visible.contains(&page).then_some(page)
        });
        self.visible.set(visible.into());
        self.interactive.set(interactive);
    }

    fn settle(&mut self) {
        self.motion = None;
        self.position.set(self.selected.unwrap_or(0) as f32);
        self.publish();
    }

    fn select(&mut self, index: usize) {
        let selected = self.pages.len().checked_sub(1).map(|last| index.min(last));
        if self.selected == selected {
            return;
        }
        self.selected = selected;
        let size = self.size.get_untracked();
        if self.transition == PagesTransition::None
            || !self.curve.duration().is_finite()
            || self.curve.duration() <= 0.
            || size.x <= 0.
            || size.y <= 0.
        {
            self.settle();
        } else {
            self.motion = Some(Motion {
                from: self.position.get_untracked(),
                elapsed: 0.,
            });
            self.publish();
        }
    }

    fn resize(&mut self, size: Vector) -> bool {
        if self.size.get_untracked() == size {
            return false;
        }
        self.size.set(size);
        self.settle();
        true
    }

    fn advance(&mut self, dt: f32) {
        let Some(motion) = self.motion.as_mut() else {
            return;
        };
        motion.elapsed = (motion.elapsed + dt).min(self.curve.duration());
        let progress = motion.elapsed / self.curve.duration();
        if progress >= 1. {
            self.settle();
        } else {
            // A single bounded coordinate keeps neighboring pages adjacent, even on reversal.
            let sample = self.curve.sample(progress).clamp(0., 1.);
            let to = self.selected.unwrap_or(0) as f32;
            self.position.set(motion.from * (1. - sample) + to * sample);
            self.publish();
        }
    }
}

impl Driver for RefCell<Presentation> {
    fn advance(&self, dt: f32) {
        self.borrow_mut().advance(dt);
    }
}

impl<I> IntoNode for PagesBuilder<I>
where
    I: IntoIterator,
    I::Item: IntoNode,
{
    fn into_node(self) -> Node {
        let Self {
            selected,
            pages,
            mut style,
            transition,
            curve,
        } = self;
        style.provide_defaults(Style::new().size_full());
        Node::build_with_style(style, move |style| {
            let position = signal(0.);
            let size = signal(Vector::ZERO);
            let visible = signal(Rc::<[NodeRef]>::default());
            let interactive = signal(None::<NodeRef>);
            let pages: Vec<_> = pages
                .into_iter()
                .enumerate()
                .map(|(index, page)| {
                    let offset = computed(move |_| {
                        let distance = index as f32 - position.get();
                        let size = size.get();
                        if transition == PagesTransition::SlideVertical {
                            Vector::new(0., distance * size.y)
                        } else {
                            Vector::new(distance * size.x, 0.)
                        }
                    });
                    flexbox()
                        .absolute()
                        .width(computed(move |_| size.get().x))
                        .height(computed(move |_| size.get().y))
                        .inset_left(computed(move |_| offset.get().x))
                        .inset_top(computed(move |_| offset.get().y))
                        .children(page.into_node())
                        .into_node()
                })
                .collect();
            let initial = pages
                .len()
                .checked_sub(1)
                .map(|last| selected.get_untracked().min(last));
            let presentation = Rc::new(RefCell::new(Presentation {
                pages,
                selected: initial,
                position,
                size,
                visible,
                interactive,
                transition,
                curve,
                motion: None,
            }));
            presentation.borrow_mut().settle();
            let driver: Rc<dyn Driver> = presentation.clone();
            animation::register_driver(&driver);
            if let Prop::Dynamic(selected) = selected {
                let presentation = presentation.clone();
                watch(selected, move |selected| {
                    presentation.borrow_mut().select(selected.get_untracked());
                });
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
                    if let Some(page) = interactive.get_untracked() {
                        crate::event::hit_test::do_hit_test(page, pos, result);
                    }
                    result.insert(node);
                }));
            });
            use_background(style, layout.rect);
        })
    }
}
