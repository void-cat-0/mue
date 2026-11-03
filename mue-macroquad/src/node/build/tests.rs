use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use mue_core::{prop::PropValue, scope::current_scope, signal::ReadSignal};

use super::*;
use crate::{
    node::Component,
    runtime::Runtime,
    style::{Size, Styleable, StyleableExt},
    theme::{Rule, Selector, Specificity},
};

#[derive(Default)]
struct Trace {
    prepares: usize,
    mounts: usize,
    width: Option<Prop<Size>>,
    height: Option<Prop<Size>>,
    resources: Vec<ReadSignal<i32>>,
    early_hover: Vec<ReadSignal<bool>>,
    aliases: Vec<ReadSignal<bool>>,
    defaults: usize,
    clones: usize,
    panic_prepare: bool,
    panic_mount: bool,
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

fn reset() {
    TRACE.set(Trace::default());
}
fn dimensions() -> (Size, Size) {
    TRACE.with_borrow(|trace| {
        (
            trace.width.as_ref().expect("width consumer").get(),
            trace.height.as_ref().expect("height consumer").get(),
        )
    })
}

#[mue_macros::node]
#[theme]
fn probe_leaf() -> NodePlan<impl FnOnce(&mut Style)> {
    TRACE.with_borrow_mut(|trace| {
        trace.prepares += 1;
        trace.resources.push(*signal(1));
        assert!(!trace.panic_prepare, "probe prepare panic");
    });
    NodePlan::new(Style::new().width(10.).height(20.), move |style| {
        TRACE.with_borrow_mut(|trace| {
            trace.mounts += 1;
            trace.width = Some(style.take_width());
            trace.height = Some(style.take_height());
            trace.resources.push(*signal(2));
            assert!(!trace.panic_mount, "probe mount panic");
        });
    })
}

#[mue_macros::component]
#[theme]
fn probe_outer() -> impl Component {
    probe_leaf().width(30.)
}

fn rule(width: f32) -> Theme {
    Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |target| {
            target.is::<ProbeLeaf>()
        })],
        move || Style::new().width(width),
    )])
}

#[test]
fn explicit_layers_precede_theme_then_prepare_fallback() {
    reset();
    let outer =
        AppBuildContext::with_theme(rule(40.).into(), || probe_outer().width(50.).into_node());
    assert_eq!(dimensions(), (Size::Px(50.), Size::Px(20.)));
    drop(outer);
    reset();
    let inner = AppBuildContext::with_theme(rule(40.).into(), || probe_outer().into_node());
    assert_eq!(dimensions(), (Size::Px(30.), Size::Px(20.)));
    drop(inner);
    reset();
    let themed = AppBuildContext::with_theme(rule(40.).into(), || probe_leaf().into_node());
    assert_eq!(dimensions(), (Size::Px(40.), Size::Px(20.)));
    drop(themed);
    reset();
    let fallback = probe_leaf().into_node();
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    drop(fallback);
}

#[test]
fn dynamic_theme_adds_and_removes_properties_without_rebuilding() {
    reset();
    let source_owner = OwnedScope::new();
    let theme = source_owner.run(|| signal(Theme::default()));
    let node = AppBuildContext::with_theme(theme.into(), || probe_leaf().into_node());
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    theme.set(rule(42.));
    assert_eq!(dimensions(), (Size::Px(42.), Size::Px(20.)));
    theme.set(Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |target| {
            target.is::<ProbeLeaf>()
        })],
        || Style::new().height(88.),
    )]));
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(88.)));
    theme.set(Theme::default());
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.prepares, 1);
        assert_eq!(trace.mounts, 1);
    });
    drop(node);
    assert!(!theme.is_disposed());
}

