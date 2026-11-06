pub use mue_macros::{component, node, style};

pub mod __private {
    pub mod core {
        pub mod prop {
            pub enum Prop<T> {
                Static(T),
            }

            impl<T> From<T> for Prop<T> {
                fn from(value: T) -> Self {
                    Self::Static(value)
                }
            }
        }

        pub mod signal {
            pub struct Signal<T>(pub T);

            pub fn signal<T>(value: T) -> Signal<T> {
                Signal(value)
            }
        }
    }
}

pub mod style {
    #[derive(Default)]
    pub struct Style {
        pub opacity: Option<f32>,
        pub value: u32,
    }

    impl Style {
        pub fn new() -> Self {
            Self::default()
        }
    }

    pub trait Styleable: Sized {
        fn style_mut(&mut self) -> &mut Style;

        fn opacity(mut self, value: f32) -> Self {
            self.style_mut().opacity = Some(value);
            self
        }
    }

    impl Styleable for Style {
        fn style_mut(&mut self) -> &mut Style {
            self
        }
    }

    pub trait StyleableExt: Styleable {}
    impl<T: Styleable> StyleableExt for T {}
}

pub mod node {
    use std::cell::Cell;

    use crate::style::Style;

    thread_local! {
        static ACTIVE_SCOPES: Cell<usize> = const { Cell::new(0) };
    }

    pub fn assert_in_scope() {
        assert!(
            ACTIVE_SCOPES.get() > 0,
            "construction must run inside its scope"
        );
    }

    #[derive(Debug)]
    pub struct Node {
        pub value: u32,
        pub opacity: Option<f32>,
        pub layers: usize,
        pub(crate) scope: std::rc::Rc<()>,
    }

    pub type ContextAlias = NodeContext;

    #[derive(Default)]
    pub struct NodeContext {
        opacity: Option<f32>,
        layers: usize,
        scope: std::rc::Rc<()>,
    }

    pub struct Scope;

    impl NodeContext {
        pub fn scope(&self) -> Scope {
            Scope
        }

        pub fn push_style(&mut self, style: Style) {
            assert_in_scope();
            self.opacity = self.opacity.or(style.opacity);
            self.layers += 1;
        }

        pub fn finish(self, mut defaults: Style, body: impl FnOnce(&mut Style)) -> Node {
            assert_in_scope();
            defaults.opacity = self.opacity.or(defaults.opacity);
            body(&mut defaults);
            Node {
                value: defaults.value,
                opacity: defaults.opacity,
                layers: self.layers,
                scope: self.scope,
            }
        }
    }

    impl Scope {
        pub fn run<T>(self, callback: impl FnOnce() -> T) -> T {
            struct RestoreScope(usize);
            impl Drop for RestoreScope {
                fn drop(&mut self) {
                    ACTIVE_SCOPES.set(self.0);
                }
            }
            let previous = ACTIVE_SCOPES.get();
            let _restore = RestoreScope(previous);
            ACTIVE_SCOPES.set(previous + 1);
            callback()
        }
    }

    pub trait BuildNode: Sized {
        fn build(self, ctx: NodeContext) -> Node;
    }

    pub trait IntoNode {
        fn into_node(self) -> Node;
    }

    impl<T: BuildNode> IntoNode for T {
        fn into_node(self) -> Node {
            BuildNode::build(self, NodeContext::default())
        }
    }
}
