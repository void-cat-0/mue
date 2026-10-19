use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use crate::{prelude::*, scope::inject};

#[test]
fn conditional_effect_unsubscribes_from_old_branch() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let choose_left = signal(true);
        let left = signal(0);
        let right = signal(0);
        let runs = Rc::new(Cell::new(0));

        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            runs_for_effect.set(runs_for_effect.get() + 1);
            if choose_left.get() {
                left.get();
            } else {
                right.get();
            }
        });

        assert_eq!(runs.get(), 1);
        choose_left.set(false);
        assert_eq!(runs.get(), 2);

        left.set(1);
        assert_eq!(runs.get(), 2);
        right.set(1);
        assert_eq!(runs.get(), 3);
    });
}

#[test]
fn computed_equal_result_suppresses_dependents() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let quotient = computed(move |_| source.get() / 2);
        let runs = Rc::new(Cell::new(0));

        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            quotient.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
        });

        assert_eq!(runs.get(), 1);
        source.set(1);
        assert_eq!(runs.get(), 1);
        source.set(2);
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn effect_owner_context_survives_trigger_from_child_scope() {
    let owner_a = Scope::new().owned();
    owner_a.provide::<&'static str>("A");

    let (source, runs) = owner_a.run(|| {
        let source = signal(0);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            assert_eq!(inject::<&'static str>(), Some("A"));
            source.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        (source, runs)
    });

    let owner_b = owner_a.run(|| Scope::new().owned());
    owner_b.run(|| source.set(1));
    assert_eq!(runs.get(), 2);
}

#[test]
fn reaction_is_owned_and_repeated_dispose_is_harmless() {
    let source = signal(0).owned();
    let scope = Scope::new();
    let runs = Rc::new(Cell::new(0));

    scope.run(|| {
        let runs_for_reaction = runs.clone();
        let reaction = create_reaction(move || {
            runs_for_reaction.set(runs_for_reaction.get() + 1);
        });

        reaction.track(|| source.get());
        source.set(1);
        assert_eq!(runs.get(), 1);

        reaction.dispose();
        reaction.dispose();
        source.set(2);
        assert_eq!(runs.get(), 1);
    });

    scope.run(|| {
        let runs_for_reaction = runs.clone();
        let reaction = create_reaction(move || {
            runs_for_reaction.set(runs_for_reaction.get() + 1);
        });
        reaction.track(|| source.get());
    });
    assert_eq!(runs.get(), 1);

    scope.dispose();
    assert!(scope.is_disposed());
    source.set(3);
    assert_eq!(runs.get(), 1, "disposing the owner must stop its reaction");
}

#[test]
fn cleanup_is_untracked_and_can_read_write_and_stop_effects() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(1);
        let other = signal(0);
        let derived = computed(move |_| source.get() * 2);
        let outer_runs = Rc::new(Cell::new(0));
        let cleanup_runs = Rc::new(Cell::new(0));
        let temporary_runs = Rc::new(Cell::new(0));

        let outer_runs_for_effect = outer_runs.clone();
        let cleanup_runs_for_effect = cleanup_runs.clone();
        let temporary_runs_for_cleanup = temporary_runs.clone();
        let _outer = watch_effect(move || {
            source.get();
            outer_runs_for_effect.set(outer_runs_for_effect.get() + 1);

            let derived = derived;
            let other = other;
            let cleanup_runs = cleanup_runs_for_effect.clone();
            let temporary_runs = temporary_runs_for_cleanup.clone();
            on_cleanup(move || {
                cleanup_runs.set(cleanup_runs.get() + 1);
                assert_eq!(derived.get(), 4);
                other.set(10);

                let temporary = watch_effect(move || {
                    other.get();
                    temporary_runs.set(temporary_runs.get() + 1);
                });
                temporary.dispose();
            });
        });

        assert_eq!(outer_runs.get(), 1);
        let result = catch_unwind(AssertUnwindSafe(|| source.set(2)));
        assert!(result.is_ok(), "cleanup operations must not borrow-panic");
        assert_eq!(outer_runs.get(), 2);
        assert_eq!(cleanup_runs.get(), 1);
        assert_eq!(temporary_runs.get(), 1);

        other.set(11);
        assert_eq!(
            outer_runs.get(),
            2,
            "cleanup reads/writes must not pollute tracking"
        );
    });
}

