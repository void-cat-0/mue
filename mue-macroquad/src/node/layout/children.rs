use std::{collections::HashMap, hash::Hash, mem, rc::Rc};

use mue_core::{
    Disposable, Owned,
    effect::computed,
    prelude::current_scope,
    prop::Prop,
    signal::{Access, ReadSignal, signal},
};

use crate::node::{IntoNode, Node, NodeRef};

/// A reactive ordered list of borrowed node handles.
pub type Children = ReadSignal<Rc<[NodeRef]>>;

/// Convert nodes, tuples, and reactive child lists into owned children.
pub trait IntoChildren {
    fn into_children(self) -> Owned<Children>;
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

impl<T: IntoNode> IntoChildren for T {
    fn into_children(self) -> Owned<Children> {
        let node = self.into_node();
        computed(move |_| Rc::from([*node])).owned()
    }
}

macro_rules! impl_static_children {
    (@inner $ty:ty) => {
        fn into_children(self) -> Owned<Children> {
            let node_refs: Rc<[NodeRef]> = self.iter().map(|it| **it).collect();
            computed(move |_| {
                // Explicitly move self into closure
                let _ = &self;
                node_refs.clone()
            })
            .owned()
        }
    };
    ($ty:ty) => {
        impl IntoChildren for $ty {
            impl_static_children!(@inner $ty);
        }
    };
}

impl_static_children!(Rc<[Node]>);
impl_static_children!(Vec<Node>);

impl<const N: usize> IntoChildren for [Node; N] {
    impl_static_children!(@inner [Node; N]);
}

/// Map a reactive list to nodes in list order, retaining nodes by key.
///
/// `key_fn` is evaluated once per item on each update. A key present in both the
/// previous and next list reuses the same node, including after a reorder.
/// `node_fn` runs only for newly inserted keys: changing an item's other fields
/// **does not patch or rebuild** its retained node. Use signals inside the node
/// for independently reactive content, or change its key to replace it.
///
/// Removing a key drops its node and disposes its reactive scope. Reinserting
/// that key later creates a fresh node. The returned list owns its cached nodes;
/// its [`NodeRef`] values are borrowed handles and do not extend their lifetime.
///
/// # Panics
///
/// Panics if two items in one list have equal keys. All keys are validated before
/// reconciling the cache or calling `node_fn`, so invalid input cannot publish a
/// list containing dangling handles caused by overwriting a duplicate node.
/// Recovering and continuing after a reactive callback panics is not guaranteed.
pub fn map_keyed<T, K, N>(
    list: impl Into<Prop<Rc<[T]>>>,
    key_fn: impl FnMut(&T) -> K + 'static,
    mut node_fn: impl FnMut(&T) -> N + 'static,
) -> Owned<Children>
where
    T: 'static,
    K: Hash + Eq + 'static,
    N: IntoNode + 'static,
{
    fn inner<T, K>(
        list: Prop<Rc<[T]>>,
        mut key_fn: Box<dyn FnMut(&T) -> K + 'static>,
        mut node_fn: Box<dyn FnMut(&T) -> Node + 'static>,
    ) -> Owned<Children>
    where
        T: 'static,
        K: Hash + Eq + 'static,
    {
        let owner_scope = current_scope();

        let mut node_cache: HashMap<K, Node> = HashMap::new();
        computed(move |_| {
            let list = list.get_clone();
            let keys: Vec<K> = list.iter().map(&mut key_fn).collect();
            // Validate before taking the cache: replacing an earlier duplicate
            // would drop its Node while new_nodes still contains its NodeRef.
            let mut seen = HashMap::with_capacity(keys.len());
            for (index, key) in keys.iter().enumerate() {
                let previous = seen.insert(key, index);
                assert!(
                    previous.is_none(),
                    "map_keyed: duplicate key at index {index} (first seen at index {}); keys must be unique within each list",
                    previous.unwrap()
                );
            }
            drop(seen);

            let mut new_nodes = Vec::with_capacity(list.len());
            let mut prev_node_cache = mem::take(&mut node_cache);
            for (value, key) in list.iter().zip(keys) {
                let node = prev_node_cache
                    .remove(&key)
                    .unwrap_or_else(|| owner_scope.run(|| node_fn(value)));
                new_nodes.push(*node);
                node_cache.insert(key, node);
            }
            new_nodes.into()
        })
        .owned()
    }

    inner::<T, K>(
        list.into(),
        Box::new(key_fn),
        Box::new(move |t| node_fn(t).into_node()),
    )
}

/// Include an existing node only while `condition` is true.
///
/// The node is constructed before this call and owned by the returned list, even
/// when initially hidden. Hiding removes its handle from the list (and therefore
/// from a consuming parent's layout/render traversal), but does not dispose the
/// node or suspend its reactive effects. Showing it again reuses the same node
/// and local state; this is not a lazy mount/unmount helper.
///
/// Dropping the returned list, or disposing the parent that owns it, disposes the
/// node and its reactive scope, including when the node is hidden.
pub fn show_if(condition: impl Into<Prop<bool>>, node: Node) -> Owned<Children> {
    fn inner(condition: Prop<bool>, node: Node) -> Owned<Children> {
        let refs = Rc::<[NodeRef]>::from([*node]);
        computed(move |_| {
            // Explicitly moves node into this closure
            let _ = &node;
            if condition.get() {
                refs.clone()
            } else {
                Rc::default()
            }
        })
        .owned()
    }

    inner(condition.into(), node)
}

/// Flatten owned child lists into one reactive list, preserving order.
///
/// Lists are concatenated in their supplied order, and each list's internal
/// order is preserved. Updates to any input list recompute the flattened result;
/// empty lists contribute no nodes. This does not clone, patch, or deduplicate
/// nodes, so inputs must not supply the same node more than once to a layout
/// parent.
///
/// The returned list retains the input collection. Dropping it releases that
/// collection; when its last `Rc` owner is dropped, the input lists and their
/// owned nodes are disposed. Copying a resulting [`NodeRef`] does not retain
/// them. Tuples implementing [`IntoChildren`] use this same operation.
pub fn join_children(children: impl Into<Rc<[Owned<Children>]>>) -> Owned<Children> {
    fn inner(children: Rc<[Owned<Children>]>) -> Owned<Children> {
        let mut buffer = Vec::with_capacity(children.len());
        computed(move |_| {
            buffer.clear();
            buffer.extend(children.iter().map(|it| it.get_clone()));
            buffer.iter().flat_map(|it| it.iter().copied()).collect()
        })
        .owned()
    }

    inner(children.into())
}

macro_rules! impl_join {
    ($($p:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($p: IntoChildren),*> IntoChildren for ($($p,)*) {
            fn into_children(self) -> Owned<Children> {
                let ($($p,)*) = self;
                join_children([$($p.into_children()),*])
            }
        }
    };
}

impl_join!(T1);
impl_join!(T1 T2);
impl_join!(T1 T2 T3);
impl_join!(T1 T2 T3 T4);
impl_join!(T1 T2 T3 T4 T5);
impl_join!(T1 T2 T3 T4 T5 T6);
impl_join!(T1 T2 T3 T4 T5 T6 T7);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15);
impl_join!(T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16);

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        panic::{AssertUnwindSafe, catch_unwind},
    };

    use mue_core::{effect::watch_effect, scope::Scope};
    use taffy::prelude::TaffyMaxContent;

    use super::*;
    use crate::{
        layout::use_layout,
        runtime::Runtime,
        style::{Style, Styleable},
    };

    // Neither node construction nor use_layout initializes Macroquad's GPU context.
    fn layout_node(width: f32) -> Node {
        Node::build_with_style(Style::default().width(width).height(10.), |style| {
            use_layout(style);
        })
    }

    fn layout_parent(children: Owned<Children>) -> Node {
        Node::build_with_style(Style::default().children(children), |style| {
            use_layout(style);
        })
    }

    fn layout_id(node: NodeRef) -> taffy::NodeId {
        Runtime::with(|rt| rt.node(node.id).layout_id.unwrap())
    }

    fn is_alive(node: NodeRef) -> bool {
        Runtime::with(|rt| rt.nodes.borrow().contains_key(node.id))
    }

    fn assert_nodes(actual: &[NodeRef], expected: &[NodeRef]) {
        assert_eq!(
            actual.iter().map(|node| node.id).collect::<Vec<_>>(),
            expected.iter().map(|node| node.id).collect::<Vec<_>>()
        );
    }

    fn assert_layout_children(parent: NodeRef, expected: &[NodeRef]) {
        let parent = layout_id(parent);
        let expected: Vec<_> = expected.iter().map(|node| layout_id(*node)).collect();
        Runtime::with_taffy_mut(|taffy| {
            assert_eq!(taffy.children(parent).unwrap(), expected);
            for child in expected {
                assert_eq!(taffy.parent(child), Some(parent));
            }
            // Exercise actual layout as well as the tree's child bookkeeping.
            taffy
                .compute_layout(parent, taffy::Size::MAX_CONTENT)
                .unwrap();
        });
    }

    #[test]
    fn map_keyed_reuses_reorders_removes_and_does_not_patch_content() {
        let scope = Scope::new().owned();
        let created = Rc::new(RefCell::new(Vec::new()));
        let (list, children) = scope.run(|| {
            let list = signal(Rc::<[(u32, u32)]>::from([(1, 10), (2, 20), (3, 30)]));
            let children = map_keyed(list, |item| item.0, {
                let created = created.clone();
                move |item| {
                    created.borrow_mut().push(*item);
                    layout_node(item.1 as f32)
                }
            });
            (list, children)
        });
        let original = children.get_clone();

        list.set(Rc::from([(3, 300), (1, 100), (2, 200)]));
        assert_nodes(
            &children.get_clone(),
            &[original[2], original[0], original[1]],
        );
        assert_eq!(&*created.borrow(), &[(1, 10), (2, 20), (3, 30)]);
        for (node, width) in original.iter().zip([10., 20., 30.]) {
            Runtime::with_taffy_mut(|taffy| {
                let id = layout_id(*node);
                taffy.compute_layout(id, taffy::Size::MAX_CONTENT).unwrap();
                assert_eq!(taffy.layout(id).unwrap().size.width, width);
            });
        }

        list.set(Rc::from([(3, 300), (2, 200)]));
        assert_nodes(&children.get_clone(), &[original[2], original[1]]);
        assert!(!is_alive(original[0]), "removed key must dispose its node");
        assert!(is_alive(original[1]) && is_alive(original[2]));

        list.set(Rc::from([(1, 100), (2, 200)]));
        let reinserted = children.get_clone();
        assert!(reinserted[0] != original[0]);
        assert!(reinserted[1] == original[1]);
        assert!(!is_alive(original[2]));
        assert_eq!(created.borrow().last(), Some(&(1, 100)));

        list.set(Rc::default());
        assert!(children.get_clone().is_empty());
        assert!(reinserted.iter().all(|node| !is_alive(*node)));
    }

    #[test]
    fn map_keyed_duplicate_update_reports_indices_before_building_nodes() {
        let scope = Scope::new().owned();
        let builds = Rc::new(Cell::new(0));
        let (list, children) = scope.run(|| {
            let list = signal(Rc::<[u32]>::from([1, 2]));
            let children = map_keyed(list, |key| *key, {
                let builds = builds.clone();
                move |_| {
                    builds.set(builds.get() + 1);
                    Node::build(|| {})
                }
            });
            (list, children)
        });
        let previous = children.get_clone();
        // A new key before the duplicate must not invoke the factory either.
        let panic = catch_unwind(AssertUnwindSafe(|| {
            list.set(Rc::from([3, 1, 1]));
            children.get_clone();
        }))
        .expect_err("duplicate keys must panic");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap();
        assert!(message.contains("map_keyed: duplicate key at index 2"));
        assert!(message.contains("first seen at index 1"));
        assert_eq!(builds.get(), 2);
        // Core may drop a panicking callback; do not require panic recovery.
        // What matters here is rejecting the entire input before node_fn runs.
        drop(children);
        assert!(previous.iter().all(|node| !is_alive(*node)));
    }

    #[test]
    fn map_keyed_duplicate_initial_keys_do_not_build_nodes() {
        let scope = Scope::new().owned();
        let builds = Rc::new(Cell::new(0));
        // Create the scope outside the unwind boundary: older core runtimes do
        // not restore Scope::run's current scope on panic.
        scope.run(|| {
            let result = catch_unwind(AssertUnwindSafe(|| {
                map_keyed(Prop::Static(Rc::from([1, 1])), |key| *key, {
                    let builds = builds.clone();
                    move |_| {
                        builds.set(builds.get() + 1);
                        Node::build(|| {})
                    }
                })
            }));
            assert!(result.is_err());
        });
        assert_eq!(builds.get(), 0);
    }

    #[test]
    fn show_if_retains_hidden_node_and_effects_until_parent_disposal() {
        let scope = Scope::new().owned();
        scope.run(|| {
            let visible = signal(false);
            let tick = signal(0);
            let observed = Rc::new(Cell::new(0));
            let child = Node::build({
                let observed = observed.clone();
                move || {
                    watch_effect(move || observed.set(tick.get()));
                }
            });
            let child_ref = *child;
            let children = show_if(visible, child);
            let refs = *children;
            let parent = layout_parent(children);
            assert!(refs.get_clone().is_empty());
            assert!(is_alive(child_ref));

            tick.set(1);
            assert_eq!(observed.get(), 1, "initially hidden effects must run");
            visible.set(true);
            assert_nodes(&refs.get_clone(), &[child_ref]);
            visible.set(false);
            assert!(refs.get_clone().is_empty());
            tick.set(2);
            assert_eq!(observed.get(), 2, "hiding must not suspend effects");
            visible.set(true);
            assert_nodes(&refs.get_clone(), &[child_ref]);
            visible.set(false);

            drop(parent);
            assert!(!is_alive(child_ref), "hidden nodes must not leak");
            tick.set(3);
            assert_eq!(observed.get(), 2, "parent disposal must stop effects");
        });
    }

    #[test]
    fn join_children_preserves_list_and_item_order_and_owns_inputs() {
        let scope = Scope::new().owned();
        scope.run(|| {
            let first = Node::build(|| {});
            let middle_a = Node::build(|| {});
            let middle_b = Node::build(|| {});
            let last = Node::build(|| {});
            let expected = [*first, *middle_a, *middle_b, *last];
            let visible = signal(false);
            let joined = join_children([
                first.into_children(),
                ().into_children(),
                show_if(visible, middle_a),
                middle_b.into_children(),
                last.into_children(),
            ]);
            assert_nodes(
                &joined.get_clone(),
                &[expected[0], expected[2], expected[3]],
            );
            visible.set(true);
            assert_nodes(&joined.get_clone(), &expected);
            visible.set(false);
            assert_nodes(
                &joined.get_clone(),
                &[expected[0], expected[2], expected[3]],
            );

            drop(joined);
            assert!(expected.iter().all(|node| !is_alive(*node)));
        });
    }

    #[test]
    fn keyed_and_joined_updates_reconnect_taffy_children_in_order() {
        let scope = Scope::new().owned();
        scope.run(|| {
            let list = signal(Rc::<[u32]>::from([1, 2, 3]));
            let keyed = map_keyed(list, |key| *key, |_| layout_node(10.));
            let keyed_refs = *keyed;
            let original = keyed.get_clone();
            let visible = signal(true);
            let tail = layout_node(20.);
            let tail_ref = *tail;
            let tail_layout = layout_id(tail_ref);
            let non_layout = Node::build(|| {});
            let non_layout_ref = *non_layout;
            let children =
                join_children([keyed, non_layout.into_children(), show_if(visible, tail)]);
            let parent = layout_parent(children);
            assert_layout_children(*parent, &[original[0], original[1], original[2], tail_ref]);

            list.set(Rc::from([3, 1, 2]));
            assert_layout_children(*parent, &[original[2], original[0], original[1], tail_ref]);
            visible.set(false);
            assert_layout_children(*parent, &[original[2], original[0], original[1]]);
            Runtime::with_taffy_mut(|taffy| assert_eq!(taffy.parent(tail_layout), None));
            assert!(is_alive(tail_ref));

            list.set(Rc::from([2, 3]));
            assert_nodes(&keyed_refs.get_clone(), &[original[1], original[2]]);
            assert!(!is_alive(original[0]));
            assert_layout_children(*parent, &[original[1], original[2]]);
            visible.set(true);
            assert_layout_children(*parent, &[original[1], original[2], tail_ref]);

            drop(parent);
            assert!(original.iter().all(|node| !is_alive(*node)));
            assert!(!is_alive(tail_ref));
            assert!(!is_alive(non_layout_ref));
        });
    }
}
