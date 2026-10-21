use crate::{
    factory::Factory,
    node::{IntoNode, Node},
    shader::Brush,
    style::Style,
};
use mue_core::{
    prop::Prop,
    scope::{Scope, inject},
    signal::{Access, ReadSignal, Signal},
};
use std::{any::Any, rc::Rc};

pub use mue_macros::theme;

/// Read-only view of the factory identity and this instance's final options.
#[derive(Clone, Copy)]
pub struct ComponentRef<'a> {
    id: usize,
    options: &'a dyn Any,
    classes: &'a [String],
    hovered: ReadSignal<bool>,
    ancestors: &'a [usize],
    siblings: &'a [Option<usize>],
    sibling_index: usize,
    ancestor_locations: &'a [(Vec<Option<usize>>, usize)],
}
impl<'a> ComponentRef<'a> {
    /// Check whether this instance is currently hovered.
    pub fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    /// Match a selector chain against this component and its factory ancestors.
    /// Relations use 0 for descendant, 1 for child, 2 for adjacent sibling, and 3 for sibling.
    pub fn matches_chain(&self, factories: &[usize], relations: &[u8]) -> bool {
        if factories.is_empty()
            || relations.len() + 1 != factories.len()
            || (*factories.last().unwrap() != 0 && *factories.last().unwrap() != self.id)
        {
            return false;
        }
        let mut current = self.ancestors.len();
        let mut sibling_ids = self.siblings;
        let mut sibling_index = self.sibling_index;
        for index in (0..relations.len()).rev() {
            let target = factories[index];
            match relations[index] {
                0 | 1 => {
                    if relations[index] == 1 {
                        if current == 0 || (target != 0 && self.ancestors[current - 1] != target) {
                            return false;
                        }
                        current -= 1;
                    } else {
                        let Some(found) = self.ancestors[..current]
                            .iter()
                            .rposition(|id| target == 0 || *id == target)
                        else {
                            return false;
                        };
                        current = found;
                        sibling_ids = &self.ancestor_locations[found].0;
                        sibling_index = self.ancestor_locations[found].1;
                    }
                    if relations[index] == 1 {
                        sibling_ids = &self.ancestor_locations[current].0;
                        sibling_index = self.ancestor_locations[current].1;
                    }
                }
                2 => {
                    if sibling_index == 0 {
                        return false;
                    }
                    sibling_index -= 1;
                    if !sibling_ids
                        .get(sibling_index)
                        .is_some_and(|id| id.is_some_and(|id| target == 0 || id == target))
                    {
                        return false;
                    }
                }
                3 => {
                    let Some(found) = sibling_ids[..sibling_index]
                        .iter()
                        .rposition(|id| id.is_some_and(|id| target == 0 || id == target))
                    else {
                        return false;
                    };
                    sibling_index = found;
                }
                _ => return false,
            }
        }
        true
    }

    /// Check whether this instance was created by a named factory.
    pub fn is<B, O: 'static>(&self, target: &Factory<B, O>) -> bool {
        self.id == target.id
    }

    /// Check a dynamic class flag attached to this factory instance.
    pub fn has_class(&self, name: &str) -> bool {
        self.classes.iter().any(|class| class == name)
    }

    /// Return this instance's options if it belongs to the target factory.
    pub fn options<B, O: 'static>(&self, target: &Factory<B, O>) -> Option<&'a O> {
        if self.id == target.id {
            Some(
                self.options
                    .downcast_ref::<O>()
                    .expect("factory identity/options type mismatch"),
            )
        } else {
            None
        }
    }
}

#[derive(Clone)]
/// A visual-style callback selected by factory identity and instance options.
pub struct Theme(Rc<dyn for<'a> Fn(ComponentRef<'a>, Style) -> Style>);
/// Build a theme callback. It may set visual properties, but not children or hooks.
pub fn theme(f: impl for<'a> Fn(ComponentRef<'a>, Style) -> Style + 'static) -> Theme {
    Theme(Rc::new(f))
}

impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
/// A fixed theme or a reactive theme signal.
pub enum ThemeSource {
    Static(Theme),
    Dynamic(ReadSignal<Theme>),
}
impl ThemeSource {
    fn get(&self) -> Theme {
        match self {
            Self::Static(value) => value.clone(),
            Self::Dynamic(value) => value.get_clone(),
        }
    }
}
impl From<Theme> for ThemeSource {
    fn from(value: Theme) -> Self {
        Self::Static(value)
    }
}
impl From<ReadSignal<Theme>> for ThemeSource {
    fn from(value: ReadSignal<Theme>) -> Self {
        Self::Dynamic(value)
    }
}
impl From<Signal<Theme>> for ThemeSource {
    fn from(value: Signal<Theme>) -> Self {
        Self::Dynamic(*value)
    }
}

#[derive(Clone, Default)]
pub(crate) struct ThemeChain(pub(crate) Vec<ThemeSource>);
impl ThemeChain {
    pub(crate) fn resolve(
        &self,
        id: usize,
        options: &dyn Any,
        classes: Prop<Rc<[String]>>,
        hovered: ReadSignal<bool>,
        ancestors: Vec<usize>,
        siblings: Vec<Option<usize>>,
        sibling_index: usize,
        ancestor_locations: Vec<(Vec<Option<usize>>, usize)>,
    ) -> Rc<Style> {
        let classes = classes.get_clone();
        let reference = ComponentRef {
            id,
            options,
            classes: &classes,
            hovered,
            ancestors: &ancestors,
            siblings: &siblings,
            sibling_index,
            ancestor_locations: &ancestor_locations,
        };
        let mut combined = Style::new();
        for source in &self.0 {
            let layer = (source.get().0)(reference, Style::new());
            layer.assert_visual_only();
            combined.provide_defaults(layer);
        }
        Rc::new(combined)
    }
}

/// A view constructed inside a theme scope.
pub struct ThemeProvider<F> {
    theme: ThemeSource,
    view: F,
}

/// Enter a theme scope before constructing the enclosed view.
/// Nested themes take precedence; explicit instance styles take precedence over both.
pub fn provide_theme<T>(
    theme: impl Into<ThemeSource>,
    view: impl FnOnce() -> T,
) -> ThemeProvider<impl FnOnce() -> T> {
    ThemeProvider {
        theme: theme.into(),
        view,
    }
}

#[derive(Clone)]
struct Foreground(Prop<Brush>);

/// Build a subtree whose text uses `color` when no local text color is set.
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

/// Return the text color inherited from the nearest foreground provider.
pub fn inherited_foreground() -> Option<Prop<Brush>> {
    inject::<Foreground>().map(|foreground| foreground.0)
}

impl<F, R> IntoNode for ForegroundProvider<F>
where
    F: FnOnce() -> R,
    R: IntoNode,
{
    fn into_node(self) -> Node {
        let scope = Scope::new();
        scope.provide(Foreground(self.color));
        let node = scope.run(|| (self.view)().into_node());
        node.retain_scope(scope);
        node
    }
}

impl<F, R> IntoNode for ThemeProvider<F>
where
    F: FnOnce() -> R,
    R: IntoNode,
{
    fn into_node(self) -> Node {
        let scope = Scope::new();
        let mut themes = inject::<ThemeChain>().unwrap_or_default();
        themes.0.insert(0, self.theme);
        scope.provide(themes);
        let node = scope.run(|| (self.view)().into_node());
        node.retain_scope(scope);
        node
    }
}

/// Match static and reactive boolean option values in a selector.
pub trait SelectorValue {
    fn is_set(&self) -> bool;
}
impl SelectorValue for bool {
    fn is_set(&self) -> bool {
        *self
    }
}
impl SelectorValue for Prop<bool> {
    fn is_set(&self) -> bool {
        self.get()
    }
}
impl SelectorValue for ReadSignal<bool> {
    fn is_set(&self) -> bool {
        self.get()
    }
}
impl SelectorValue for Signal<bool> {
    fn is_set(&self) -> bool {
        self.get()
    }
}