#[test]
fn empty_resolver_updates_skip_projections_but_keep_future_matches_and_generations() {
    reset();
    let baseline = mue_core::runtime::resource_counts();
    let owner = OwnedScope::new();
    let source = owner.run(|| Signal::shallow(Theme::default()));
    let derived = Rc::new(RefCell::new(Vec::new()));
    let empty_rule = {
        let derived = derived.clone();
        Theme::new(vec![Rule::new(
            vec![Selector::new(Specificity(0, 0, 1), |_| true)],
            move || {
                derived.borrow_mut().push(*signal(42));
                Style::new()
            },
        )])
    };
    let node = AppBuildContext::with_theme(source.into(), || probe_leaf().into_node());
    let initial = crate::runtime::theme_stats();
    source.set(empty_rule.clone());
    source.set(empty_rule);
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    let unchanged = crate::runtime::theme_stats();
    assert_eq!(unchanged.resolver_runs, initial.resolver_runs + 2);
    assert_eq!(unchanged.projection_runs, initial.projection_runs);
    assert!(derived.borrow()[0].is_disposed());
    assert!(!derived.borrow()[1].is_disposed());
    source.set(rule(47.));
    assert_eq!(dimensions().0, Size::Px(47.));
    assert!(derived.borrow()[1].is_disposed());
    source.set(Theme::default());
    assert_eq!(dimensions().0, Size::Px(10.));
    let removed = crate::runtime::theme_stats();
    source.set(Theme::default());
    assert_eq!(
        crate::runtime::theme_stats().projection_runs,
        removed.projection_runs
    );
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.prepares, 1);
        assert_eq!(trace.mounts, 1);
    });
    drop(node);
    drop(owner);
    assert_eq!(mue_core::runtime::resource_counts(), baseline);
}

#[test]
fn one_rule_matched_by_multiple_targets_runs_once_and_uses_matched_specificity() {
    reset();
    let runs = Rc::new(Cell::new(0));
    let capture = runs.clone();
    let theme = Theme::new(vec![
        Rule::new(
            vec![
                Selector::new(Specificity(0, 0, 1), |target| {
                    target.is::<ProbeLeaf>() || target.is::<ProbeOuter>()
                }),
                Selector::new(Specificity(0, 50, 1), |_| false),
            ],
            move || {
                capture.set(capture.get() + 1);
                Style::new().height(31.)
            },
        ),
        Rule::new(
            vec![Selector::new(Specificity(0, 1, 1), |target| {
                target.is::<ProbeLeaf>()
            })],
            || Style::new().height(41.),
        ),
    ]);
    let node = AppBuildContext::with_theme(theme.into(), || probe_outer().into_node());
    assert_eq!(runs.get(), 1);
    assert_eq!(dimensions().1, Size::Px(41.));
    drop(node);
}

#[test]
fn comma_selectors_use_only_the_highest_actually_matching_branch() {
    use crate::theme::Themeable;

    reset();
    let theme = mue_macros::theme! {
        ProbeLeaf, ProbeOuter.never {
            height: 31px;
        }
        ProbeLeaf.selected {
            height: 41px;
        }
    };
    let node = AppBuildContext::with_theme(theme.into(), || {
        probe_leaf()
            .classes(std::collections::HashSet::from([mue_macros::class!(
                "selected"
            )]))
            .into_node()
    });
    assert_eq!(dimensions().1, Size::Px(41.));
    drop(node);

    reset();
    let theme = mue_macros::theme! {
        ProbeLeaf, ProbeLeaf.selected {
            height: 51px;
        }
        ProbeLeaf {
            height: 61px;
        }
    };
    let node = AppBuildContext::with_theme(theme.into(), || {
        probe_leaf()
            .classes(std::collections::HashSet::from([mue_macros::class!(
                "selected"
            )]))
            .into_node()
    });
    assert_eq!(dimensions().1, Size::Px(51.));
    drop(node);
}

#[derive(PartialEq)]
struct CloneProbe(i32);
impl Clone for CloneProbe {
    fn clone(&self) -> Self {
        TRACE.with_borrow_mut(|trace| trace.clones += 1);
        Self(self.0)
    }
}
impl PropValue for CloneProbe {}
fn default_probe() -> CloneProbe {
    TRACE.with_borrow_mut(|trace| trace.defaults += 1);
    CloneProbe(7)
}

