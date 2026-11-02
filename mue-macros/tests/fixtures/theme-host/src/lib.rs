pub mod widgets {
    pub struct Button;
    pub struct ButtonOptions {
        primary: bool,
        progress: u32,
    }
    impl ButtonOptions {
        pub fn primary(&self) -> bool {
            self.primary
        }
        pub fn progress(&self) -> u32 {
            self.progress
        }
    }
    impl crate::theme::ThemeTarget for Button {
        type Options = ButtonOptions;
    }

    pub struct Badge<T, const N: usize>(std::marker::PhantomData<T>);
    impl<T: 'static, const N: usize> crate::theme::ThemeTarget for Badge<T, N> {
        type Options = ();
    }
}

pub mod style {
    #[derive(Default)]
    pub struct Style;

    impl Style {
        pub fn new() -> Self {
            Self
        }
    }

    pub trait Styleable: Sized {
        fn opacity(self, _value: f32) -> Self {
            self
        }
    }

    impl Styleable for Style {}

    pub trait StyleableExt: Styleable {}
    impl<T: Styleable> StyleableExt for T {}
}

pub mod theme {
    use std::{any::Any, collections::HashSet, rc::Rc};

    pub use mue_macros::{class, theme};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Specificity(pub u32, pub u32, pub u32);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ClassName(&'static str);

    impl ClassName {
        pub const fn from_static_checked(value: &'static str) -> Self {
            const fn valid(value: &str) -> bool {
                let bytes = value.as_bytes();
                if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
                    return false;
                }
                let mut index = 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if !byte.is_ascii_alphanumeric() && byte != b'_' && byte != b'-' {
                        return false;
                    }
                    index += 1;
                }
                true
            }
            assert!(valid(value), "invalid class name");
            Self(value)
        }
    }

    pub trait ThemeTarget: 'static {
        type Options: 'static;
        fn key() -> std::any::TypeId
        where
            Self: Sized,
        {
            std::any::TypeId::of::<Self>()
        }
    }

    #[derive(Clone, Copy)]
    pub struct ComponentRef<'a> {
        key: std::any::TypeId,
        options: &'a dyn Any,
        classes: &'a HashSet<ClassName>,
        hovered: bool,
    }

    impl ComponentRef<'_> {
        pub fn is<T: ThemeTarget>(&self) -> bool {
            self.key == std::any::TypeId::of::<T>()
        }
        pub fn options<T: ThemeTarget>(&self) -> Option<&T::Options> {
            self.is::<T>()
                .then(|| self.options.downcast_ref())
                .flatten()
        }
        pub fn has_class(&self, name: &str) -> bool {
            self.classes.iter().any(|class| class.0 == name)
        }
        pub fn hovered(&self) -> bool {
            self.hovered
        }
    }

    #[derive(Clone)]
    pub struct Selector {
        _specificity: Specificity,
        identity: Option<std::any::TypeId>,
        predicate: Rc<dyn for<'a> Fn(ComponentRef<'a>) -> bool>,
    }
    impl Selector {
        pub fn new(
            specificity: Specificity,
            predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
        ) -> Self {
            Self::with_identity(None, specificity, predicate)
        }
        pub fn for_target<T: ThemeTarget>(
            specificity: Specificity,
            predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
        ) -> Self {
            Self::with_identity(Some(T::key()), specificity, predicate)
        }
        fn with_identity(
            identity: Option<std::any::TypeId>,
            specificity: Specificity,
            predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
        ) -> Self {
            Self {
                _specificity: specificity,
                identity,
                predicate: Rc::new(predicate),
            }
        }
        pub fn matches(&self, component: ComponentRef<'_>) -> bool {
            self.identity.is_none_or(|key| key == component.key) && (self.predicate)(component)
        }
    }

    pub struct Rule {
        selectors: Vec<Selector>,
        _declarations: Rc<dyn Fn() -> crate::style::Style>,
    }
    impl Rule {
        pub fn new(
            selectors: Vec<Selector>,
            declarations: impl Fn() -> crate::style::Style + 'static,
        ) -> Self {
            Self {
                selectors,
                _declarations: Rc::new(declarations),
            }
        }
        pub fn selector_count(&self) -> usize {
            self.selectors.len()
        }
    }

    pub struct Theme(Vec<Rule>);
    impl Theme {
        pub fn new(rules: Vec<Rule>) -> Self {
            Self(rules)
        }
        pub fn rule_count(&self) -> usize {
            self.0.len()
        }
    }
}
