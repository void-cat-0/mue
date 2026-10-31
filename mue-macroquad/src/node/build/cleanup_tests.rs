use super::*;
use crate::{
    layout::use_layout,
    node::{self, IntoNode, NodePlan},
    runtime::Runtime,
    style::{Size, Styleable, StyleableExt},
    theme::{Rule, Selector, Specificity, Themeable},
};
use mue_core::{effect::computed, runtime::resource_counts, signal::Access};
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[mue_macros::node]
#[theme]
fn checked_leaf() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().width(12.), |style| {
        use_layout(style);
    })
}

fn ui_counts() -> (usize, usize, usize, usize) {
    Runtime::with(|rt| {
        (
            rt.nodes.borrow().len(),
            rt.gestures.borrow().len(),
            rt.taffy.borrow().total_node_count(),
            node::CONTEXT.with_borrow(Vec::len),
        )
    })
}

#[test]
fn every_failed_build_stage_restores_node_layout_gestures_and_core_resources() {
    for stage in 0..5 {
        let before = resource_counts();
        let ui_before = ui_counts();
        let failed = catch_unwind(AssertUnwindSafe(|| match stage {
            0 => {
                let _ = Style::new().with_hovered(|_, hovered| {
                    let _ = hovered.map(|value| !value);
                    panic!("configuration");
                });
            }
            1 => {
                let appearance = Theme::new(vec![Rule::new(
                    vec![Selector::new(Specificity(0, 0, 1), |_| panic!("selector"))],
                    Style::new,
                )]);
                AppBuildContext::with_theme(appearance.into(), || {
                    let _ = checked_leaf().into_node();
                });
            }
            2 => {
                let appearance = Theme::new(vec![Rule::new(
                    vec![Selector::new(Specificity(0, 0, 1), |_| true)],
                    || {
                        let _ = signal(42);
                        panic!("rule body");
                    },
                )]);
                AppBuildContext::with_theme(appearance.into(), || {
                    let _ = checked_leaf().into_node();
                });
            }
            3 => {
                let child = NodePlan::new(Style::new(), |style| {
                    use_layout(style);
                    panic!("child mount");
                });
                let _ = node::flexbox().children(child).into_node();
            }
            _ => {
                let _ = NodePlan::new(Style::new(), |style| {
                    use_layout(style);
                    node::add_gesture(crate::gesture::TapGesture::default());
                    panic!("parent mount");
                })
                .into_node();
            }
        }));
        assert!(failed.is_err());
        assert_eq!(resource_counts(), before, "core stage {stage}");
        assert_eq!(ui_counts(), ui_before, "UI stage {stage}");
        assert!(!RootBuild::new().has_theme());
        let next = checked_leaf().into_node();
        drop(next);
        assert_eq!(resource_counts(), before);
        assert_eq!(ui_counts(), ui_before);
    }
}

#[test]
fn unmounted_prepared_node_disposes_sealed_graph_and_hover_resources() {
    let before = resource_counts();
    let ui_before = ui_counts();
    let prepared = AppBuildContext::with_theme(Theme::default().into(), || {
        checked_leaf().enable_hovered().prepare(RootBuild::new())
    });
    assert!(resource_counts().signals > before.signals);
    assert_eq!(ui_counts(), ui_before);
    drop(prepared);
    assert_eq!(resource_counts(), before);
    assert_eq!(ui_counts(), ui_before);
}

#[test]
fn rule_created_derived_signals_are_bounded_and_use_the_node_context() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.provide(17u32);
    let input = owner.run(|| signal(1));
    let values = Rc::new(RefCell::new(Vec::new()));
    let captured = values.clone();
    let appearance = Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |_| true)],
        move || {
            assert_eq!(mue_core::scope::current_scope().inject::<u32>(), Some(17));
            let value = computed(move |_| Size::Px(input.get() as f32));
            captured.borrow_mut().push(value);
            Style::new().width(value)
        },
    )]);
    let node =
        owner.run(|| AppBuildContext::with_theme(appearance.into(), || checked_leaf().into_node()));
    let stable = resource_counts();
    // The rule expression can read a Signal directly and allocate a derived Prop.
    // Only its active generation may survive later dynamic Theme replacement.
    let appearance_source = owner.run(|| signal(Theme::default()));
    drop(node);
    let runs = Rc::new(Cell::new(0));
    let recorded = runs.clone();
    let themed = Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |_| true)],
        move || {
            recorded.set(recorded.get() + 1);
            let _ = signal(0);
            Style::new().width(14.)
        },
    )]);
    let node = owner.run(|| {
        AppBuildContext::with_theme(appearance_source.into(), || checked_leaf().into_node())
    });
    appearance_source.set(themed.clone());
    let active = resource_counts();
    for _ in 0..32 {
        appearance_source.set(Theme::default());
        appearance_source.set(themed.clone());
        assert_eq!(resource_counts(), active);
    }
    assert_eq!(runs.get(), 33);
    assert!(values.borrow().iter().all(|value| value.is_disposed()));
    let _ = stable;
    drop(node);
    drop(owner);
    assert_eq!(resource_counts(), before);
}
