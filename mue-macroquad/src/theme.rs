use std::{
    any::{Any, TypeId},
    borrow::{Borrow, Cow},
    collections::{HashMap, HashSet},
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

#[cfg(test)]
mod tests;

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
    identity: Option<ThemeKey>,
    predicate: Rc<dyn for<'a> Fn(ComponentRef<'a>) -> bool>,
}
impl Selector {
    /// An unrestricted predicate. Kept in the common candidate bucket.
    pub fn new(
        specificity: Specificity,
        predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
    ) -> Self {
        Self::with_identity(None, specificity, predicate)
    }
    /// Matches only this concrete identity, then evaluates the remaining conditions.
    /// The identity is enforced even when the predicate does not check it.
    pub fn for_target<T: ThemeTarget>(
        specificity: Specificity,
        predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
    ) -> Self {
        Self::with_identity(Some(T::key()), specificity, predicate)
    }
    fn with_identity(
        identity: Option<ThemeKey>,
        specificity: Specificity,
        predicate: impl for<'a> Fn(ComponentRef<'a>) -> bool + 'static,
    ) -> Self {
        Self {
            specificity,
            identity,
            predicate: Rc::new(predicate),
        }
    }
    pub fn specificity(&self) -> Specificity {
        self.specificity
    }
    pub fn matches(&self, component: ComponentRef<'_>) -> bool {
        self.identity.is_none_or(|key| key == component.key) && (self.predicate)(component)
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
#[derive(Clone)]
pub struct Theme {
    rules: Rc<[Rule]>,
    indexed: Rc<RuleIndex>,
}
#[derive(Default)]
struct RuleIndex {
    by_identity: HashMap<ThemeKey, Vec<usize>>,
    generic: Vec<usize>,
}
impl RuleIndex {
    fn build(rules: &[Rule]) -> Self {
        let mut index = Self::default();
        for (rule_index, rule) in rules.iter().enumerate() {
            for selector in &rule.selectors {
                let bucket = if let Some(key) = selector.identity {
                    index.by_identity.entry(key).or_default()
                } else {
                    &mut index.generic
                };
                // Rules are visited in source order; repeated branches need no set.
                if bucket.last() != Some(&rule_index) {
                    bucket.push(rule_index);
                }
            }
        }
        index
    }
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            rules: Rc::from([]),
            indexed: Rc::new(RuleIndex::default()),
        }
    }
}
impl Theme {
    pub fn new(rules: Vec<Rule>) -> Self {
        if rules.is_empty() {
            return Self::default();
        }
        let indexed = Rc::new(RuleIndex::build(&rules));
        Self {
            rules: rules.into(),
            indexed,
        }
    }
    pub fn compose(themes: impl IntoIterator<Item = Theme>) -> Self {
        Self::new(
            themes
                .into_iter()
                .flat_map(|theme| theme.rules.to_vec())
                .collect(),
        )
    }
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
    fn candidate_indices(&self, components: &[ComponentRef<'_>]) -> Cow<'_, [usize]> {
        if components.is_empty() {
            return Cow::Borrowed(&[]);
        }
        let generic = self.indexed.generic.as_slice();
        if generic.len() == self.rules.len() {
            return Cow::Borrowed(generic);
        }
        let mut buckets = smallvec::SmallVec::<[&[usize]; 9]>::new();
        if !generic.is_empty() {
            buckets.push(generic);
        }
        // Keep short return chains on the stack, without quadratic work on long chains.
        let mut seen = (components.len() > 8).then(HashSet::new);
        for (position, component) in components.iter().enumerate() {
            let duplicate = if let Some(seen) = &mut seen {
                !seen.insert(component.key)
            } else {
                components[..position]
                    .iter()
                    .any(|previous| previous.key == component.key)
            };
            if duplicate {
                continue;
            }
            if let Some(indices) = self.indexed.by_identity.get(&component.key) {
                if indices.len() == self.rules.len() {
                    return Cow::Borrowed(indices);
                }
                buckets.push(indices);
            }
        }
        match buckets.as_slice() {
            [] => Cow::Borrowed(&[]),
            [indices] => Cow::Borrowed(indices),
            _ => {
                // Reserve once for the union instead of growing a cloned first bucket.
                let mut indices =
                    Vec::with_capacity(buckets.iter().map(|bucket| bucket.len()).sum());
                for bucket in buckets {
                    indices.extend_from_slice(bucket);
                }
                indices.sort_unstable();
                indices.dedup();
                Cow::Owned(indices)
            }
        }
    }
    fn resolve_indices(
        &self,
        components: &[ComponentRef<'_>],
        indices: impl IntoIterator<Item = usize>,
    ) -> Style {
        let mut matched: Vec<_> = indices
            .into_iter()
            .filter_map(|order| {
                let rule = &self.rules[order];
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
    pub fn resolve(&self, components: &[ComponentRef<'_>]) -> Style {
        let candidates = self.candidate_indices(components);
        self.resolve_indices(components, candidates.iter().copied())
    }
    #[cfg(test)]
    pub(crate) fn resolve_linear(&self, components: &[ComponentRef<'_>]) -> Style {
        self.resolve_indices(components, 0..self.rules.len())
    }
}
impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.rules, &other.rules)
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
