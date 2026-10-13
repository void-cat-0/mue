use crate::{
    node::{Component, IntoNode, Node},
    style::{Style, Styleable},
    theme::ThemeChain,
};
use mue_core::{
    effect::computed_always,
    scope::{Scope, inject},
};
use std::{
    marker::PhantomData,
    mem,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_FACTORY_ID: AtomicUsize = AtomicUsize::new(1);
fn next_id() -> usize {
    NEXT_FACTORY_ID.fetch_add(1, Ordering::Relaxed)
}

type StyleCallback<O> = dyn Fn(Style, &O) -> Style;

/// A cheap, identity-preserving handle to a reusable component definition.
pub struct Factory<B, O = ()> {
    base: Rc<B>,
    defaults: O,
    style: Rc<StyleCallback<O>>,
    pub(crate) id: usize,
    styled: bool,
}

impl<B, O: Clone> Clone for Factory<B, O> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            defaults: self.defaults.clone(),
            style: self.style.clone(),
            id: self.id,
            styled: self.styled,
        }
    }
}

/// Define a reusable component constructor with a stable identity for themes.
pub fn factory<B>(base: B) -> Factory<B> {
    Factory {
        base: Rc::new(base),
        defaults: (),
        style: Rc::new(|s, _| s),
        id: next_id(),
        styled: false,
    }
}

impl<B, O> Factory<B, O> {
    /// Set default instance options before adding a style callback.
    pub fn defaults<P: Clone + 'static>(self, defaults: P) -> Factory<B, P> {
        assert!(!self.styled, "set factory defaults before style");
        Factory {
            base: self.base,
            defaults,
            style: Rc::new(|s, _| s),
            id: next_id(),
            styled: false,
        }
    }

    /// Set visual defaults from the final instance options.
    pub fn style(self, style: impl Fn(Style, &O) -> Style + 'static) -> Self {
        Self {
            base: self.base,
            defaults: self.defaults,
            style: Rc::new(style),
            id: next_id(),
            styled: true,
        }
    }
}

impl<B, O: Clone> Factory<B, O> {
    /// Create a styleable instance. Pass its content here, including child lists.
    /// Instance styles override themes, factory defaults, and base styles.
    pub fn create<C, I, R>(&self, content: C) -> FactoryInstance<B, O, I, R>
    where
        C: Into<I>,
        B: Fn(I) -> R,
        R: Component,
    {
        FactoryInstance {
            factory: self.clone(),
            content: content.into(),
            options: self.defaults.clone(),
            style: Style::new(),
            output: PhantomData,
        }
    }
}

/// A component instance awaiting node construction, with content and options.
pub struct FactoryInstance<B, O, C, R> {
    factory: Factory<B, O>,
    content: C,
    options: O,
    style: Style,
    output: PhantomData<fn() -> R>,
}

impl<B, O, C, R> FactoryInstance<B, O, C, R> {
    /// Override this instance's options before theme and style resolution.
    pub fn options(mut self, change: impl FnOnce(&mut O)) -> Self {
        change(&mut self.options);
        self
    }
}

impl<B, O, C, R> Styleable for FactoryInstance<B, O, C, R> {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    fn children(self, _children: impl crate::node::IntoChildren) -> Self {
        panic!(
            "factory instances receive their content through create(...); children(...) would replace the component tree"
        )
    }
}

impl<B, O, C, R> IntoNode for FactoryInstance<B, O, C, R>
where
    B: Fn(C) -> R,
    R: Component,
    O: Clone + 'static,
{
    fn into_node(self) -> Node {
        let scope = Scope::new();
        let node = scope.run(|| self.build_node());
        node.retain_scope(scope);
        node
    }
}

impl<B, O, C, R> FactoryInstance<B, O, C, R>
where
    B: Fn(C) -> R,
    R: Component,
    O: Clone + 'static,
{
    fn build_node(self) -> Node {
        let Self {
            factory,
            content,
            options,
            mut style,
            ..
        } = self;
        let mut root = (factory.base)(content);
        let base_style = mem::take(root.style_mut());
        let factory_style = (factory.style)(Style::new(), &options);
        factory_style.assert_visual_only();
        let mut defaults = factory_style;
        defaults.provide_defaults(base_style);
        let themes = inject::<ThemeChain>().unwrap_or_default();
        if themes.0.is_empty() {
            style.provide_defaults(defaults);
        } else {
            let options = Rc::new(options);
            let id = factory.id;
            let themed = computed_always(move |_| themes.resolve(id, options.as_ref()));
            style.provide_defaults(Style::themed_defaults(themed, defaults));
        }
        *root.style_mut() = style;
        root.into_node()
    }
}
