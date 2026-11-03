#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
mod perf;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod visual_tests;

use super::{IntoNode, Node};
use crate::{
    style::Style,
    theme::{ClassName, ComponentRef, Theme, ThemeKey, ThemeTarget},
};
use mue_core::{
    prop::Prop,
    scope::{OwnedScope, Scope},
    signal::{Access, Signal, signal},
};
use std::{any::Any, cell::RefCell, collections::HashSet, rc::Rc};

#[cfg(test)]
pub(crate) mod resolver_reference;

thread_local! {
    static THEME: RefCell<Option<Prop<Theme>>> = const { RefCell::new(None) };
}

/// Restorable application input; it never contains a same-root collector.
#[doc(hidden)]
#[derive(Clone)]
pub struct AppBuildContext {
    theme: Option<Prop<Theme>>,
}
impl AppBuildContext {
    pub fn capture() -> Self {
        Self {
            theme: THEME.with_borrow(Clone::clone),
        }
    }
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        struct Restore(Option<Prop<Theme>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                THEME.with_borrow_mut(|theme| *theme = self.0.take());
            }
        }
        let _restore =
            Restore(THEME.with_borrow_mut(|theme| std::mem::replace(theme, self.theme.clone())));
        f()
    }
    pub(crate) fn with_theme<R>(theme: Prop<Theme>, f: impl FnOnce() -> R) -> R {
        assert!(
            THEME.with_borrow(Option::is_none),
            "themes can only be installed at the application root"
        );
        let context = Self { theme: Some(theme) };
        context.run(f)
    }
}

pub(crate) struct HoverOwner {
    pub(crate) scope: OwnedScope,
    pub(crate) signal: Signal<bool>,
}
impl HoverOwner {
    pub(crate) fn new() -> Rc<Self> {
        let scope = OwnedScope::new_inherited();
        let signal = scope.run(|| signal(false));
        Rc::new(Self { scope, signal })
    }
}

/// Preparation returns defaults and an owned consumer executed exactly once after seal.
pub struct NodePlan<F: FnOnce(&mut Style) + 'static> {
    defaults: Style,
    mount: F,
}
impl<F: FnOnce(&mut Style) + 'static> NodePlan<F> {
    pub fn new(defaults: Style, mount: F) -> Self {
        Self { defaults, mount }
    }
}

#[doc(hidden)]
pub trait PrepareNode: Sized {
    fn prepare(self, root: RootBuild) -> PreparedNode;
}
impl<F: FnOnce(&mut Style) + 'static> PrepareNode for NodePlan<F> {
    fn prepare(self, root: RootBuild) -> PreparedNode {
        root.finish(self)
    }
}
impl<F: FnOnce(&mut Style) + 'static> IntoNode for NodePlan<F> {
    fn into_node(self) -> Node {
        build(self)
    }
}

struct Target {
    key: ThemeKey,
    options: Rc<dyn Any>,
    classes: Prop<HashSet<ClassName>>,
}

/// Move-only continuation shared exclusively by a component's returned builder chain.
#[doc(hidden)]
pub struct RootBuild {
    owner: OwnedScope,
    environment: AppBuildContext,
    explicit: Style,
    last_classes: Prop<HashSet<ClassName>>,
    targets: Vec<Target>,
    hover_owners: Vec<Rc<HoverOwner>>,
}
impl Default for RootBuild {
    fn default() -> Self {
        Self::new()
    }
}
impl RootBuild {
    pub fn new() -> Self {
        Self {
            owner: OwnedScope::new_inherited(),
            environment: AppBuildContext::capture(),
            explicit: Style::new(),
            last_classes: Prop::Static(HashSet::new()),
            targets: Vec::new(),
            hover_owners: Vec::new(),
        }
    }
    pub fn scope(&self) -> Scope {
        self.owner.scope()
    }
    pub fn has_theme(&self) -> bool {
        self.environment.theme.is_some()
    }
    pub(super) fn into_untyped(self) -> (OwnedScope, AppBuildContext, Style, Vec<Rc<HoverOwner>>) {
        (
            self.owner,
            self.environment,
            self.explicit,
            self.hover_owners,
        )
    }
    pub fn push_style(&mut self, mut style: Style) {
        self.last_classes = style.classes.take().unwrap_or_default();
        for owner in std::mem::take(&mut style.hover_owners) {
            if !self
                .hover_owners
                .iter()
                .any(|previous| Rc::ptr_eq(previous, &owner))
            {
                self.hover_owners.push(owner);
            }
        }
        self.explicit.provide_defaults(style);
    }
    pub fn push_target<T: ThemeTarget>(&mut self, options: Rc<T::Options>) {
        self.targets.push(Target {
            key: T::key(),
            options,
            classes: self.last_classes.clone(),
        });
    }
    pub fn finish<F: FnOnce(&mut Style) + 'static>(mut self, plan: NodePlan<F>) -> PreparedNode {
        let scope = self.scope();
        let style = scope.run(|| {
            let NodePlan { mut defaults, .. } = plan;
            for owner in std::mem::take(&mut defaults.hover_owners) {
                if !self
                    .hover_owners
                    .iter()
                    .any(|previous| Rc::ptr_eq(previous, &owner))
                {
                    self.hover_owners.push(owner);
                }
            }
            if let Some(theme) = self
                .environment
                .theme
                .clone()
                .filter(|_| !self.targets.is_empty())
            {
                let hovered = self.hover_owners.first().map(|owner| *owner.signal);
                let targets = std::mem::take(&mut self.targets);
                let resolved =
                    mue_core::effect::computed_with_notify(move |previous: Option<&Rc<Style>>| {
                        #[cfg(test)]
                        crate::runtime::record_resolver();
                        let classes: Vec<_> = targets
                            .iter()
                            .map(|target| target.classes.get_clone())
                            .collect();
                        let components: Vec<_> = targets
                            .iter()
                            .zip(&classes)
                            .map(|(target, classes)| {
                                ComponentRef::new(
                                    target.key,
                                    target.options.as_ref(),
                                    classes,
                                    hovered,
                                )
                            })
                            .collect();
                        // Rule expressions may create derived signals. Keep only this generation.
                        let owner = scope.run(OwnedScope::new_inherited);
                        let mut result = owner.run(|| theme.get_clone().resolve(&components));
                        result.theme_owners.push(Rc::new(owner));
                        let notify = !result.is_visually_empty()
                            || previous.is_none_or(|style| !style.is_visually_empty());
                        #[cfg(test)]
                        let notify = notify || resolver_reference::enabled();
                        (Rc::new(result), notify)
                    });
                self.explicit.provide_themed_defaults(resolved, defaults);
            } else {
                self.explicit.provide_defaults(defaults);
            }
            std::mem::take(&mut self.explicit)
        });
        PreparedNode {
            owner: self.owner,
            environment: self.environment,
            style,
            hover_owners: self.hover_owners,
            mount: Box::new(plan.mount),
        }
    }
}

#[doc(hidden)]
pub struct PreparedNode {
    pub(super) owner: OwnedScope,
    pub(super) environment: AppBuildContext,
    pub(super) style: Style,
    pub(super) hover_owners: Vec<Rc<HoverOwner>>,
    pub(super) mount: Box<dyn FnOnce(&mut Style)>,
}
impl PreparedNode {
    pub fn mount(self) -> Node {
        Node::mount_prepared(self)
    }
}
/// Final conversion is independent for every real node, including side branches.
#[doc(hidden)]
pub fn build(builder: impl PrepareNode) -> Node {
    let root = RootBuild::new();
    let scope = root.scope();
    scope.run(|| builder.prepare(root).mount())
}
