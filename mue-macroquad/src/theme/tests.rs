use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    marker::PhantomData,
    rc::Rc,
};

use mue_core::{effect::computed_always, scope::OwnedScope, signal::signal};

use super::*;
use crate::style::{Size, Styleable};

struct Alpha;
impl ThemeTarget for Alpha {
    type Options = ();
}
struct Beta;
impl ThemeTarget for Beta {
    type Options = ();
}
struct Gamma;
impl ThemeTarget for Gamma {
    type Options = ();
}

struct Generic<T, const N: usize>(PhantomData<T>);
impl<T: 'static, const N: usize> ThemeTarget for Generic<T, N> {
    type Options = ();
}

struct Options {
    enabled: bool,
}
struct WithOptions;
impl ThemeTarget for WithOptions {
    type Options = Options;
}
struct OtherOptions;
impl ThemeTarget for OtherOptions {
    type Options = Options;
}

struct LiveOptions {
    enabled: ReadSignal<bool>,
}
struct ReactiveTarget;
impl ThemeTarget for ReactiveTarget {
    type Options = LiveOptions;
}

fn classes(names: &[&str]) -> HashSet<ClassName> {
    names
        .iter()
        .map(|name| ClassName::try_from(*name).unwrap())
        .collect()
}

fn component<T: ThemeTarget<Options = ()>>(classes: &HashSet<ClassName>) -> ComponentRef<'_> {
    ComponentRef::new(T::key(), &(), classes, None)
}

type SparseStyle = (Option<Size>, Option<Size>, Option<f32>);

fn sparse(style: &Style) -> SparseStyle {
    (
        style.width.as_ref().map(|value| value.get()),
        style.height.as_ref().map(|value| value.get()),
        style.opacity.as_ref().map(|value| value.get()),
    )
}

fn assert_candidates(theme: &Theme, components: &[ComponentRef<'_>], expected: &[usize]) {
    let candidates = theme.candidate_indices(components);
    let candidates: &[usize] = candidates.as_ref();
    assert_eq!(candidates, expected);
}

fn logged_rule(id: usize, selectors: Vec<Selector>, events: &Rc<RefCell<Vec<usize>>>) -> Rule {
    let events = events.clone();
    Rule::new(selectors, move || {
        events.borrow_mut().push(id);
        let mut style = Style::new();
        if id & 1 != 0 {
            style = style.width(10. + id as f32);
        }
        if id & 2 != 0 {
            style = style.height(20. + id as f32);
        }
        if id & 4 != 0 {
            style = style.opacity((id % 4 + 1) as f32 * 0.125);
        }
        style
    })
}

#[test]
fn keyed_selectors_enforce_identity_before_calling_the_predicate() {
    let empty = HashSet::new();
    let alpha = component::<Alpha>(&empty);
    let beta = component::<Beta>(&empty);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let keyed = Selector::for_target::<Alpha>(Specificity(0, 0, 1), move |_| {
        count.set(count.get() + 1);
        true
    });

    assert!(!keyed.matches(beta));
    assert_eq!(calls.get(), 0);
    assert!(keyed.matches(alpha));
    assert_eq!(calls.get(), 1);

    let unkeyed = Selector::new(Specificity(0, 0, 1), |_| true);
    assert!(unkeyed.matches(alpha));
    assert!(unkeyed.matches(beta));
}

#[test]
fn identity_index_omits_unrelated_rules_and_deduplicates_repeated_and_mixed_lists() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let weight = Specificity(0, 0, 1);
    let theme = Theme::new(vec![
        logged_rule(
            0,
            vec![
                Selector::for_target::<Alpha>(weight, |_| true),
                Selector::for_target::<Alpha>(weight, |_| true),
            ],
            &events,
        ),
        logged_rule(
            1,
            vec![
                Selector::for_target::<Beta>(weight, |_| true),
                Selector::for_target::<Alpha>(weight, |_| true),
            ],
            &events,
        ),
        logged_rule(2, vec![Selector::new(weight, |_| true)], &events),
        logged_rule(
            3,
            vec![
                Selector::for_target::<Alpha>(weight, |_| true),
                Selector::new(weight, |_| true),
                Selector::for_target::<Beta>(weight, |_| true),
                Selector::new(weight, |_| true),
            ],
            &events,
        ),
        logged_rule(
            4,
            vec![Selector::for_target::<Gamma>(weight, |_| true)],
            &events,
        ),
    ]);
    let empty = HashSet::new();
    let alpha = component::<Alpha>(&empty);
    let beta = component::<Beta>(&empty);

    assert_eq!(
        theme.indexed.by_identity.get(&Alpha::key()).unwrap(),
        &[0, 1, 3]
    );
    assert_eq!(theme.indexed.generic, [2, 3]);
    assert_candidates(&theme, &[alpha, alpha, beta, beta], &[0, 1, 2, 3]);
    assert_candidates(&theme, &[], &[]);
    theme.resolve(&[alpha, alpha, beta, beta]);
    assert_eq!(&*events.as_ref().borrow(), &[0, 1, 2, 3]);

    events.borrow_mut().clear();
    assert_eq!(sparse(&theme.resolve(&[])), (None, None, None));
    assert!(events.as_ref().borrow().is_empty());
}

