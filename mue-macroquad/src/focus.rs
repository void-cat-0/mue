//! Keyboard focus: one focused node per thread, Tab order, and focus scopes.
//!
//! A node takes focus when it declares `focusable(true)`. Pressing a focusable
//! node focuses it; pressing anywhere else blurs. Tab and Shift+Tab move focus
//! through focusable nodes in tree order (positive `tab_index` values first, in
//! ascending order; negative values are skipped by Tab but can still be focused
//! by pointer or code). A node with `focus_scope(FocusScope::Contain)` keeps Tab
//! inside its subtree while focus is there, and restores the previous focus
//! when it is removed or hidden. `auto_focus(true)` focuses the node, or its
//! first focusable descendant, each time it becomes displayed with a non-empty
//! area. When the focused node stops being displayed (removed, `display:
//! none`, a closed `presence`), focus moves back to the node that had it
//! before focus entered the enclosing `Contain` scope, or is cleared.
//!
//! Key and character events follow the focus path; see
//! [`crate::event::keyboard`]. Read focus state with [`focused`],
//! [`focus_visible`], [`StyleableExt::with_focus`](crate::style::StyleableExt::with_focus),
//! or [`use_focus`] inside a node body.
//!
//! "Focus visible" is true when focus last moved by keyboard; show focus rings
//! only then, as browsers do for `:focus-visible`.

use std::{cell::RefCell, rc::Rc};

use indexmap::IndexSet;
use mue_core::{
    prop::{Prop, PropValue},
    scope::{OwnedScope, Scope},
    signal::{Access, ReadSignal, Signal, signal},
};

use crate::{
    event::keyboard::{CharEvent, FocusEvent, FocusReason, KeyCode, KeyEvent},
    hook::{CallbackList, NodeHooks},
    node::{NodeId, NodeInner, NodeRef},
    runtime::Runtime,
};

/// How a node treats focus inside its subtree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusScope {
    #[default]
    None,
    /// Keep Tab navigation inside this subtree and restore the previously
    /// focused node when this node is removed. Use it for dialogs and menus.
    Contain,
}
impl PropValue for FocusScope {}

/// Reactive focus state of one node.
#[derive(Clone, Copy)]
pub struct FocusState {
    /// The node itself has focus.
    pub focused: ReadSignal<bool>,
    /// The node has focus and focus moved by keyboard (show a focus ring).
    pub visible: ReadSignal<bool>,
}

/// Owns a node's focus signals so styles can bind them before the node exists.
pub(crate) struct FocusOwner {
    _scope: OwnedScope,
    pub(crate) focused: Signal<bool>,
    pub(crate) visible: Signal<bool>,
}

impl FocusOwner {
    pub(crate) fn new() -> Rc<Self> {
        let scope = OwnedScope::new_inherited();
        let (focused, visible) = scope.run(|| (signal(false), signal(false)));
        Rc::new(Self {
            _scope: scope,
            focused,
            visible,
        })
    }

    pub(crate) fn state(&self) -> FocusState {
        FocusState {
            focused: *self.focused,
            visible: *self.visible,
        }
    }
}

/// Focus declarations kept on a node that opted into focus.
pub(crate) struct FocusSlot {
    pub focusable: Prop<bool>,
    pub tab_index: Prop<i32>,
    pub scope: Prop<FocusScope>,
    pub owners: Vec<Rc<FocusOwner>>,
    /// For `Contain` scopes: the node focused before focus entered the scope.
    pub return_to: Rc<std::cell::Cell<Option<NodeRef>>>,
}

struct Global {
    focused: Signal<Option<NodeRef>>,
    visible: Signal<bool>,
    roots: RefCell<Vec<NodeRef>>,
    /// Nodes with `auto_focus` and whether they were displayed last frame.
    auto_focus: RefCell<Vec<(NodeRef, bool)>>,
    /// Restore targets of `Contain` scopes removed since the last frame.
    pending_restore: RefCell<Vec<NodeRef>>,
    /// The innermost `Contain` scope around the focused node, recorded when
    /// focus moved (the scope may be detached later, when it is hidden).
    current_scope: std::cell::Cell<Option<NodeRef>>,
}