#[test]
fn batch_panic_restores_runtime_for_a_later_update() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            source.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
        });

        let result = catch_unwind(AssertUnwindSafe(|| {
            batch(|| {
                source.set(1);
                panic!("batch callback failed");
            });
        }));
        assert!(result.is_err());

        source.set(2);
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn effect_callback_panic_recovers_for_a_later_update() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let should_panic = Cell::new(true);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            let value = source.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
            if value == 1 && should_panic.replace(false) {
                panic!("effect callback failed");
            }
        });

        assert!(catch_unwind(AssertUnwindSafe(|| source.set(1))).is_err());
        source.set(2);
        assert_eq!(runs.get(), 3);
    });
}

#[test]
fn self_writing_effect_is_not_reentrant_and_does_not_drop_updates() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let value = signal(0);
        let depth = Cell::new(0);
        let max_depth = Rc::new(Cell::new(0));
        let runs = Rc::new(Cell::new(0));

        let max_depth_for_effect = max_depth.clone();
        let runs_for_effect = runs.clone();
        let _effect = watch_effect(move || {
            let next_depth = depth.get() + 1;
            depth.set(next_depth);
            max_depth_for_effect.set(max_depth_for_effect.get().max(next_depth));

            let current = value.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
            if current < 3 {
                value.set(current + 1);
            }

            depth.set(next_depth - 1);
        });

        assert_eq!(value.get(), 3);
        assert_eq!(runs.get(), 4);
        assert_eq!(max_depth.get(), 1);
    });
}

#[test]
fn mutually_writing_effects_hit_budget_but_runtime_remains_usable() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let enabled = signal(false);
        let left = signal(0);
        let right = signal(0);

        let probe = signal(0);
        let probe_runs = Rc::new(Cell::new(0));
        let probe_runs_for_effect = probe_runs.clone();
        let _probe_effect = watch_effect(move || {
            probe.get();
            probe_runs_for_effect.set(probe_runs_for_effect.get() + 1);
        });

        let _left_effect = watch_effect(move || {
            if enabled.get() {
                left.get();
                right.set_with(|value| value + 1);
            }
        });
        let _right_effect = watch_effect(move || {
            if enabled.get() {
                right.get();
                left.set_with(|value| value + 1);
            }
        });

        assert!(catch_unwind(AssertUnwindSafe(|| enabled.set(true))).is_err());
        assert!(
            catch_unwind(AssertUnwindSafe(|| enabled.set(false))).is_ok(),
            "the update budget panic must leave the runtime flushable"
        );

        probe.set(1);
        assert_eq!(probe_runs.get(), 2);
    });
}

#[test]
fn callback_can_dispose_itself_without_stale_access() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let calls = Rc::new(Cell::new(0));
        let reaction_slot: Rc<RefCell<Option<Reaction>>> = Rc::new(RefCell::new(None));

        let reaction = create_reaction({
            let calls = calls.clone();
            let reaction_slot = reaction_slot.clone();
            move || {
                calls.set(calls.get() + 1);
                if let Some(reaction) = *reaction_slot.borrow() {
                    reaction.dispose();
                }
            }
        });
        *reaction_slot.borrow_mut() = Some(reaction);

        reaction.track(|| source.get());
        source.set(1);
        assert_eq!(calls.get(), 1);
        source.set(2);
        assert_eq!(calls.get(), 1);
        reaction.dispose();
    });
}

#[test]
fn callback_can_dispose_its_owner_without_stale_access() {
    let source = signal(0).owned();
    let owner = Scope::new().owned();
    let owner_copy = *owner;
    let calls = Rc::new(Cell::new(0));

    owner.run(|| {
        let calls = calls.clone();
        let source = *source;
        let _effect = watch_effect(move || {
            source.get();
            calls.set(calls.get() + 1);
            owner_copy.dispose();
        });
    });

    assert_eq!(calls.get(), 1);
    assert!(owner.is_disposed());
    assert!(catch_unwind(AssertUnwindSafe(|| source.set(1))).is_ok());
    assert_eq!(calls.get(), 1);
    owner.dispose();
}

#[test]
fn computed_retries_after_a_failed_recompute_instead_of_returning_cache() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(1);
        let fail_once = Rc::new(Cell::new(false));
        let fail_once_for_computed = fail_once.clone();
        let doubled = computed(move |_| {
            let value = source.get();
            if fail_once_for_computed.replace(false) {
                panic!("computed recompute failed once");
            }
            value * 2
        });

        assert_eq!(doubled.get(), 2);
        fail_once.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| source.set(2))).is_err());

        // The failed run must leave the computed dirty. Reading it retries the
        // callback and observes the new source value rather than stale cache.
        assert_eq!(doubled.get(), 4);
    });
}