#[test]
fn a_rule_runs_once_at_its_highest_actually_matching_selector_specificity() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let first_events = events.clone();
    let second_events = events.clone();
    let theme = Theme::new(vec![
        Rule::new(
            vec![
                Selector::for_target::<Alpha>(Specificity(0, 0, 1), |_| true),
                Selector::for_target::<Alpha>(Specificity(0, 3, 1), |target| {
                    target.has_class("selected")
                }),
                Selector::for_target::<Beta>(Specificity(0, 50, 1), |target| {
                    target.has_class("never")
                }),
                Selector::for_target::<Gamma>(Specificity(1, 0, 0), |_| true),
            ],
            move || {
                first_events.borrow_mut().push(0);
                Style::new().height(31.)
            },
        ),
        Rule::new(
            vec![Selector::for_target::<Alpha>(Specificity(0, 2, 1), |_| {
                true
            })],
            move || {
                second_events.borrow_mut().push(1);
                Style::new().height(41.)
            },
        ),
    ]);
    let empty = HashSet::new();
    let selected = classes(&["selected"]);
    let beta = component::<Beta>(&empty);

    for (names, expected_height, expected_order) in
        [(&empty, 41., vec![0, 1]), (&selected, 31., vec![1, 0])]
    {
        events.borrow_mut().clear();
        let alpha = component::<Alpha>(names);
        let style = theme.resolve(&[alpha, beta, alpha]);
        assert_eq!(sparse(&style).1, Some(Size::Px(expected_height)));
        assert_eq!(*events.as_ref().borrow(), expected_order);
    }
}

#[test]
fn compose_preserves_rule_and_module_order_including_duplicate_modules() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let weight = Specificity(0, 0, 1);
    let first = Theme::new(vec![
        logged_rule(
            1,
            vec![Selector::for_target::<Alpha>(weight, |_| true)],
            &events,
        ),
        logged_rule(3, vec![Selector::new(weight, |_| true)], &events),
    ]);
    let second = Theme::new(vec![logged_rule(
        5,
        vec![Selector::for_target::<Alpha>(weight, |_| true)],
        &events,
    )]);
    let theme = Theme::compose([first.clone(), Theme::default(), second, first]);
    let empty = HashSet::new();
    let style = theme.resolve(&[component::<Alpha>(&empty)]);

    assert_eq!(theme.rules().len(), 5);
    assert_eq!(&*events.as_ref().borrow(), &[1, 3, 5, 1, 3]);
    assert_eq!(
        sparse(&style),
        (Some(Size::Px(13.)), Some(Size::Px(23.)), Some(0.25))
    );
}

