use std::{
    any::{Any, TypeId},
    borrow::{Borrow, Cow},
    collections::HashSet,
    fmt,
    rc::Rc,
};

use crate::{
    node::{IntoNode, Node},
    shader::Brush,
    style::{Style, Styleable},
};
use mue_core::{
    prop::{Prop, PropValue},
    scope::{OwnedScope, inject},
    signal::{Access, ReadSignal},
};

pub use mue_macros::{class, theme};

/// A validated, case-sensitive class name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClassName(Cow<'static, str>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidClassName;
impl fmt::Display for InvalidClassName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("class names must match [A-Za-z][A-Za-z0-9_-]*")
    }
}
impl std::error::Error for InvalidClassName {}
impl ClassName {
    const fn valid(value: &str) -> bool {
        let bytes = value.as_bytes();
        if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
            return false;
        }
        let mut i = 1;
        while i < bytes.len() {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'_' && bytes[i] != b'-' {
                return false;
            }
            i += 1;
        }
        true
    }
    #[doc(hidden)]
    pub const fn from_static_checked(value: &'static str) -> Self {
        assert!(Self::valid(value), "invalid class name");
        Self(Cow::Borrowed(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ClassName {
    type Error = InvalidClassName;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Self::valid(&value) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(InvalidClassName)
        }
    }
}
impl TryFrom<&str> for ClassName {
    type Error = InvalidClassName;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}
impl Borrow<str> for ClassName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for ClassName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl PropValue for ClassName {}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThemeKey(TypeId);
impl ThemeKey {
    pub fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// Declaration identity is the concrete builder type, including generic arguments.
pub trait ThemeTarget: 'static {
    type Options: 'static;
    fn key() -> ThemeKey
    where
        Self: Sized,
    {
        ThemeKey::of::<Self>()
    }
}
pub trait Themeable: ThemeTarget + Styleable {
    fn classes(mut self, value: impl Into<Prop<HashSet<ClassName>>>) -> Self {
        self.style_mut().classes = Some(value.into());
        self
    }
    fn enable_hovered(mut self) -> Self {
        self.style_mut().ensure_hovered();
        self
    }
}
impl<T: ThemeTarget + Styleable> Themeable for T {}

#[derive(Clone, Copy)]
pub struct ComponentRef<'a> {
    key: ThemeKey,
    options: &'a dyn Any,
    classes: &'a HashSet<ClassName>,
    hovered: Option<ReadSignal<bool>>,
}
impl<'a> ComponentRef<'a> {
    #[doc(hidden)]
    pub fn new(
        key: ThemeKey,
        options: &'a dyn Any,
        classes: &'a HashSet<ClassName>,
        hovered: Option<ReadSignal<bool>>,
    ) -> Self {
        Self {
            key,
            options,
            classes,
            hovered,
        }
    }
    pub fn is<T: ThemeTarget>(&self) -> bool {
        self.key == T::key()
    }
    pub fn options<T: ThemeTarget>(&self) -> Option<&'a T::Options> {
        self.is::<T>()
            .then(|| self.options.downcast_ref::<T::Options>())
            .flatten()
    }
    pub fn has_class(&self, name: &str) -> bool {
        self.classes.contains(name)
    }
    pub fn hovered(&self) -> bool {
        self.hovered.is_some_and(|hovered| hovered.get())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

#[derive(Clone)]
pub struct Selector {
    specificity: Specificity,
    predicate: Rc<dyn for<'a> Fn(ComponentRef<'a>) -> bool>,
}
impl Selector {
    pub fn new(
        specificity: Specificity,
        predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
    ) -> Self {
        Self {
            specificity,
            predicate: Rc::new(predicate),
        }
    }
    pub fn specificity(&self) -> Specificity {
        self.specificity
    }
    pub fn matches(&self, component: ComponentRef<'_>) -> bool {
        (self.predicate)(component)
    }
}
#[derive(Clone)]
pub struct Rule {
    selectors: Vec<Selector>,
    declarations: Rc<dyn Fn() -> Style>,
}
impl Rule {
    pub fn new(selectors: Vec<Selector>, declarations: impl Fn() -> Style + 'static) -> Self {
        assert!(!selectors.is_empty(), "theme rule requires a selector");
        Self {
            selectors,
            declarations: Rc::new(declarations),
        }
    }
    pub fn selectors(&self) -> &[Selector] {
        &self.selectors
    }
}
/// Ordered sparse declarations; composing modules appends their source order.
#[derive(Clone, Default)]
pub struct Theme(Rc<[Rule]>);
impl Theme {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self(rules.into())
    }
    pub fn compose(themes: impl IntoIterator<Item = Theme>) -> Self {
        Self::new(
            themes
                .into_iter()
                .flat_map(|theme| theme.0.to_vec())
                .collect(),
        )
    }
    pub fn rules(&self) -> &[Rule] {
        &self.0
    }
    pub fn resolve(&self, components: &[ComponentRef<'_>]) -> Style {
        // One candidate per rule, weighted only by its actually matched branches.
        let mut matched: Vec<_> = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(order, rule)| {
                rule.selectors
                    .iter()
                    .filter(|selector| {
                        components
                            .iter()
                            .any(|component| selector.matches(*component))
                    })
                    .map(Selector::specificity)
                    .max()
                    .map(|weight| (weight, order, rule))
            })
            .collect();
        matched.sort_by_key(|(weight, order, _)| (*weight, *order));
        let mut result = Style::new();
        for (_, _, rule) in matched {
            let mut layer = (rule.declarations)();
            layer.assert_visual_only();
            layer.provide_defaults(result);
            result = layer;
        }
        result
    }
}
impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl PropValue for Theme {}

#[derive(Clone)]
struct Foreground(Prop<Brush>);
/// Build a subtree with an inherited foreground, independent of theme rules.
pub fn provide_foreground<T>(
    color: impl Into<Prop<Brush>>,
    view: impl FnOnce() -> T,
) -> ForegroundProvider<impl FnOnce() -> T> {
    ForegroundProvider {
        color: color.into(),
        view,
    }
}
pub struct ForegroundProvider<F> {
    color: Prop<Brush>,
    view: F,
}
pub fn inherited_foreground() -> Option<Prop<Brush>> {
    inject::<Foreground>().map(|foreground| foreground.0)
}
impl<F, R> IntoNode for ForegroundProvider<F>
where
    F: FnOnce() -> R,
    R: IntoNode,
{
    fn into_node(self) -> Node {
        let scope = OwnedScope::new_inherited();
        scope.provide(Foreground(self.color));
        let node = scope.run(|| (self.view)().into_node());
        node.retain_owned_scope(scope);
        node
    }
}