#[test]
fn reading_computed_chain_after_panicking_batch_does_not_false_cycle() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(1);
        let middle = computed(move |_| source.get() + 1);
        let tail = computed(move |_| middle.get() + 1);

        assert_eq!(tail.get(), 3);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                batch(|| {
                    source.set(2);
                    panic!("batch callback failed");
                });
            }))
            .is_err()
        );

        // The pending source update is still valid; traversing Check -> Dirty
        // computed dependencies must not mistake the interrupted batch for a cycle.
        assert_eq!(tail.get(), 4);
    });
}

#[test]
fn computed_budget_panic_leaves_dirty_cache_for_nonreactive_gate_retry() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let gate = Rc::new(Cell::new(false));
        let gate_for_computed = gate.clone();
        let derived = computed(move |_| {
            let current = source.get();
            if gate_for_computed.get() && current < 150 {
                source.set(current + 1);
            }
            current
        });

        assert_eq!(derived.get(), 0);
        gate.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| source.set(1))).is_err());
        let value_after_budget = source.get();
        assert!(value_after_budget >= 100);

        // The gate is deliberately non-reactive. A read after disabling it must
        // retry the computed callback instead of returning its pre-panic cache.
        gate.set(false);
        assert_eq!(derived.get(), source.get());
    });
}

#[test]
fn cleanup_runs_later_entries_after_an_earlier_cleanup_panics() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let first = Rc::new(Cell::new(0));
        let second = Rc::new(Cell::new(0));
        let first_for_effect = first.clone();
        let second_for_effect = second.clone();

        let _effect = watch_effect(move || {
            source.get();
            let first = first_for_effect.clone();
            on_cleanup(move || {
                first.set(first.get() + 1);
                panic!("first cleanup failed");
            });
            let second = second_for_effect.clone();
            on_cleanup(move || second.set(second.get() + 1));
        });

        assert!(catch_unwind(AssertUnwindSafe(|| source.set(1))).is_err());
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 1);
    });
}

#[test]
fn cleanup_after_owner_dispose_uses_null_scope_for_new_resources() {
    let source = signal(0).owned();
    let owner = Scope::new().owned();
    let owner_copy = *owner;

    owner.run(|| {
        let source = *source;
        let _effect = watch_effect(move || {
            source.get();
            let owner_for_first_cleanup = owner_copy;
            on_cleanup(move || owner_for_first_cleanup.dispose());
            on_cleanup(move || {
                // The first cleanup disposed the owner. This resource must be
                // created under Scope::null rather than the invalid owner.
                let temporary = watch_effect(|| {});
                temporary.dispose();
            });
        });
    });

    assert!(catch_unwind(AssertUnwindSafe(|| source.set(1))).is_ok());
    assert!(owner.is_disposed());
}

#[test]
fn batch_consuming_dirty_computed_runs_new_effect_only_once() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let doubled = computed(move |_| source.get() * 2);
        let runs = Rc::new(Cell::new(0));

        batch(|| {
            source.set(1);
            let runs_for_effect = runs.clone();
            let _effect = watch_effect(move || {
                assert_eq!(doubled.get(), 2);
                runs_for_effect.set(runs_for_effect.get() + 1);
            });
            assert_eq!(runs.get(), 1);
        });

        assert_eq!(runs.get(), 1);
    });
}

#[test]
fn reaction_tracking_dirty_computed_does_not_call_reaction_callback() {
    let scope = Scope::new().owned();
    scope.run(|| {
        let source = signal(0);
        let doubled = computed(move |_| source.get() * 2);
        let callback_runs = Rc::new(Cell::new(0));
        let callback_runs_for_reaction = callback_runs.clone();
        let reaction = create_reaction(move || {
            callback_runs_for_reaction.set(callback_runs_for_reaction.get() + 1);
        });

        batch(|| {
            source.set(1);
            assert_eq!(reaction.track(|| doubled.get()), 2);
            assert_eq!(callback_runs.get(), 0);
        });
        assert_eq!(callback_runs.get(), 0);

        batch(|| {
            source.set(2);
            assert_eq!(reaction.track(|| doubled.get()), 4);
        });
        assert_eq!(callback_runs.get(), 0);
        source.set(3);
        assert_eq!(callback_runs.get(), 1);
    });
}