#[test]
fn classes_and_options_must_match_on_one_target_even_when_keys_are_repeated() {
    let empty = HashSet::new();
    let selected = classes(&["selected"]);
    let enabled = Options { enabled: true };
    let disabled = Options { enabled: false };
    let enabled_plain = ComponentRef::new(WithOptions::key(), &enabled, &empty, None);
    let disabled_selected = ComponentRef::new(WithOptions::key(), &disabled, &selected, None);
    let enabled_selected = ComponentRef::new(WithOptions::key(), &enabled, &selected, None);
    let other_selected = ComponentRef::new(OtherOptions::key(), &enabled, &selected, None);
    let theme = Theme::new(vec![Rule::new(
        vec![Selector::for_target::<WithOptions>(
            Specificity(0, 2, 1),
            |target| {
                target.has_class("selected")
                    && target
                        .options::<WithOptions>()
                        .is_some_and(|options| options.enabled)
            },
        )],
        || Style::new().height(42.),
    )]);

    assert!(other_selected.options::<WithOptions>().is_none());
    assert_eq!(
        sparse(&theme.resolve(&[enabled_plain, disabled_selected, other_selected])),
        (None, None, None),
    );
    assert_eq!(
        sparse(&theme.resolve(&[enabled_plain, disabled_selected, enabled_selected])),
        (None, Some(Size::Px(42.)), None),
    );
}

#[test]
fn concrete_generic_type_and_const_arguments_have_distinct_identity_buckets() {
    assert!(Generic::<u8, 1>::key() != Generic::<u16, 1>::key());
    assert!(Generic::<u8, 1>::key() != Generic::<u8, 2>::key());
    let weight = Specificity(0, 0, 1);
    let theme = Theme::new(vec![
        Rule::new(
            vec![Selector::for_target::<Generic<u8, 1>>(weight, |_| true)],
            || Style::new().width(8.),
        ),
        Rule::new(
            vec![Selector::for_target::<Generic<u16, 1>>(weight, |_| true)],
            || Style::new().height(16.),
        ),
        Rule::new(
            vec![Selector::for_target::<Generic<u8, 2>>(weight, |_| true)],
            || Style::new().opacity(0.5),
        ),
    ]);
    let empty = HashSet::new();
    let first = component::<Generic<u8, 1>>(&empty);
    let second = component::<Generic<u16, 1>>(&empty);
    let third = component::<Generic<u8, 2>>(&empty);

    assert_candidates(&theme, &[first], &[0]);
    assert_candidates(&theme, &[second], &[1]);
    assert_candidates(&theme, &[third], &[2]);
    assert_eq!(
        sparse(&theme.resolve(&[first])),
        (Some(Size::Px(8.)), None, None)
    );
    assert_eq!(
        sparse(&theme.resolve(&[second])),
        (None, Some(Size::Px(16.)), None)
    );
    assert_eq!(sparse(&theme.resolve(&[third])), (None, None, Some(0.5)));
}

