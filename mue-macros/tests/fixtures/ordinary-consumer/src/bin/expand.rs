use std::sync::atomic::{AtomicUsize, Ordering};

use ui::__private::core::{prop::Prop, signal::signal};
use ui::node::{IntoNode, Node, NodeContext, assert_in_scope};
use ui::style::{Style, Styleable};
use ui::{component, node, style};

static DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
static MODEL_DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
static COMPONENT_DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_CALLS: AtomicUsize = AtomicUsize::new(0);
static COMPONENT_CALLS: AtomicUsize = AtomicUsize::new(0);
static INIT_CALLS: AtomicUsize = AtomicUsize::new(0);

fn default_amount() -> u32 {
    assert_in_scope();
    DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
    7
}

fn default_model() -> u32 {
    assert_in_scope();
    MODEL_DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
    1
}

fn default_outer() -> bool {
    assert_in_scope();
    COMPONENT_DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
    false
}

#[node]
fn plain<'a, T, const N: usize>(
    cx: ui::node::ContextAlias,
    value: T,
    label: &'a str,
    #[default(default_amount())] amount: u32,
    #[model]
    #[default(default_model())]
    model: u32,
    #[default] active: bool,
) -> Node {
    assert_in_scope();
    ORIGINAL_CALLS.fetch_add(1, Ordering::SeqCst);
    let defaults = Style {
        opacity: Some(0.2),
        ..Style::new()
    };
    cx.finish(defaults, move |style| {
        let Prop::Static(label) = label;
        let Prop::Static(amount) = amount;
        let Prop::Static(active) = active;
        INIT_CALLS.fetch_add(1, Ordering::SeqCst);
        // Generic non-Clone values and borrowed arguments survive explicit mount.
        drop(value);
        assert!(!label.is_empty());
        let _ = (N, active);
        let expected_opacity = if N == 4 {
            if active { 0.5 } else { 0.8 }
        } else {
            0.2
        };
        assert_eq!(style.opacity, Some(expected_opacity));
        style.value = amount + model.0;
    })
}

#[component]
fn wrapped<'a>(label: &'a str, #[default(default_outer())] active: bool) -> Plain<'a, (), 4> {
    assert_in_scope();
    COMPONENT_CALLS.fetch_add(1, Ordering::SeqCst);
    let Prop::Static(label) = label;
    let Prop::Static(active) = active;
    plain::<(), 4>((), label).active(active).opacity(0.5)
}

#[node]
fn generic<'a, T: ?Sized, const N: usize>(mut ctx: NodeContext) -> Node
where
    T: 'a,
{
    // Exercise a mutable injected binding and unused lifetime/type parameters.
    let owner = &mut ctx;
    let _ = owner;
    ctx.finish(Style::new(), |style| {
        style.value = N as u32;
    })
}

#[component]
fn generic_wrapped<'a, T: ?Sized, const N: usize>() -> Generic<'a, T, N>
where
    T: 'a,
{
    generic::<T, N>()
}

fn main() {
    let declarations = style! { opacity: 25%; };
    assert_eq!(declarations.opacity, Some(0.25));

    let label = String::from("borrowed locally");
    let pending = wrapped(label.as_str()).opacity(0.8);
    assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(COMPONENT_DEFAULT_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(ORIGINAL_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(COMPONENT_CALLS.load(Ordering::SeqCst), 0);
    let wrapped_node = pending.into_node();
    assert_eq!(wrapped_node.value, 8);
    assert_eq!(wrapped_node.opacity, Some(0.8));
    assert_eq!(wrapped_node.layers, 2);
    assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(MODEL_DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(COMPONENT_DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(COMPONENT_CALLS.load(Ordering::SeqCst), 1);

    struct NonClone;
    let node = Plain::<NonClone, 2>::new(NonClone, label.as_str())
        .value(NonClone)
        .label(label.as_str())
        .amount(3)
        .model(signal(9))
        .into_node();
    assert_eq!(node.value, 12);
    assert_eq!(node.opacity, Some(0.2));
    assert_eq!(node.layers, 1);
    assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(MODEL_DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(ORIGINAL_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(INIT_CALLS.load(Ordering::SeqCst), 2);

    let generic_node = generic_wrapped::<str, 11>().into_node();
    assert_eq!(generic_node.value, 11);
    assert_eq!(generic_node.layers, 2);

    let overridden = Wrapped::new(label.as_str()).active(true).into_node();
    assert_eq!(overridden.value, 8);
    assert_eq!(overridden.opacity, Some(0.5));
    assert_eq!(COMPONENT_DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(COMPONENT_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(ORIGINAL_CALLS.load(Ordering::SeqCst), 3);
    assert_eq!(INIT_CALLS.load(Ordering::SeqCst), 3);
}
