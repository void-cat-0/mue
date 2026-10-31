use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    mem,
    rc::Rc,
};

use mue_core::{
    Disposable, Owned,
    effect::computed,
    prelude::current_scope,
    prop::Prop,
    scope::OwnedScope,
    signal::{Access, ReadSignal, signal},
};

use crate::node::{AppBuildContext, IntoNode, Node, NodeRef};

/// A reactive ordered list of borrowed node handles.
pub type Children = ReadSignal<Rc<[NodeRef]>>;

/// A one-shot, deferred conversion to an owned child list.
pub struct ChildrenPlan(Box<dyn FnOnce() -> Owned<Children>>);

impl ChildrenPlan {
    /// Defer child construction until the parent is ready to mount children.
    pub fn new(build: impl FnOnce() -> Owned<Children> + 'static) -> Self {
        Self(Box::new(build))
    }

    /// Construct and return the owned child list.
    pub fn mount(self) -> Owned<Children> {
        (self.0)()
    }

    /// Compatibility alias for callers that explicitly request eager conversion.
    pub fn into_children(self) -> Owned<Children> {
        self.mount()
    }
}

/// Convert nodes, tuples, and reactive child lists into owned children.
pub trait IntoChildren: 'static {
    fn into_children(self) -> Owned<Children>;

    /// Defer conversion until the parent mounts its children.
    fn into_children_plan(self) -> ChildrenPlan
    where
        Self: Sized,
    {
        ChildrenPlan::new(move || self.into_children())
    }
}

impl IntoChildren for ChildrenPlan {
    fn into_children(self) -> Owned<Children> {
        self.mount()
    }

    fn into_children_plan(self) -> ChildrenPlan {
        self
    }
}

impl IntoChildren for () {
    fn into_children(self) -> Owned<Children> {
        (*signal(Rc::default())).owned()
    }
}

impl IntoChildren for Owned<Children> {
    fn into_children(self) -> Owned<Children> {
        self
    }
}

impl<T: IntoNode + 'static> IntoChildren for T {
    fn into_children(self) -> Owned<Children> {
        let node = self.into_node();
        computed(move |_| {
            let _ = &node;
            Rc::from([*node])
        })
        .owned()
    }
}

impl<T: IntoChildren> IntoChildren for Vec<T> {
    fn into_children(self) -> Owned<Children> {
        let children = self.into_iter().map(IntoChildren::into_children).collect();
        join_owned_children(children)
    }
}

impl<T: IntoChildren, const N: usize> IntoChildren for [T; N] {
    fn into_children(self) -> Owned<Children> {
        let children = self.into_iter().map(IntoChildren::into_children).collect();
        join_owned_children(children)
    }
}

impl IntoChildren for Rc<[Node]> {
    fn into_children(self) -> Owned<Children> {
        computed(move |_| {
            let refs: Rc<[NodeRef]> = self.iter().map(|node| **node).collect();
            refs
        })
        .owned()
    }
}

fn join_owned_children(children: Rc<[Owned<Children>]>) -> Owned<Children> {
    let mut buffer = Vec::with_capacity(children.len());
    computed(move |_| {
        buffer.clear();
        buffer.extend(children.iter().map(|child| child.get_clone()));
        buffer
            .iter()
            .flat_map(|children| children.iter().copied())
            .collect()
    })
    .owned()
}

impl IntoChildren for Rc<[Owned<Children>]> {
    fn into_children(self) -> Owned<Children> {
        join_owned_children(self)
    }
}

/// Defer flattening nodes or child lists, preserving their order.
///
/// Tuples can combine different builders, nodes, plans, and existing child lists.
/// Vectors and arrays can contain any one of those child types.
pub fn join_children(children: impl IntoChildren) -> ChildrenPlan {
    children.into_children_plan()
}

/// Map a reactive list to nodes, retaining each node while its key is present.
/// Keys must be unique; retained nodes keep their original content and local state.
pub fn map_keyed<T, K, N>(
    list: impl Into<Prop<Rc<[T]>>>,
    mut key_fn: impl FnMut(&T) -> K + 'static,
    mut node_fn: impl FnMut(&T) -> N + 'static,
) -> ChildrenPlan
where
    T: 'static,
    K: Hash + Eq + 'static,
    N: IntoNode + 'static,
{
    let list = list.into();
    ChildrenPlan::new(move || {
        let owner_scope = current_scope();
        let build_context = AppBuildContext::capture();
        // Own callback-created resources for exactly as long as their keyed node.
        // Node is first so its own consumers are dropped before this extra scope.
        let mut node_cache: HashMap<K, (Node, OwnedScope)> = HashMap::new();

        computed(move |_| {
            let list = list.get_clone();
            let keys: Vec<K> = list.iter().map(&mut key_fn).collect();
            let mut seen = HashSet::with_capacity(keys.len());
            for (index, key) in keys.iter().enumerate() {
                assert!(
                    seen.insert(key),
                    "map_keyed: duplicate key at index {index}; keys must be unique"
                );
            }

            let mut new_nodes = Vec::with_capacity(list.len());
            let mut previous = mem::take(&mut node_cache);
            for (value, key) in list.iter().zip(keys) {
                let entry = previous.remove(&key).unwrap_or_else(|| {
                    owner_scope.run(|| {
                        let scope = OwnedScope::new_inherited();
                        let node = scope.run(|| build_context.run(|| node_fn(value).into_node()));
                        (node, scope)
                    })
                });
                new_nodes.push(*entry.0);
                node_cache.insert(key, entry);
            }
            new_nodes.into()
        })
        .owned()
    })
}

