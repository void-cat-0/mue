use crate::{
    node::{Component, IntoNode, Node},
    style::{Style, Styleable},
    theme::ThemeChain,
};
use mue_core::{
    effect::{computed, computed_always},
    prop::Prop,
    scope::{Scope, inject},
    signal::Access,
};
use std::{
    marker::PhantomData,
    mem,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_FACTORY_ID: AtomicUsize = AtomicUsize::new(1);
thread_local! {
    static FACTORY_STACK: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    static FACTORY_LOCATIONS: std::cell::RefCell<Vec<(Vec<Option<usize>>, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
    static SIBLING_CONTEXT: std::cell::RefCell<Option<(Vec<Option<usize>>, usize)>> = const { std::cell::RefCell::new(None) };
}
pub(crate) fn factory_ancestors() -> Vec<usize> {
    FACTORY_STACK.with_borrow(Clone::clone)
}
fn factory_ancestor_locations() -> Vec<(Vec<Option<usize>>, usize)> {
    FACTORY_LOCATIONS.with_borrow(Clone::clone)
}
pub(crate) fn with_sibling_context<T>(
    siblings: Vec<Option<usize>>,
    index: usize,
    f: impl FnOnce() -> T,
) -> T {
    let previous = SIBLING_CONTEXT.with_borrow_mut(|context| context.replace((siblings, index)));
    let result = f();
    SIBLING_CONTEXT.with_borrow_mut(|context| *context = previous);
    result
}
pub(crate) fn without_sibling_context<T>(f: impl FnOnce() -> T) -> T {
    let previous = SIBLING_CONTEXT.with_borrow_mut(|context| context.take());
    let result = f();
    SIBLING_CONTEXT.with_borrow_mut(|context| *context = previous);
    result
}
fn take_sibling_context() -> (Vec<Option<usize>>, usize) {
    SIBLING_CONTEXT
        .with_borrow_mut(|context| context.take())
        .unwrap_or_default()
}
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
    /// Expose stable component identity to style selectors.
    pub fn identity(&self) -> usize {
        self.id
    }

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
            classes: Vec::new(),
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
    classes: Vec<(String, Prop<bool>)>,
    style: Style,
    output: PhantomData<fn() -> R>,
}

impl<B, O, C, R> FactoryInstance<B, O, C, R> {
    /// Override this instance's options before theme and style resolution.
    pub fn options(mut self, change: impl FnOnce(&mut O)) -> Self {
        change(&mut self.options);
        self
    }

    /// Add or remove a named class reactively.
    pub fn class(mut self, name: impl Into<String>, active: impl Into<Prop<bool>>) -> Self {
        self.classes.push((name.into(), active.into()));
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
    fn factory_identity(&self) -> Option<usize> {
        Some(self.factory.id)
    }

    fn into_node(self) -> Node {
        let scope = Scope::new();
        let ancestors = factory_ancestors();
        let ancestor_locations = factory_ancestor_locations();
        let id = self.factory.id;
        let sibling_context = take_sibling_context();
        FACTORY_STACK.with_borrow_mut(|stack| stack.push(id));
        FACTORY_LOCATIONS.with_borrow_mut(|locations| locations.push(sibling_context.clone()));
        let node =
            scope.run(|| self.build_node(ancestors, ancestor_locations, id, sibling_context));
        FACTORY_LOCATIONS.with_borrow_mut(|locations| {
            locations.pop();
        });
        FACTORY_STACK.with_borrow_mut(|stack| {
            stack.pop();
        });
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
    fn build_node(
        self,
        ancestors: Vec<usize>,
        ancestor_locations: Vec<(Vec<Option<usize>>, usize)>,
        id: usize,
        sibling_context: (Vec<Option<usize>>, usize),
    ) -> Node {
        let Self {
            factory,
            content,
            options,
            classes,
            mut style,
            ..
        } = self;
        let hovered = mue_core::signal::signal(false);
        let mut root = (factory.base)(content).use_hovered(hovered);
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
            let class_list = computed(move |_| {
                classes
                    .iter()
                    .filter(|(_, active)| active.get())
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>()
                    .into()
            });
            let themed = computed_always(move |_| {
                themes.resolve(
                    id,
                    options.as_ref(),
                    class_list.into(),
                    *hovered,
                    ancestors.clone(),
                    sibling_context.0.clone(),
                    sibling_context.1,
                    ancestor_locations.clone(),
                )
            });
            style.provide_defaults(Style::themed_defaults(themed, defaults));
        }
        *root.style_mut() = style;
        root.into_node()
    }
}

/// Options and class controls exposed by generated named factory constructors.
pub trait FactoryUse: Component + Sized {
    type Options;
    fn options(self, change: impl FnOnce(&mut Self::Options)) -> Self;
    fn class(self, name: impl Into<String>, active: impl Into<Prop<bool>>) -> Self;
}

impl<B, O, C, R> FactoryUse for FactoryInstance<B, O, C, R>
where
    FactoryInstance<B, O, C, R>: Component,
{
    type Options = O;
    fn options(mut self, change: impl FnOnce(&mut O)) -> Self {
        change(&mut self.options);
        self
    }
    fn class(mut self, name: impl Into<String>, active: impl Into<Prop<bool>>) -> Self {
        self.classes.push((name.into(), active.into()));
        self
    }
}
