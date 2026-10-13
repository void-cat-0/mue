use crate::{
    factory::Factory,
    node::{IntoNode, Node},
    style::Style,
};
use mue_core::{
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
}
impl<'a> ComponentRef<'a> {
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
pub struct Theme(Rc<dyn for<'a> Fn(ComponentRef<'a>, Style) -> Style>);
pub fn theme(f: impl for<'a> Fn(ComponentRef<'a>, Style) -> Style + 'static) -> Theme {
    Theme(Rc::new(f))
}

impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
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
    pub(crate) fn resolve(&self, id: usize, options: &dyn Any) -> Rc<Style> {
        let reference = ComponentRef { id, options };
        let mut combined = Style::new();
        for source in &self.0 {
            let layer = (source.get().0)(reference, Style::new());
            layer.assert_visual_only();
            combined.provide_defaults(layer);
        }
        Rc::new(combined)
    }
}

pub struct ThemeProvider<F> {
    theme: ThemeSource,
    view: F,
}

/// Enter a theme scope before constructing the enclosed view.
pub fn provide_theme<T>(
    theme: impl Into<ThemeSource>,
    view: impl FnOnce() -> T,
) -> ThemeProvider<impl FnOnce() -> T> {
    ThemeProvider {
        theme: theme.into(),
        view,
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