#[test]
fn reactive_options_classes_hover_and_theme_replacement_recompute_matches() {
    let scope = OwnedScope::new();
    scope.run(|| {
        let enabled = signal(false);
        let names = signal(HashSet::new());
        let hovered = signal(false);
        let initial = Theme::new(vec![
            Rule::new(
                vec![Selector::for_target::<ReactiveTarget>(
                    Specificity(0, 0, 1),
                    |_| true,
                )],
                || Style::new().width(10.),
            ),
            Rule::new(
                vec![Selector::for_target::<ReactiveTarget>(
                    Specificity(0, 1, 1),
                    |target| target.options::<ReactiveTarget>().unwrap().enabled.get(),
                )],
                || Style::new().height(20.),
            ),
            Rule::new(
                vec![Selector::for_target::<ReactiveTarget>(
                    Specificity(0, 1, 1),
                    |target| target.has_class("selected"),
                )],
                || Style::new().width(30.),
            ),
            Rule::new(
                vec![Selector::for_target::<ReactiveTarget>(
                    Specificity(0, 1, 1),
                    |target| target.hovered(),
                )],
                || Style::new().opacity(0.5),
            ),
        ]);
        let theme = signal(initial.clone());
        let resolved = computed_always(move |_| {
            let options = LiveOptions { enabled: *enabled };
            let names = names.get_clone();
            let component =
                ComponentRef::new(ReactiveTarget::key(), &options, &names, Some(*hovered));
            Rc::new(theme.get_clone().resolve(&[component]))
        });
        let read = || sparse(&resolved.get_clone());

        assert_eq!(read(), (Some(Size::Px(10.)), None, None));
        enabled.set(true);
        assert_eq!(read(), (Some(Size::Px(10.)), Some(Size::Px(20.)), None));
        names.set(classes(&["selected"]));
        assert_eq!(read(), (Some(Size::Px(30.)), Some(Size::Px(20.)), None));
        hovered.set(true);
        assert_eq!(
            read(),
            (Some(Size::Px(30.)), Some(Size::Px(20.)), Some(0.5))
        );
        enabled.set(false);
        assert_eq!(read(), (Some(Size::Px(30.)), None, Some(0.5)));
        names.set(HashSet::new());
        assert_eq!(read(), (Some(Size::Px(10.)), None, Some(0.5)));
        hovered.set(false);
        assert_eq!(read(), (Some(Size::Px(10.)), None, None));

        theme.set(Theme::new(vec![
            Rule::new(
                vec![Selector::for_target::<Beta>(Specificity(0, 0, 1), |_| true)],
                || Style::new().width(99.),
            ),
            Rule::new(
                vec![Selector::for_target::<ReactiveTarget>(
                    Specificity(0, 1, 1),
                    |target| target.has_class("selected"),
                )],
                || Style::new().opacity(0.25),
            ),
        ]));
        assert_eq!(read(), (None, None, None));
        names.set(classes(&["selected"]));
        assert_eq!(read(), (None, None, Some(0.25)));
        theme.set(initial);
        assert_eq!(read(), (Some(Size::Px(30.)), None, None));
        theme.set(Theme::default());
        assert_eq!(read(), (None, None, None));
    });
}

