//! Explicit inherited properties for a subtree.

use mue_core::{
    prop::Prop,
    scope::{OwnedScope, inject},
};

use crate::{
    node::{IntoNode, Node},
    paint::shader::Brush,
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