#[mue_macros::node]
#[theme(option)]
fn option_leaf(
    #[default(default_probe())] option: CloneProbe,
) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().width(10.), move |style| {
        let _ = &option;
        TRACE.with_borrow_mut(|trace| trace.width = Some(style.take_width()));
    })
}

#[test]
fn defaults_are_evaluated_once_and_no_theme_does_not_clone_options() {
    reset();
    let node = option_leaf().into_node();
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.defaults, 1);
        assert_eq!(trace.clones, 0);
    });
    drop(node);
    reset();
    let theme = Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 0, 1), |target| {
            target
                .options::<OptionLeaf>()
                .is_some_and(|options| options.option().0 == 7)
        })],
        || Style::new().width(71.),
    )]);
    let node = AppBuildContext::with_theme(theme.into(), || option_leaf().into_node());
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.defaults, 1);
        assert_eq!(trace.clones, 2);
        assert_eq!(trace.width.as_ref().unwrap().get(), Size::Px(71.));
    });
    drop(node);
}

#[mue_macros::component]
#[theme]
fn hover_outer() -> impl Component {
    probe_leaf().with_hovered(|builder, hovered| {
        TRACE.with_borrow_mut(|trace| trace.aliases.push(hovered));
        builder
    })
}

#[test]
fn early_hover_aliases_survive_configuration_scope_then_drop_with_the_node() {
    reset();
    let configuration = OwnedScope::new();
    let builder = configuration.run(|| {
        hover_outer().with_hovered(|builder, hovered| {
            TRACE.with_borrow_mut(|trace| trace.early_hover.push(hovered));
            let width = hovered.map(|value| if value { 73. } else { 13. });
            builder.width(width)
        })
    });
    TRACE.with_borrow(|trace| {
        assert_eq!(trace.early_hover.len(), 1);
        assert!(trace.aliases.is_empty());
        assert!(!trace.early_hover[0].get());
    });
    drop(configuration);
    let node = builder.into_node();
    assert_eq!(dimensions().0, Size::Px(13.));
    TRACE.with_borrow(|trace| {
        assert!(!trace.early_hover[0].is_disposed());
        assert!(!trace.aliases[0].is_disposed());
    });
    let aliases = Runtime::with(|rt| {
        let state = rt.nodes.borrow();
        let mounted = &state[node.id];
        std::iter::once(mounted.hovered.unwrap())
            .chain(mounted.hovered_aliases.iter().copied())
            .collect::<Vec<_>>()
    });
    for alias in aliases {
        alias.set(true);
    }
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].get());
        assert!(trace.aliases[0].get());
    });
    assert_eq!(dimensions().0, Size::Px(73.));
    drop(node);
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].is_disposed());
        assert!(trace.aliases[0].is_disposed());
    });
}

#[test]
fn unmounted_hover_builder_drop_releases_its_early_resources() {
    reset();
    let builder = probe_leaf().with_hovered(|builder, hovered| {
        TRACE.with_borrow_mut(|trace| trace.early_hover.push(hovered));
        builder
    });
    drop(builder);
    TRACE.with_borrow(|trace| {
        assert!(trace.early_hover[0].is_disposed());
        assert_eq!(trace.prepares, 0);
        assert_eq!(trace.mounts, 0);
    });
}

#[test]
fn prepare_and_mount_panics_release_owned_resources_and_restore_context() {
    for during_mount in [false, true] {
        reset();
        let parent = OwnedScope::new();
        parent.provide(17u32);
        let external = parent.run(|| signal(99));
        TRACE.with_borrow_mut(|trace| {
            trace.panic_prepare = !during_mount;
            trace.panic_mount = during_mount;
        });
        parent.run(|| {
            let failed = catch_unwind(AssertUnwindSafe(|| {
                AppBuildContext::with_theme(rule(40.).into(), || probe_leaf().into_node())
            }));
            assert!(failed.is_err());
            assert_eq!(current_scope().inject::<u32>(), Some(17));
            assert!(!RootBuild::new().has_theme());
            crate::node::CONTEXT.with_borrow(|stack| assert!(stack.is_empty()));
        });
        TRACE.with_borrow(|trace| {
            assert!(trace.resources.iter().all(|value| value.is_disposed()));
        });
        assert_eq!(external.get(), 99);
        assert!(!external.is_disposed());
    }
}

