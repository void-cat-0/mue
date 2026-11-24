//! Application-level text dialogs. The App owns routing and drawing order.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::{Rc, Weak},
};

use mue_core::{
    prop::Prop,
    scope::OwnedScope,
    signal::{Access, Signal, signal},
};
use mue_macroquad::{
    ModalRoot,
    color::Color,
    node::{IntoNode, Node, flexbox, keyed, text},
    style::{Styleable, StyleableExt, percent, vh},
};

use crate::{ScrollOptions, button, scroll_area, stack, wrap};

/// A text dialog with a scrollable message and a row of action buttons.
/// Defaults to one OK button and allows dismissal by Escape or the backdrop.
pub struct Dialog {
    title: Prop<Rc<str>>,
    message: Prop<Rc<str>>,
    buttons: Vec<Prop<Rc<str>>>,
    dismissible: bool,
    background: Color,
    foreground: Color,
}

impl Dialog {
    pub fn new(title: impl Into<Prop<Rc<str>>>, message: impl Into<Prop<Rc<str>>>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            buttons: vec!["OK".into()],
            dismissible: true,
            background: Color::from_rgba(25, 31, 45, 255),
            foreground: Color::WHITE,
        }
    }

    /// Replace the button labels. The last button is styled as the primary action.
    pub fn buttons<T: Into<Prop<Rc<str>>>>(mut self, labels: impl IntoIterator<Item = T>) -> Self {
        self.buttons = labels.into_iter().map(Into::into).collect();
        self
    }

    /// Enable or disable Escape and backdrop dismissal. Leave at least one
    /// action button if both dismissal paths are disabled.
    pub fn dismissible(mut self, enabled: bool) -> Self {
        self.dismissible = enabled;
        self
    }

    /// Set the panel and text colors; button styles remain independent.
    pub fn colors(mut self, background: Color, foreground: Color) -> Self {
        self.background = background;
        self.foreground = foreground;
        self
    }
}

/// A button's zero-based index, or dismissal via Escape or the backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Button(usize),
    Dismissed,
}

type Reply = Box<dyn FnOnce(DialogResult)>;

struct Request {
    dialog: Dialog,
    reply: Cell<Option<Reply>>,
}

#[derive(Clone)]
struct Active(Rc<Request>);