#[test]
fn indexed_resolution_matches_linear_scan_for_a_deterministic_sparse_matrix() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut rules = Vec::new();
    for index in 0usize..24 {
        let weight = Specificity(0, (index / 6) as u32, 1);
        let higher = Specificity(0, (index / 6) as u32 + 5, 1);
        let selectors = match index % 6 {
            0 => vec![
                Selector::for_target::<Alpha>(weight, |target| target.has_class("wide")),
                Selector::for_target::<Alpha>(higher, |target| target.has_class("active")),
            ],
            1 => vec![Selector::for_target::<Beta>(weight, |target| {
                target.hovered()
            })],
            2 => vec![
                Selector::new(weight, |target| target.has_class("global")),
                Selector::for_target::<Alpha>(higher, |target| target.has_class("selected")),
            ],
            3 => vec![
                Selector::for_target::<Gamma>(weight, |_| true),
                Selector::for_target::<Generic<u8, 1>>(higher, |_| true),
            ],
            4 => vec![
                Selector::for_target::<WithOptions>(higher, |target| {
                    target.has_class("selected") && target.options::<WithOptions>().unwrap().enabled
                }),
                Selector::new(weight, |target| {
                    target.has_class("active") && target.hovered()
                }),
            ],
            _ => vec![
                Selector::new(weight, |_| true),
                Selector::for_target::<Beta>(Specificity(1, 0, 0), |_| false),
            ],
        };
        rules.push(logged_rule(index, selectors, &events));
    }
    let first = Theme::new(rules[..12].to_vec());
    let second = Theme::new(rules[12..].to_vec());
    let theme = Theme::compose([first.clone(), second, first]);
    let class_sets = [
        classes(&[]),
        classes(&["wide"]),
        classes(&["active", "global"]),
        classes(&["selected", "wide", "active"]),
    ];
    let scope = OwnedScope::new();
    scope.run(|| {
        let hovered = signal(false);
        for left in &class_sets {
            for right in &class_sets {
                for is_hovered in [false, true] {
                    hovered.set(is_hovered);
                    for enabled in [false, true] {
                        let options = Options { enabled };
                        let targets = [
                            component::<Alpha>(left),
                            ComponentRef::new(Alpha::key(), &(), right, Some(*hovered)),
                            ComponentRef::new(Beta::key(), &(), right, Some(*hovered)),
                            ComponentRef::new(WithOptions::key(), &options, left, Some(*hovered)),
                            component::<Generic<u16, 1>>(right),
                            component::<Generic<u8, 1>>(left),
                            component::<Gamma>(right),
                        ];
                        // Every prefix includes the empty case; the first two
                        // targets deliberately share a key but keep distinct classes.
                        for length in 0..=targets.len() {
                            let targets = &targets[..length];
                            events.borrow_mut().clear();
                            let indexed = sparse(&theme.resolve(targets));
                            let indexed_order = events.as_ref().borrow().clone();
                            events.borrow_mut().clear();
                            let linear = sparse(&theme.resolve_linear(targets));
                            let linear_order = events.as_ref().borrow().clone();

                            assert_eq!(
                                indexed, linear,
                                "length={length}, hovered={is_hovered}, enabled={enabled}"
                            );
                            assert_eq!(
                                indexed_order, linear_order,
                                "length={length}, hovered={is_hovered}, enabled={enabled}"
                            );
                            if length == 0 {
                                assert_eq!(indexed, (None, None, None));
                                assert!(indexed_order.is_empty());
                            }
                        }
                    }
                }
            }
        }
    });
}

#[test]
fn long_component_chains_preserve_distinct_same_key_conditions() {
    let theme = Theme::new(vec![
        Rule::new(
            vec![Selector::for_target::<Alpha>(
                Specificity(0, 1, 1),
                |target| target.has_class("selected"),
            )],
            || Style::new().width(42.),
        ),
        Rule::new(
            vec![Selector::for_target::<Beta>(Specificity(0, 0, 1), |_| true)],
            || Style::new().height(9.),
        ),
    ]);
    let empty = HashSet::new();
    let selected = classes(&["selected"]);
    let mut targets = vec![component::<Alpha>(&empty); 32];
    targets.push(component::<Alpha>(&selected));
    assert_candidates(&theme, &targets, &[0]);
    assert_eq!(
        sparse(&theme.resolve(&targets)),
        (Some(Size::Px(42.)), None, None)
    );
    assert_eq!(
        sparse(&theme.resolve(&targets)),
        sparse(&theme.resolve_linear(&targets))
    );
}

#[test]
fn macro_selectors_keep_named_generic_and_alias_identity_metadata() {
    type Alias = Generic<u8, 1>;
    let theme = mue_macros::theme! {
        Generic<u8, 1>, Alias.selected, .selected, * { width: 11px; }
        Generic<u16, 1> { height: 12px; }
    };
    let selectors = theme.rules()[0].selectors();
    assert!(selectors[0].identity == Some(Generic::<u8, 1>::key()));
    assert!(selectors[1].identity == Some(Alias::key()));
    assert!(selectors[2].identity.is_none());
    assert!(selectors[3].identity.is_none());
    assert!(theme.rules()[1].selectors()[0].identity == Some(Generic::<u16, 1>::key()));
    let empty = HashSet::new();
    assert!(!selectors[0].matches(component::<Generic<u16, 1>>(&empty)));
    assert_candidates(&theme, &[component::<Generic<u8, 1>>(&empty)], &[0]);
}