thread_local! {
    static GLOBAL: Global = Scope::null().run(|| Global {
        focused: Signal::new(None),
        visible: Signal::new(false),
        roots: RefCell::new(Vec::new()),
        auto_focus: RefCell::new(Vec::new()),
        pending_restore: RefCell::new(Vec::new()),
        current_scope: std::cell::Cell::new(None),
    });
}

/// The focused node, if it is still alive. Tracked.
pub fn focused() -> Option<NodeRef> {
    GLOBAL
        .with(|global| global.focused.get())
        .filter(NodeRef::is_live)
}

fn focused_untracked() -> Option<NodeRef> {
    GLOBAL
        .with(|global| global.focused.get_untracked())
        .filter(NodeRef::is_live)
}

/// The focused node as a read-only signal. It may name a node that has since
/// been removed; prefer [`focused`] for a checked read.
pub fn focused_signal() -> ReadSignal<Option<NodeRef>> {
    GLOBAL.with(|global| *global.focused)
}

/// Whether focus last moved by keyboard. Tracked.
pub fn focus_visible() -> ReadSignal<bool> {
    GLOBAL.with(|global| *global.visible)
}

/// Focus `node` programmatically. Returns false when the node is not focusable.
pub fn focus(node: NodeRef) -> bool {
    if !is_focusable(node) {
        return false;
    }
    set_focus(Some(node), FocusReason::Programmatic);
    true
}

/// Remove focus from whichever node has it.
pub fn blur() {
    set_focus(None, FocusReason::Programmatic);
}

/// Move focus to the next (or previous) tabbable node, wrapping around.
/// Returns false when nothing is tabbable. The move counts as keyboard
/// navigation, so focus becomes visible.
pub fn move_focus(forward: bool) -> bool {
    let current = focused_untracked();
    let scope = current
        .and_then(containing_scope)
        .or_else(topmost_contain_scope);
    let order = tab_order(scope);
    if order.is_empty() {
        return false;
    }
    let index = current.and_then(|node| order.iter().position(|n| *n == node));
    let next = match (index, forward) {
        (Some(i), true) => order[(i + 1) % order.len()],
        (Some(i), false) => order[(i + order.len() - 1) % order.len()],
        (None, true) => order[0],
        (None, false) => order[order.len() - 1],
    };
    set_focus(Some(next), FocusReason::Keyboard);
    true
}

/// Focus `node` if focusable, otherwise its first tabbable descendant.
pub fn focus_within(node: NodeRef) -> bool {
    if is_focusable(node) {
        set_focus(Some(node), FocusReason::Programmatic);
        return true;
    }
    match tab_order(Some(node)).first() {
        Some(&first) => {
            set_focus(Some(first), FocusReason::Programmatic);
            true
        }
        None => false,
    }
}

/// Focus signals for the node being built, creating them on first use.
pub fn use_focus() -> FocusState {
    NodeInner::with_mut(|node| {
        let slot = node.focus.get_or_insert_with(|| {
            Box::new(FocusSlot {
                focusable: Prop::Static(false),
                tab_index: Prop::Static(0),
                scope: Prop::Static(FocusScope::None),
                owners: Vec::new(),
                return_to: Rc::default(),
            })
        });
        if slot.owners.is_empty() {
            slot.owners.push(FocusOwner::new());
        }
        slot.owners[0].state()
    })
}

impl NodeRef {
    /// Focus this node. Returns false when it is not focusable.
    pub fn focus(&self) -> bool {
        focus(*self)
    }

    /// Remove focus if this node has it.
    pub fn blur(&self) {
        if focused() == Some(*self) {
            blur();
        }
    }

    /// Whether this node has focus. Tracked.
    pub fn is_focused(&self) -> bool {
        focused() == Some(*self)
    }
}

