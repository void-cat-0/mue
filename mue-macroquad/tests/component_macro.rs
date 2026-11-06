//! Cross-crate regression coverage for synchronous generated builders.
//! These nodes mount without graphics resources, so no window is required.

use std::{any::TypeId, cell::Cell};

use mue_core::{prop::Prop, scope::OwnedScope, signal::Access};
use mue_macroquad::{
    node::{BuildNode, IntoNode, Node, NodeContext},
    style::Style,
};

struct NotClone;

#[mue_macros::node]
fn plain<T>(cx: NodeContext, payload: T) -> Node {
    cx.finish(Style::new(), move |_| {
        drop(payload);
    })
}

#[mue_macros::node]
pub fn marker<'a, T: ?Sized, const N: usize>(ctx: NodeContext) -> Node
where
    T: 'a,
{
    ctx.finish(Style::new(), |_| {})
}

#[mue_macros::node]
pub fn configured<T>(
    other: NodeContext,
    payload: T,
    #[default] active: bool,
    #[model] count: u32,
) -> Node {
    other.finish(Style::new(), move |_| {
        drop(payload);
        assert!(!active.get());
        assert_eq!(count.get(), 7);
    })
}

#[mue_macros::component]
fn wrapped<T, const N: usize>() -> MarkerBuilder<'static, T, N>
where
    T: 'static,
{
    marker::<T, N>()
}

#[mue_macros::node]
fn text(cx: NodeContext, text: String) -> Node {
    cx.finish(Style::new(), move |_| {
        assert_eq!(text.get_clone(), "text");
    })
}

thread_local! {
    static DEFAULTS: Cell<usize> = const { Cell::new(0) };
    static BODIES: Cell<usize> = const { Cell::new(0) };
    static INITS: Cell<usize> = const { Cell::new(0) };
    static COMPONENTS: Cell<usize> = const { Cell::new(0) };
}

fn reset_counts() {
    DEFAULTS.with(|count| count.set(0));
    BODIES.with(|count| count.set(0));
    INITS.with(|count| count.set(0));
    COMPONENTS.with(|count| count.set(0));
}
fn counts() -> (usize, usize, usize, usize) {
    (
        DEFAULTS.with(Cell::get),
        BODIES.with(Cell::get),
        INITS.with(Cell::get),
        COMPONENTS.with(Cell::get),
    )
}
fn next_default() -> u32 {
    DEFAULTS.with(|count| count.set(count.get() + 1));
    19
}

#[mue_macros::node]
fn once(mut ctx: NodeContext, #[default(next_default())] value: u32) -> Node {
    BODIES.with(|count| count.set(count.get() + 1));
    let owner = &mut ctx;
    let _ = owner;
    assert_eq!(value.get(), 19);
    let label = String::from("borrowed from the node body");
    let mut initialized = false;
    let node = ctx.finish(Style::new(), |style| {
        let _ = style;
        assert_eq!(label.as_str(), "borrowed from the node body");
        drop(label);
        initialized = true;
        INITS.with(|count| count.set(count.get() + 1));
    });
    assert!(initialized);
    node
}

#[mue_macros::component]
fn wrapped_once() -> OnceBuilder {
    COMPONENTS.with(|count| count.set(count.get() + 1));
    once()
}

mod hidden {
    use super::*;

    // Private declarations still generate usable builders.
    #[mue_macros::node]
    fn local(other: NodeContext, #[default] flag: bool) -> Node {
        other.finish(Style::new(), move |_| {
            assert!(!flag.get());
        })
    }

    pub fn check() {
        drop(local().into_node());
    }
}

#[test]
fn preserves_unused_lifetime_type_and_const_identity_without_clone_bounds() {
    fn assert_builder<T: BuildNode>() {}
    assert_builder::<Marker<'static, NotClone, 4>>();
    assert_builder::<Marker<'static, str, 8>>();
    assert_ne!(
        TypeId::of::<Marker<'static, NotClone, 4>>(),
        TypeId::of::<Marker<'static, NotClone, 5>>(),
    );
    assert_eq!(
        TypeId::of::<Marker<'static, NotClone, 4>>(),
        TypeId::of::<MarkerBuilder<'static, NotClone, 4>>(),
    );
    let builder: Marker<'static, NotClone, 4> = marker::<NotClone, 4>();
    drop(builder.into_node());
    let builder: Wrapped<NotClone, 4> = wrapped::<NotClone, 4>();
    drop(builder.into_node());
    hidden::check();
}

#[test]
fn arguments_need_neither_clone_nor_static_for_synchronous_initialization() {
    let borrowed = NotClone;
    let builder: Plain<&NotClone> = plain(Prop::Static(&borrowed));
    drop(builder.into_node());
}

#[test]
fn constructor_and_setters_exclude_injected_build_and_trait_method_builds_a_node() {
    let payload = NotClone;
    let builder = PlainBuilder::new(Prop::Static(&payload)).payload(Prop::Static(&payload));
    let node: Node = builder.build(NodeContext::new());
    drop(node);
    let node: Node = MarkerBuilder::<'static, NotClone, 3>::new().build(NodeContext::new());
    drop(node);
}

#[test]
fn defaults_body_component_and_init_run_once_during_into_node() {
    reset_counts();
    let builder = wrapped_once();
    assert_eq!(counts(), (0, 0, 0, 0));
    let node = builder.into_node();
    assert_eq!(counts(), (1, 1, 1, 1));
    drop(node);
    assert_eq!(counts(), (1, 1, 1, 1));
}

#[test]
fn dropping_unbuilt_builders_does_not_evaluate_defaults_or_execute_bodies() {
    reset_counts();
    drop(wrapped_once());
    drop(once().value(Prop::Static(19)));
    assert_eq!(counts(), (0, 0, 0, 0));
}

#[test]
fn explicit_option_skips_default_and_initializes_synchronously() {
    reset_counts();
    let builder = once().value(Prop::Static(19));
    assert_eq!(counts(), (0, 0, 0, 0));
    drop(builder.into_node());
    assert_eq!(counts(), (0, 1, 1, 0));
}

#[test]
fn nonclone_payload_models_and_string_arguments_compile_and_build() {
    let scope = OwnedScope::new_inherited();
    scope.run(|| {
        let count = mue_core::signal::signal(7_u32);
        let builder = configured(Prop::Static(NotClone), count);
        drop(builder.into_node());
        assert!(!count.is_disposed());
        drop(text("text").into_node());
    });
}