fn classes(names: &[&str]) -> std::collections::HashSet<ClassName> {
    names
        .iter()
        .map(|name| ClassName::try_from(*name).unwrap())
        .collect()
}

#[mue_macros::component]
#[theme]
fn class_outer() -> impl Component {
    use crate::theme::Themeable;
    probe_leaf().classes(classes(&["inner"]))
}

#[test]
fn compound_classes_and_identity_must_match_one_target() {
    use crate::theme::Themeable;

    reset();
    let theme = mue_macros::theme! {
        .outer.inner { height: 81px; }
        ClassOuter.inner { width: 82px; }
        ProbeLeaf.outer { height: 83px; }
    };
    let node = AppBuildContext::with_theme(theme.into(), || {
        class_outer().classes(classes(&["outer"])).into_node()
    });
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    drop(node);

    reset();
    let theme = mue_macros::theme! { ClassOuter.outer { height: 84px; } };
    let node = AppBuildContext::with_theme(theme.into(), || {
        class_outer().classes(classes(&["outer"])).into_node()
    });
    assert_eq!(dimensions().1, Size::Px(84.));
    drop(node);
}

#[test]
fn pure_class_rules_follow_the_replacement_dynamic_source_only() {
    use crate::theme::Themeable;

    reset();
    let owner = OwnedScope::new();
    let previous = owner.run(|| signal(classes(&["selected"])));
    let current = owner.run(|| signal(classes(&[])));
    let rules = Rc::new(Cell::new(0));
    let count = rules.clone();
    let theme = Theme::new(vec![Rule::new(
        vec![Selector::new(Specificity(0, 1, 0), |target| {
            target.has_class("selected")
        })],
        move || {
            count.set(count.get() + 1);
            Style::new().height(85.)
        },
    )]);
    let node = AppBuildContext::with_theme(theme.into(), || {
        probe_leaf().classes(previous).classes(current).into_node()
    });
    assert_eq!(dimensions().1, Size::Px(20.));
    assert_eq!(rules.get(), 0);
    current.set(classes(&["selected"]));
    assert_eq!(dimensions().1, Size::Px(85.));
    assert_eq!(rules.get(), 1);
    previous.set(classes(&[]));
    assert_eq!(dimensions().1, Size::Px(85.));
    assert_eq!(rules.get(), 1);
    current.set(classes(&[]));
    assert_eq!(dimensions().1, Size::Px(20.));
    assert_eq!(rules.get(), 1);
    drop(node);
    assert!(!previous.is_disposed());
    assert!(!current.is_disposed());
}

#[mue_macros::node]
#[theme()]
fn empty_optin() -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(Style::new().width(10.).height(20.), |style| {
        TRACE.with_borrow_mut(|trace| {
            trace.width = Some(style.take_width());
            trace.height = Some(style.take_height());
        });
    })
}

#[test]
fn empty_theme_optin_has_unit_options_and_hover_rules_do_not_enable_tracking() {
    fn unit_options<T: ThemeTarget<Options = ()>>() {}
    unit_options::<EmptyOptin>();
    reset();
    let theme = mue_macros::theme! { EmptyOptin:hover { height: 91px; } };
    let node = AppBuildContext::with_theme(theme.into(), || empty_optin().into_node());
    assert_eq!(dimensions(), (Size::Px(10.), Size::Px(20.)));
    assert!(node.hovered().is_none());
    Runtime::with(|rt| {
        let nodes = rt.nodes.borrow();
        assert!(nodes[node.id].hover_owners.is_empty());
        assert!(nodes[node.id].hovered_aliases.is_empty());
    });
    drop(node);
}