impl PartialEq for Active {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

type Inbox = RefCell<VecDeque<Request>>;

/// UI-thread sender for one App modal host. Does not retain the host.
#[derive(Clone)]
pub struct DialogSender(Weak<Inbox>);

impl DialogSender {
    /// Queue a dialog. Requests are shown one at a time in order. `on_result`
    /// runs once when the user closes it; it does not run if the host is removed.
    /// Use owned data or App-lived state, not non-owning Scene-local bindings.
    pub fn show(&self, dialog: Dialog, on_result: impl FnOnce(DialogResult) + 'static) {
        if let Some(inbox) = self.0.upgrade() {
            inbox.borrow_mut().push_back(Request {
                dialog,
                reply: Cell::new(Some(Box::new(on_result))),
            });
        }
    }
}

/// Create a modal root and a sender. Install it once with [`mue_macroquad::App::set_modal_root`].
///
/// ```no_run
/// use mue_components::{Dialog, DialogResult, dialog_host};
/// use mue_macroquad::App;
/// fn install(app: &mut App) {
///     let (host, dialogs) = dialog_host();
///     app.set_modal_root(host);
///     dialogs.show(Dialog::new("Leave room?", "You can join again later.").buttons(["Stay", "Leave"]),
///         |result| {
///             if result == DialogResult::Button(1) { /* Ask the application to leave. */ }
///         });
/// }
/// ```
pub fn dialog_host() -> (ModalRoot, DialogSender) {
    let owner = OwnedScope::new();
    let inbox = Rc::new(Inbox::default());
    let sender = DialogSender(Rc::downgrade(&inbox));
    let (node, current, open) = owner.run(|| {
        let current = signal(None::<Active>);
        let open = current.map(|dialog| dialog.is_some());
        let node = flexbox()
            .size_full()
            .on_update(move |_| activate_next(&inbox, current))
            .children(keyed(
                current,
                move |active: &Option<Active>| match active {
                    Some(Active(request)) => dialog_view(&request.dialog, current),
                    None => Node::empty(),
                },
            ))
            .into_node();
        (node, current, open)
    });
    node.retain_owned_scope(owner);
    let root =
        ModalRoot::new(node, open).on_escape(move || resolve(current, DialogResult::Dismissed));
    (root, sender)
}

fn activate_next(inbox: &Inbox, current: Signal<Option<Active>>) {
    if current.get_clone_untracked().is_none() {
        let next = inbox.borrow_mut().pop_front();
        if let Some(request) = next {
            current.set(Some(Active(Rc::new(request))));
        }
    }
}

fn resolve(current: Signal<Option<Active>>, result: DialogResult) {
    let Some(Active(request)) = current.get_clone_untracked() else {
        return;
    };
    if result == DialogResult::Dismissed && !request.dialog.dismissible {
        return;
    }
    current.set(None);
    if let Some(reply) = request.reply.take() {
        reply(result);
    }
}

fn dialog_view(dialog: &Dialog, current: Signal<Option<Active>>) -> Node {
    let buttons = dialog
        .buttons
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, label)| {
            button(label)
                .primary(i + 1 == dialog.buttons.len())
                .font_size(16.)
                .line_height(22.)
                .max_width(percent(100.))
                .on_activate(move || resolve(current, DialogResult::Button(i)))
        })
        .collect::<Vec<_>>();
    let message = scroll_area()
        .max_height(vh(52.))
        .options(ScrollOptions {
            mouse_drag: true,
            ..Default::default()
        })
        .children(
            text(dialog.message.clone())
                .font_size(16.)
                .line_height(23.)
                .color(dialog.foreground),
        );
    let panel = stack()
        .width(500.)
        .max_width(percent(100.))
        .max_height(percent(100.))
        .padding((24.,))
        .gap(16.)
        .radius(16.)
        .background(dialog.background)
        // A tap on the body must not become a backdrop tap. Actions are deeper.
        .on_tap(|_| {})
        .children((
            text(dialog.title.clone())
                .font_size(23.)
                .line_height(30.)
                .flex_shrink(0.)
                .color(dialog.foreground),
            message,
            wrap()
                .justify_end()
                .gap(8.)
                .flex_shrink(0.)
                .children(buttons),
        ));
    flexbox()
        .size_full()
        .padding((20.,))
        .items_center()
        .justify_center()
        .background(Color::from_rgba(0, 0, 0, 153))
        .on_activate(move || resolve(current, DialogResult::Dismissed))
        .children(panel)
        .into_node()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogs_wait_their_turn_and_resolve_once() {
        let inbox = Rc::new(Inbox::default());
        let sender = DialogSender(Rc::downgrade(&inbox));
        let results = Rc::new(RefCell::new(Vec::new()));
        for title in ["First", "Second"] {
            let results = results.clone();
            sender.show(Dialog::new(title, "Message"), move |result| {
                results.borrow_mut().push(result);
            });
        }
        let owner = OwnedScope::new();
        let current = owner.run(|| signal(None::<Active>));
        activate_next(&inbox, current);
        activate_next(&inbox, current);
        assert_eq!(inbox.borrow().len(), 1);
        resolve(current, DialogResult::Button(0));
        resolve(current, DialogResult::Dismissed);
        assert!(current.get_clone().is_none());
        assert_eq!(*results.borrow(), [DialogResult::Button(0)]);
        activate_next(&inbox, current);
        resolve(current, DialogResult::Dismissed);
        assert_eq!(
            *results.borrow(),
            [DialogResult::Button(0), DialogResult::Dismissed]
        );
    }

    #[test]
    fn required_choice_ignores_dismissal() {
        let inbox = Rc::new(Inbox::default());
        let sender = DialogSender(Rc::downgrade(&inbox));
        let result = Rc::new(Cell::new(None));
        sender.show(
            Dialog::new("Required", "Choose an action").dismissible(false),
            {
                let result = result.clone();
                move |choice| result.set(Some(choice))
            },
        );
        let owner = OwnedScope::new();
        let current = owner.run(|| signal(None::<Active>));
        activate_next(&inbox, current);
        resolve(current, DialogResult::Dismissed);
        assert!(current.get_clone().is_some());
        assert_eq!(result.get(), None);
        resolve(current, DialogResult::Button(0));
        assert_eq!(result.get(), Some(DialogResult::Button(0)));
    }

    #[test]
    fn dropping_the_host_discards_pending_requests_without_replying() {
        let (host, sender) = dialog_host();
        let called = Rc::new(Cell::new(false));
        sender.show(Dialog::new("Pending", "Message"), {
            let called = called.clone();
            move |_| called.set(true)
        });
        drop(host);
        assert!(!called.get());
        assert!(sender.0.upgrade().is_none());
    }
}
