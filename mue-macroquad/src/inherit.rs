//! Explicit inherited properties for a subtree.

use mue_core::{
    prop::Prop,
    scope::{OwnedScope, inject},
};

use crate::{
    node::{IntoNode, Node},
    shader::Brush,
};

#[derive(Clone)]
struct Foreground(Prop<Brush>);
/// Build a subtree with an inherited foreground, using the ordinary scope context.
pub fn provide_foreground<T>(
    color: impl Into<Prop<Brush>>,
    view: impl FnOnce() -> T,
) -> ForegroundProvider<impl FnOnce() -> T> {
    ForegroundProvider {
        color: color.into(),
        view,
    }
}
pub struct ForegroundProvider<F> {
    color: Prop<Brush>,
    view: F,
}
pub fn inherited_foreground() -> Option<Prop<Brush>> {
    inject::<Foreground>().map(|foreground| foreground.0)
}
impl<F, R> IntoNode for ForegroundProvider<F>
where
    F: FnOnce() -> R,
    R: IntoNode,
{
    fn into_node(self) -> Node {
        let scope = OwnedScope::new_inherited();
        scope.provide(Foreground(self.color));
        let node = scope.run(|| (self.view)().into_node());
        node.retain_owned_scope(scope);
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::Style;
    use mue_core::{
        runtime::resource_counts,
        scope::{OwnedScope, current_scope},
        signal::{Access, signal},
    };
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn foreground_is_inherited_reactively_and_released_with_its_node() {
        let before = resource_counts();
        let source_owner = OwnedScope::new();
        let foreground = source_owner.run(|| signal(Brush::from(Color::WHITE)));
        let observed = Rc::new(RefCell::new(None));
        let capture = observed.clone();
        let node_owner = OwnedScope::new();
        let node = node_owner.run(|| {
            provide_foreground(foreground, move || {
                Node::leaf(Style::new(), move |_| {
                    *capture.borrow_mut() = inherited_foreground();
                })
            })
            .into_node()
        });

        let read = observed.borrow().as_ref().unwrap().clone();
        assert_eq!(read.get_clone(), Brush::from(Color::WHITE));
        foreground.set(Brush::from(Color::BLACK));
        assert_eq!(read.get_clone(), Brush::from(Color::BLACK));
        assert!(current_scope().inject::<Foreground>().is_none());

        drop(node);
        drop(node_owner);
        assert!(!foreground.is_disposed());
        drop(source_owner);
        assert_eq!(resource_counts(), before);
    }

    #[test]
    fn nested_foreground_provider_shadows_the_outer_value() {
        let before = resource_counts();
        let owner = OwnedScope::new();
        owner.run(|| {
            let nested = provide_foreground(Brush::from(Color::BLACK), || {
                assert_eq!(
                    inherited_foreground().unwrap().get_clone(),
                    Brush::from(Color::BLACK)
                );
                Node::empty()
            });
            let outer = provide_foreground(Brush::from(Color::WHITE), || nested.into_node());
            let node = outer.into_node();
            drop(node);
        });
        drop(owner);
        assert_eq!(resource_counts(), before);
    }
}