fn slot_info(node: NodeRef) -> Option<(bool, i32, FocusScope)> {
    Runtime::try_with(|rt| {
        let nodes = rt.nodes.borrow();
        let slot = nodes.get(node.id)?.focus.as_ref()?;
        Some((
            slot.focusable.get_untracked(),
            slot.tab_index.get_untracked(),
            slot.scope.get_untracked(),
        ))
    })
    .ok()
    .flatten()
}

fn is_focusable(node: NodeRef) -> bool {
    slot_info(node).is_some_and(|(focusable, _, _)| focusable)
}

/// Whether a node takes part in pointer hit testing because it can take focus.
pub(crate) fn declares_focus(inner: &NodeInner) -> bool {
    inner
        .focus
        .as_ref()
        .is_some_and(|slot| !matches!(slot.focusable, Prop::Static(false)))
}

fn children_of(node: NodeRef) -> Rc<[NodeRef]> {
    Runtime::with(|rt| {
        rt.nodes
            .borrow()
            .get(node.id)
            .map(|inner| *inner.children)
            .filter(|children| !children.is_null())
            .and_then(|children| children.try_get_clone_untracked())
    })
    .unwrap_or_else(|| Rc::from([]))
}

fn has_area(node: NodeRef) -> bool {
    node.rect()
        .and_then(|rect| rect.try_get_untracked())
        .is_none_or(|rect| rect.w > 0. && rect.h > 0.)
}

