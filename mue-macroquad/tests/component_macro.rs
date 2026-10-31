//! Cross-crate regression coverage for generated builders. These tests prepare
//! plans without mounting, so they do not need a graphics window.

use std::{any::TypeId, cell::Cell, rc::Rc};

use mue_core::{prop::Prop, scope::OwnedScope, signal::Access};
use mue_macroquad::{
    node::{NodePlan, PrepareNode, RootBuild},
    style::Style,
    theme::ThemeTarget,
};

struct NotClone;

#[mue_macros::node]
fn plain<T>(payload: T) -> NodePlan<fn(&mut Style)> {
    drop(payload);
    NodePlan::new(Style::new(), |_| {})
}

#[mue_macros::node]
#[theme]
pub fn marker<'a, T: ?Sized, const N: usize>() -> NodePlan<fn(&mut Style)>
where
    T: 'a,
{
    NodePlan::new(Style::new(), |_| {})
}

#[mue_macros::node]
#[theme(active, count)]
pub fn configured<T>(
    payload: T,
    #[default] active: bool,
    #[model] count: u32,
) -> NodePlan<fn(&mut Style)> {
    drop(payload);
    assert!(!active.get());
    assert_eq!(count.get(), 7);
    NodePlan::new(Style::new(), |_| {})
}

#[mue_macros::component]
fn wrapped<T, const N: usize>() -> MarkerBuilder<'static, T, N>
where
    T: 'static,
{
    marker::<T, N>()
}

#[mue_macros::node]
fn text(text: String) -> NodePlan<fn(&mut Style)> {
    assert_eq!(text.get_clone(), "text");
    NodePlan::new(Style::new(), |_| {})
}

thread_local! {
    static DEFAULTS: Cell<usize> = const { Cell::new(0) };
    static BODIES: Cell<usize> = const { Cell::new(0) };
}

fn next_default() -> u32 {
    DEFAULTS.with(|count| count.set(count.get() + 1));
    19
}

#[mue_macros::node]
#[theme(value)]
fn once(#[default(next_default())] value: u32) -> NodePlan<fn(&mut Style)> {
    BODIES.with(|count| count.set(count.get() + 1));
    assert_eq!(value.get(), 19);
    NodePlan::new(Style::new(), |_| {})
}

mod hidden {
    use super::*;

    // Private declarations still expose public options accessors to selectors.
    #[mue_macros::node]
    #[theme(flag)]
    fn local(#[default] flag: bool) -> NodePlan<fn(&mut Style)> {
        let _ = flag;
        NodePlan::new(Style::new(), |_| {})
    }

    pub fn check() {
        fn assert_target<T: ThemeTarget>() {}
        assert_target::<Local>();
    }
}

#[test]
fn preserves_unused_lifetime_type_and_const_identity_without_clone_bounds() {
    fn assert_target<T: ThemeTarget<Options = ()>>() {}
    assert_target::<Marker<'static, NotClone, 4>>();
    assert_target::<Marker<'static, str, 8>>();
    assert_ne!(
        TypeId::of::<Marker<'static, NotClone, 4>>(),
        TypeId::of::<Marker<'static, NotClone, 5>>(),
    );
    assert_eq!(
        TypeId::of::<Marker<'static, NotClone, 4>>(),
        TypeId::of::<MarkerBuilder<'static, NotClone, 4>>(),
    );
    let _: Marker<'static, NotClone, 4> = marker::<NotClone, 4>();
    let _: Wrapped<NotClone, 4> = wrapped::<NotClone, 4>();
    hidden::check();
}

#[test]
fn non_themed_arguments_need_neither_clone_nor_static_when_plan_does_not_capture_them() {
    let borrowed = NotClone;
    let builder: Plain<&NotClone> = plain(Prop::Static(&borrowed));
    let prepared = builder.prepare(RootBuild::new());
    drop(prepared);
}

#[test]
fn defaults_and_original_body_run_once_during_prepare_not_construction() {
    DEFAULTS.with(|count| count.set(0));
    BODIES.with(|count| count.set(0));
    let builder = once();
    assert_eq!(DEFAULTS.with(Cell::get), 0);
    assert_eq!(BODIES.with(Cell::get), 0);
    let prepared = builder.prepare(RootBuild::new());
    assert_eq!(DEFAULTS.with(Cell::get), 1);
    assert_eq!(BODIES.with(Cell::get), 1);
    drop(prepared);
}

#[test]
fn nonlisted_nonclone_payload_and_listed_models_compile_and_prepare() {
    let scope = OwnedScope::new_inherited();
    scope.run(|| {
        let count = mue_core::signal::signal(7_u32);
        let builder = configured(Prop::Static(NotClone), count);
        drop(builder.prepare(RootBuild::new()));
        drop(text("text").prepare(RootBuild::new()));
    });
}

#[test]
fn options_have_public_owned_value_getters() {
    // Accessor signatures are checked from a different module than expansion.
    let _: fn(&ConfiguredOptions) -> bool = ConfiguredOptions::active;
    let _: fn(&ConfiguredOptions) -> u32 = ConfiguredOptions::count;
    let _: fn(&OnceOptions) -> u32 = OnceOptions::value;
    let _: Option<Rc<<Configured<NotClone> as ThemeTarget>::Options>> = None;
}