/// Show an existing or deferred node conditionally, retaining its state while hidden.
pub fn show_if(condition: impl Into<Prop<bool>>, node: impl IntoNode + 'static) -> ChildrenPlan {
    let condition = condition.into();
    ChildrenPlan::new(move || {
        let node = node.into_node();
        let refs = Rc::<[NodeRef]>::from([*node]);
        computed(move |_| {
            let _ = &node;
            if condition.get() {
                refs.clone()
            } else {
                Rc::default()
            }
        })
        .owned()
    })
}

macro_rules! impl_tuple_children {
    ($($item:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($item: IntoChildren),*> IntoChildren for ($($item,)*) {
            fn into_children(self) -> Owned<Children> {
                let ($($item,)*) = self;
                let children = [$($item.into_children()),*];
                join_owned_children(children.into())
            }
        }
    };
}

impl_tuple_children!(T1);
impl_tuple_children!(T1 T2);
impl_tuple_children!(T1 T2 T3);
impl_tuple_children!(T1 T2 T3 T4);
impl_tuple_children!(T1 T2 T3 T4 T5);
impl_tuple_children!(T1 T2 T3 T4 T5 T6);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15);
impl_tuple_children!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16);

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        panic::{AssertUnwindSafe, catch_unwind},
    };

    use super::*;

    struct CountedNode(Rc<Cell<usize>>);

    impl IntoNode for CountedNode {
        fn into_node(self) -> Node {
            self.0.set(self.0.get() + 1);
            Node::build(|| {})
        }
    }

    #[test]
    fn tuple_vector_and_array_plans_defer_node_construction() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let builds = Rc::new(Cell::new(0));
            let tuple = (CountedNode(builds.clone()), Node::build(|| {})).into_children_plan();
            let vector =
                vec![CountedNode(builds.clone()), CountedNode(builds.clone())].into_children_plan();
            let array =
                [CountedNode(builds.clone()), CountedNode(builds.clone())].into_children_plan();
            assert_eq!(builds.get(), 0);

            let children = join_children((tuple, vector, array)).mount();
            assert_eq!(builds.get(), 5);
            assert_eq!(children.get_clone().len(), 6);
        });
    }

    #[test]
    fn dropping_an_unmounted_plan_does_not_build_children() {
        let builds = Rc::new(Cell::new(0));
        let plan = CountedNode(builds.clone()).into_children_plan();
        drop(plan);
        assert_eq!(builds.get(), 0);
    }

    #[test]
    fn shared_node_container_keeps_its_owning_semantics() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let node = Node::build(|| {});
            let expected = *node;
            let nodes: Rc<[Node]> = Rc::from([node]);
            let retained = nodes.clone();
            let plan = nodes.into_children_plan();
            assert_eq!(Rc::strong_count(&retained), 2);
            let children = plan.mount();
            assert!(children.get_clone().as_ref() == &[expected]);
            assert_eq!(Rc::strong_count(&retained), 2);
            drop(children);
            assert_eq!(Rc::strong_count(&retained), 1);
        });
    }

    #[test]
    fn shared_owned_lists_remain_owned_until_the_shared_container_drops() {
        let owner = OwnedScope::new();
        owner.run(|| {
            let lists: Rc<[Owned<Children>]> = Rc::from([().into_children()]);
            let source = *lists[0];
            let retained = lists.clone();
            let children = join_children(lists).mount();
            assert!(children.get_clone().is_empty());
            assert!(!source.is_disposed());
            drop(children);
            assert!(!source.is_disposed());
            drop(retained);
            assert!(source.is_disposed());
        });
    }

    #[test]
    fn show_if_defers_construction_and_retains_the_node_while_hidden() {
        let source_owner = OwnedScope::new();
        let visible = source_owner.run(|| signal(false));
        let builds = Rc::new(Cell::new(0));
        let plan = show_if(visible, CountedNode(builds.clone()));
        assert_eq!(builds.get(), 0);
        let owner = OwnedScope::new();
        let children = owner.run(|| plan.mount());
        assert_eq!(builds.get(), 1);
        assert!(children.get_clone().is_empty());
        visible.set(true);
        let first = children.get_clone()[0];
        visible.set(false);
        assert!(children.get_clone().is_empty());
        visible.set(true);
        assert!(children.get_clone()[0] == first);
        assert_eq!(builds.get(), 1);
        drop(children);
        drop(owner);
        assert!(!visible.is_disposed());
    }

    #[test]
    fn keyed_nodes_reuse_state_and_dispose_only_the_removed_keys_resources() {
        let source_owner = OwnedScope::new();
        let source = source_owner.run(|| signal(Rc::<[i32]>::from([1, 2])));
        let owner = OwnedScope::new();
        owner.provide(17u32);
        let resources = Rc::new(RefCell::new(Vec::new()));
        let captured = resources.clone();
        let plan = map_keyed(
            source,
            |key| *key,
            move |value| {
                assert_eq!(current_scope().inject::<u32>(), Some(17));
                let resource = signal(*value);
                captured.borrow_mut().push((*value, *resource));
                Node::build(|| {})
            },
        );
        assert!(resources.borrow().is_empty());
        let children = owner.run(|| plan.mount());
        let original = children.get_clone();
        assert_eq!(resources.borrow().len(), 2);

        source.set(Rc::from([2, 1]));
        let reordered = children.get_clone();
        assert!(reordered[0] == original[1]);
        assert!(reordered[1] == original[0]);
        assert_eq!(resources.borrow().len(), 2);

        source.set(Rc::from([2, 3]));
        let updated = children.get_clone();
        assert!(updated[0] == original[1]);
        assert_eq!(resources.borrow().len(), 3);
        assert!(resources.borrow()[0].1.is_disposed());
        assert!(!resources.borrow()[1].1.is_disposed());
        assert!(!resources.borrow()[2].1.is_disposed());

        drop(children);
        assert!(
            resources
                .borrow()
                .iter()
                .all(|(_, signal)| signal.is_disposed())
        );
        drop(owner);
        assert!(!source.is_disposed());
        source.set(Rc::from([4]));
        assert_eq!(source.get_clone().as_ref(), &[4]);
    }

    #[test]
    fn later_keys_restore_the_mount_theme_and_isolate_failed_resources() {
        use crate::{node::RootBuild, theme::Theme};

        let source_owner = OwnedScope::new();
        let source = source_owner.run(|| signal(Rc::<[i32]>::from([])));
        let owner = OwnedScope::new();
        owner.provide(23u32);
        let resources = Rc::new(RefCell::new(Vec::new()));
        let captured = resources.clone();
        let plan = map_keyed(
            source,
            |key| *key,
            move |value| {
                assert!(RootBuild::new().has_theme());
                assert_eq!(current_scope().inject::<u32>(), Some(23));
                let resource = signal(*value);
                captured.borrow_mut().push(*resource);
                assert!(*value != 2, "key construction failed");
                Node::build(|| {})
            },
        );
        let children = owner
            .run(|| AppBuildContext::with_theme(Prop::Static(Theme::default()), || plan.mount()));
        assert!(!RootBuild::new().has_theme());
        source.set(Rc::from([1]));
        assert_eq!(children.get_clone().len(), 1);
        assert!(!resources.borrow()[0].is_disposed());
        assert!(!RootBuild::new().has_theme());

        let update_scope = OwnedScope::new();
        update_scope.provide(99u32);
        update_scope.run(|| {
            let failed = catch_unwind(AssertUnwindSafe(|| source.set(Rc::from([1, 2]))));
            assert!(failed.is_err());
            assert_eq!(current_scope().inject::<u32>(), Some(99));
            assert!(!RootBuild::new().has_theme());
        });
        assert!(resources.borrow()[1].is_disposed());
        drop(children);
        drop(owner);
        assert!(resources.borrow()[0].is_disposed());
        assert!(!source.is_disposed());
    }

    #[test]
    fn duplicate_keys_are_rejected_before_constructing_any_node() {
        let owner = OwnedScope::new();
        let builds = Rc::new(Cell::new(0));
        let captured = builds.clone();
        let plan = map_keyed(
            Prop::Static(Rc::<[i32]>::from([1, 1])),
            |key| *key,
            move |_| CountedNode(captured.clone()),
        );
        let result = catch_unwind(AssertUnwindSafe(|| owner.run(|| plan.mount())));
        let panic = match result {
            Ok(_) => panic!("duplicate keys were accepted"),
            Err(panic) => panic,
        };
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(message.contains("map_keyed: duplicate key at index 1"));
        assert_eq!(builds.get(), 0);
    }
}