/// The path from `target` up to its root, deepest first, within the displayed roots.
pub(crate) fn path_to(target: NodeRef) -> Option<Vec<NodeRef>> {
    fn visit(node: NodeRef, target: NodeRef, path: &mut Vec<NodeRef>) -> bool {
        path.push(node);
        if node == target {
            return true;
        }
        for &child in children_of(node).iter() {
            if visit(child, target, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let roots = GLOBAL.with(|global| global.roots.borrow().clone());
    for root in roots.into_iter().rev() {
        let mut path = Vec::new();
        if visit(root, target, &mut path) {
            path.reverse();
            return Some(path);
        }
    }
    None
}

fn containing_scope(node: NodeRef) -> Option<NodeRef> {
    path_to(node)?.into_iter().find(|ancestor| {
        slot_info(*ancestor).is_some_and(|(_, _, scope)| scope == FocusScope::Contain)
    })
}

/// The last `Contain` scope in paint order, used when nothing is focused yet.
fn topmost_contain_scope() -> Option<NodeRef> {
    fn visit(node: NodeRef, found: &mut Option<NodeRef>) {
        if !has_area(node) {
            return;
        }
        if slot_info(node).is_some_and(|(_, _, scope)| scope == FocusScope::Contain) {
            *found = Some(node);
        }
        for &child in crate::node::paint_order(children_of(node)).iter() {
            visit(child, found);
        }
    }
    let mut found = None;
    let roots = GLOBAL.with(|global| global.roots.borrow().clone());
    for root in roots {
        visit(root, &mut found);
    }
    found
}

/// Tabbable nodes within `scope` (or every displayed root), in Tab order.
pub(crate) fn tab_order(scope: Option<NodeRef>) -> Vec<NodeRef> {
    fn visit(node: NodeRef, out: &mut Vec<(i32, NodeRef)>) {
        if !has_area(node) {
            return;
        }
        if let Some((true, index, _)) = slot_info(node)
            && index >= 0
        {
            out.push((index, node));
        }
        for &child in children_of(node).iter() {
            visit(child, out);
        }
    }
    let mut found = Vec::new();
    match scope {
        Some(scope) => visit(scope, &mut found),
        None => {
            let roots = GLOBAL.with(|global| global.roots.borrow().clone());
            for root in roots {
                visit(root, &mut found);
            }
        }
    }
    // Stable: positive indices ascending, then index 0 in tree order.
    found.sort_by_key(|(index, _)| if *index > 0 { *index } else { i32::MAX });
    found.into_iter().map(|(_, node)| node).collect()
}

fn return_slot(node: NodeRef) -> Option<Rc<std::cell::Cell<Option<NodeRef>>>> {
    Runtime::try_with(|rt| {
        rt.nodes
            .borrow()
            .get(node.id)
            .and_then(|inner| Some(inner.focus.as_ref()?.return_to.clone()))
    })
    .ok()
    .flatten()
}

fn owners_of(node: NodeRef) -> Vec<Rc<FocusOwner>> {
    Runtime::try_with(|rt| {
        rt.nodes
            .borrow()
            .get(node.id)
            .and_then(|inner| Some(inner.focus.as_ref()?.owners.clone()))
    })
    .ok()
    .flatten()
    .unwrap_or_default()
}

fn invoke(
    node: NodeRef,
    hook: fn(&mut NodeHooks) -> &mut CallbackList<FocusEvent>,
    event: FocusEvent,
) {
    let _ = Runtime::try_with(|rt| rt.invoke_hook(node.id, hook, &event));
}

pub(crate) fn set_focus(target: Option<NodeRef>, reason: FocusReason) {
    let (previous, was_visible) = GLOBAL.with(|global| {
        (
            global.focused.get_untracked(),
            global.visible.get_untracked(),
        )
    });
    let previous = previous.filter(NodeRef::is_live);
    let visible = match reason {
        FocusReason::Keyboard => true,
        FocusReason::Pointer => false,
        FocusReason::Programmatic => was_visible,
    };
    if previous == target && visible == was_visible {
        return;
    }
    let target_scope = target.and_then(containing_scope);
    if let Some(scope) = target_scope
        && previous.is_some_and(|previous| containing_scope(previous) != Some(scope))
        && let Some(return_to) = return_slot(scope)
    {
        return_to.set(previous);
    }
    GLOBAL.with(|global| global.current_scope.set(target_scope));
    mue_core::batch(|| {
        GLOBAL.with(|global| {
            global.focused.set(target);
            global.visible.set(visible);
        });
        for owner in previous.map(owners_of).unwrap_or_default() {
            owner.focused.set(false);
            owner.visible.set(false);
        }
        for owner in target.map(owners_of).unwrap_or_default() {
            owner.focused.set(true);
            owner.visible.set(visible);
        }
    });
    let event = FocusEvent { reason, visible };
    if previous != target {
        if let Some(previous) = previous {
            mue_core::batch(|| invoke(previous, |hooks| &mut hooks.blur, event));
        }
        if let Some(target) = target {
            mue_core::batch(|| invoke(target, |hooks| &mut hooks.focus, event));
        }
    }
}

/// Record the roots in drawing order; Tab order and event paths use them.
/// Also runs the once-per-frame focus maintenance.
pub(crate) fn set_roots(roots: impl IntoIterator<Item = NodeRef>) {
    GLOBAL.with(|global| {
        let mut current = global.roots.borrow_mut();
        current.clear();
        current.extend(roots);
    });
    maintain();
}

/// Whether `node` is attached to a displayed root and it and its ancestors
/// have a non-empty layout area.
fn displayed(node: NodeRef) -> bool {
    path_to(node).is_some_and(|path| path.into_iter().all(has_area))
}

/// Once per frame: restore focus from removed scopes, move focus out of
/// subtrees that stopped being displayed, and run auto focus for nodes that
/// became displayed.
fn maintain() {
    mue_core::batch(|| {
        // Removed scopes: the first live target wins (an inner target may have
        // been removed together with its outer scope).
        let restores =
            GLOBAL.with(|global| std::mem::take(&mut *global.pending_restore.borrow_mut()));
        if focused_untracked().is_none()
            && let Some(target) = restores
                .into_iter()
                .find(|node| node.is_live() && displayed(*node))
        {
            set_focus(Some(target), FocusReason::Programmatic);
        }
        // Hidden but still mounted (display: none, a closed keep-mounted
        // presence): leave the subtree, returning to the scope's previous node.
        if let Some(current) = focused_untracked()
            && !displayed(current)
        {
            let scope = containing_scope(current)
                .or_else(|| GLOBAL.with(|global| global.current_scope.get()));
            let target = scope
                .and_then(return_slot)
                .and_then(|slot| slot.get())
                .filter(|node| node.is_live() && displayed(*node));
            set_focus(target, FocusReason::Programmatic);
        }
        // Auto focus each time a node goes from hidden to displayed.
        let entries = GLOBAL.with(|global| std::mem::take(&mut *global.auto_focus.borrow_mut()));
        let mut kept = Vec::with_capacity(entries.len());
        for (node, was_displayed) in entries {
            if !node.is_live() {
                continue;
            }
            let shown = displayed(node);
            if shown && !was_displayed {
                focus_within(node);
            }
            kept.push((node, shown));
        }
        GLOBAL.with(|global| {
            let mut list = global.auto_focus.borrow_mut();
            kept.append(&mut list);
            *list = kept;
        });
    });
}

/// A pointer press: focus the deepest focusable node on the path, else blur.
pub(crate) fn pointer_down(path: &IndexSet<NodeRef>) {
    let target = path.iter().copied().find(|node| is_focusable(*node));
    set_focus(target, FocusReason::Pointer);
}

/// The nodes a keyboard event visits, in order.
fn event_path() -> Vec<NodeRef> {
    if let Some(path) = focused_untracked().and_then(path_to) {
        return path;
    }
    GLOBAL.with(|global| global.roots.borrow().iter().rev().copied().collect())
}

fn bubble<E>(
    event: &E,
    handled: impl Fn(&E) -> bool,
    hook: fn(&mut NodeHooks) -> &mut CallbackList<E>,
) where
    E: 'static,
{
    for node in event_path() {
        if handled(event) {
            break;
        }
        let _ = Runtime::try_with(|rt| rt.invoke_hook(node.id, hook, event));
    }
}

pub(crate) fn dispatch_key_down(event: KeyEvent) {
    mue_core::batch(|| bubble(&event, KeyEvent::is_handled, |hooks| &mut hooks.key_down));
    if !event.is_handled() && event.key == KeyCode::Tab {
        mue_core::batch(|| {
            move_focus(!event.modifiers.shift);
        });
    }
    // Escape and the Android back button navigate back unless handled.
    if !event.is_handled() && !event.repeat && matches!(event.key, KeyCode::Escape | KeyCode::Back)
    {
        crate::back::dispatch_back();
    }
}

pub(crate) fn dispatch_key_up(event: KeyEvent) {
    mue_core::batch(|| bubble(&event, KeyEvent::is_handled, |hooks| &mut hooks.key_up));
}

pub(crate) fn dispatch_char(event: CharEvent) {
    mue_core::batch(|| bubble(&event, CharEvent::is_handled, |hooks| &mut hooks.char));
}

/// Called after a node with focus declarations is inserted into the arena.
pub(crate) fn mounted(id: NodeId, auto_focus: bool, contain: bool) {
    let node = NodeRef::from_id(id);
    if contain {
        let state = Runtime::with(|rt| {
            let nodes = rt.nodes.borrow();
            let inner = nodes.get(id)?;
            Some((inner.scope, inner.focus.as_ref()?.return_to.clone()))
        });
        if let Some((scope, return_to)) = state {
            // An untracked effect lives as long as the node scope; its cleanup
            // runs when the node is disposed.
            scope.run(|| {
                mue_core::effect::watch_effect(move || {
                    let return_to = return_to.clone();
                    mue_core::effect::on_cleanup(move || {
                        // Focus inside the removed subtree is gone; restore what
                        // was focused before focus entered the scope.
                        // Disposal can run inside a children update, so the
                        // restore waits for the next frame boundary.
                        if let Some(previous) = return_to.get() {
                            GLOBAL
                                .with(|global| global.pending_restore.borrow_mut().push(previous));
                        }
                    });
                });
            });
        }
    }
    if auto_focus {
        GLOBAL.with(|global| global.auto_focus.borrow_mut().push((node, false)));
    }
}
