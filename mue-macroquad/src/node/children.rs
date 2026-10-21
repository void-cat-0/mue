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

    fn factory_identity(&self) -> Option<usize> {
        None
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

impl<T: IntoNode> IntoChildren for T {
    fn into_children(self) -> Owned<Children> {
        let node = if self.factory_identity().is_some() {
            self.into_node()
        } else {
            crate::factory::without_sibling_context(|| self.into_node())
        };
        computed(move |_| Rc::from([*node])).owned()
    }

    fn factory_identity(&self) -> Option<usize> {
        IntoNode::factory_identity(self)
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

/// Map a reactive list to nodes, retaining each node while its key is present.
/// Keys must be unique; retained nodes keep their original content and local state.
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
            let mut new_nodes = Vec::with_capacity(list.len());
            let mut prev_node_cache = mem::take(&mut node_cache);
            for value in list.iter() {
                let key = key_fn(value);
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

/// Show an existing node conditionally, retaining its state while hidden.
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
                let siblings = vec![$($p.factory_identity()),*];
                let mut index = 0usize;
                let children = [$({
                    let current = index;
                    index += 1;
                    crate::factory::with_sibling_context(siblings.clone(), current, || {
                        $p.into_children()
                    })
                }),*];
                let _ = index;
                join_children(children)
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
