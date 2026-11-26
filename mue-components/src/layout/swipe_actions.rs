use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use mue_core::prelude::*;
use mue_macroquad::{
    event::pointer::{
        ButtonState, ClaimToken, PointerAction, PointerEvent, PointerId, PointerType,
    },
    gesture::Gesture,
    math::Vector,
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, NodeHandle, flexbox,
        flexbox::use_container,
    },
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

type Action = Rc<RefCell<dyn FnMut()>>;

thread_local! {
    /// The item currently swiped open; opening another closes it.
    static OPEN: RefCell<Weak<Cell<f32>>> = const { RefCell::new(Weak::new()) };
}

/// A list row that reveals actions when swiped sideways (MB7), like iOS and
/// Android mail lists. Swipe left to show the trailing actions, right for the
/// leading ones; release past half their width to keep them open, or swipe
/// past 60% of the row to run the full-swipe action. Tapping the row or
/// opening another row closes it.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _row = swipe_actions(text("Message from Ada"))
///     .trailing(button("Delete"))
///     .on_full_swipe_trailing(|| println!("deleted"));
/// ```
pub fn swipe_actions(content: impl IntoNode) -> SwipeActionsBuilder {
    SwipeActionsBuilder {
        content: content.into_node(),
        leading: None,
        trailing: None,
        full_leading: None,
        full_trailing: None,
        mouse: false,
        style: Style::new(),
    }
}

pub struct SwipeActionsBuilder {
    content: Node,
    leading: Option<Node>,
    trailing: Option<Node>,
    full_leading: Option<Action>,
    full_trailing: Option<Action>,
    mouse: bool,
    style: Style,
}

impl SwipeActionsBuilder {
    /// Actions revealed by swiping right (on the left edge).
    pub fn leading(mut self, actions: impl IntoNode) -> Self {
        self.leading = Some(actions.into_node());
        self
    }

    /// Actions revealed by swiping left (on the right edge).
    pub fn trailing(mut self, actions: impl IntoNode) -> Self {
        self.trailing = Some(actions.into_node());
        self
    }

    pub fn on_full_swipe_leading(mut self, action: impl FnMut() + 'static) -> Self {
        self.full_leading = Some(Rc::new(RefCell::new(action)));
        self
    }

    pub fn on_full_swipe_trailing(mut self, action: impl FnMut() + 'static) -> Self {
        self.full_trailing = Some(Rc::new(RefCell::new(action)));
        self
    }

    /// Also accept mouse drags (useful on desktop).
    pub fn mouse(mut self, mouse: bool) -> Self {
        self.mouse = mouse;
        self
    }
}

impl Styleable for SwipeActionsBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for SwipeActionsBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            content,
            leading,
            trailing,
            full_leading,
            full_trailing,
            mouse,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(
            Style::new().relative().overflow_x(taffy::Overflow::Hidden),
            move |style| {
                let offset = signal(0_f32);
                // Where the row settles when not dragged; shared with OPEN.
                let target = Rc::new(Cell::new(0_f32));
                let leading_handle = NodeHandle::new();
                let trailing_handle = NodeHandle::new();
                let side = |actions: Node, handle: NodeHandle, left: bool| {
                    let mut style = Style::new()
                        .absolute()
                        .inset_top(Size::Px(0.))
                        .inset_bottom(Size::Px(0.))
                        .flex_row()
                        .items_stretch();
                    style = if left {
                        style.inset_left(Size::Px(0.))
                    } else {
                        style.inset_right(Size::Px(0.))
                    };
                    flexbox()
                        .style(style)
                        .node_handle(handle)
                        .children(actions.into_children())
                        .into_node()
                };
                let mut children: Vec<Node> = Vec::new();
                children.extend(leading.map(|node| side(node, leading_handle, true)));
                children.extend(trailing.map(|node| side(node, trailing_handle, false)));
                let row = flexbox()
                    .style(Style::new().w_full().flex_column())
                    .transform(computed(move |_| {
                        Transform(vec![TransformOp::Translate(
                            Size::Px(offset.get()),
                            Size::Px(0.),
                        )])
                    }))
                    .on_tap({
                        let target = target.clone();
                        move |_| {
                            if target.get() != 0. {
                                target.set(0.);
                            }
                        }
                    })
                    .children(content.into_children())
                    .into_node();
                children.push(row);
                mue_macroquad::node::set_children(children.into_children());
                use_container(style);
                // The layout rect exists once the container is set up.
                let rect = mue_macroquad::node::use_rect();
                let width = move |handle: NodeHandle| {
                    handle
                        .rect()
                        .map_or(0., |rect: mue_macroquad::math::Rect| rect.w)
                };
                let gesture = SwipeGesture {
                    offset,
                    target: target.clone(),
                    drag: None,
                    mouse,
                    leading: Box::new(move || width(leading_handle)),
                    trailing: Box::new(move || width(trailing_handle)),
                    row_width: Box::new(move || rect.get_untracked().w),
                    full_leading,
                    full_trailing,
                };
                mue_macroquad::node::add_gesture(gesture);
                // Spring toward the target when not dragging.
                style.on_update.append({
                    let target = target.clone();
                    move |dt: &f32| {
                        let current = offset.get_untracked();
                        let goal = target.get();
                        if (current - goal).abs() < 0.5 {
                            if current != goal {
                                offset.set(goal);
                            }
                            return;
                        }
                        let rate = 1. - (-16. * dt.clamp(0., 0.05)).exp();
                        offset.set(current + (goal - current) * rate);
                    }
                });
            },
        )
    }
}

struct Drag {
    pointer: PointerId,
    origin: Vector,
    start_offset: f32,
    claimed: bool,
}

struct SwipeGesture {
    offset: Signal<f32>,
    target: Rc<Cell<f32>>,
    drag: Option<Drag>,
    mouse: bool,
    leading: Box<dyn Fn() -> f32>,
    trailing: Box<dyn Fn() -> f32>,
    row_width: Box<dyn Fn() -> f32>,
    full_leading: Option<Action>,
    full_trailing: Option<Action>,
}

impl SwipeGesture {
    fn settle(&mut self, offset: f32) -> Option<Action> {
        let (leading, trailing, row) = ((self.leading)(), (self.trailing)(), (self.row_width)());
        let full = row * 0.6;
        let (goal, action) = if offset < 0. && trailing > 0. {
            if self.full_trailing.is_some() && -offset >= full {
                (0., self.full_trailing.clone())
            } else if -offset >= trailing / 2. {
                (-trailing, None)
            } else {
                (0., None)
            }
        } else if offset > 0. && leading > 0. {
            if self.full_leading.is_some() && offset >= full {
                (0., self.full_leading.clone())
            } else if offset >= leading / 2. {
                (leading, None)
            } else {
                (0., None)
            }
        } else {
            (0., None)
        };
        self.target.set(goal);
        if goal != 0. {
            // Close whichever row was open before.
            OPEN.with(|open| {
                let previous = open.borrow().upgrade();
                if let Some(previous) = previous
                    && !Rc::ptr_eq(&previous, &self.target)
                {
                    previous.set(0.);
                }
                *open.borrow_mut() = Rc::downgrade(&self.target);
            });
        }
        action
    }
}

/// Movement before the swipe claims the pointer.
const SLOP: f32 = 10.;

impl Gesture for SwipeGesture {
    fn on_event(&mut self, event: &PointerEvent, token: &ClaimToken) {
        match event.action() {
            PointerAction::Down => {
                let allowed = match event.pointer_type() {
                    PointerType::Touch => true,
                    PointerType::Mouse => {
                        self.mouse && event.button_state().contains(ButtonState::PRIMARY)
                    }
                };
                if allowed && self.drag.is_none() {
                    self.drag = Some(Drag {
                        pointer: event.pointer_id(),
                        origin: event.position(),
                        start_offset: self.offset.get_untracked(),
                        claimed: false,
                    });
                } else {
                    token.dismiss();
                }
            }
            PointerAction::Move => {
                let Some(drag) = self.drag.as_mut() else {
                    return;
                };
                if drag.pointer != event.pointer_id() {
                    return;
                }
                let delta = event.position() - drag.origin;
                if !drag.claimed {
                    if delta.y.abs() > SLOP && delta.y.abs() >= delta.x.abs() {
                        // Vertical: let the list scroll.
                        self.drag = None;
                        token.dismiss();
                        return;
                    }
                    if delta.x.abs() <= SLOP || delta.x.abs() < delta.y.abs() * 1.2 {
                        return;
                    }
                    if !token.claim() {
                        self.drag = None;
                        token.dismiss();
                        return;
                    }
                    drag.claimed = true;
                }
                let (leading, trailing, row) =
                    ((self.leading)(), (self.trailing)(), (self.row_width)());
                // Full swipes may travel the whole row; otherwise resist past the actions.
                let min = if self.full_trailing.is_some() {
                    -row
                } else {
                    -trailing - 24.
                };
                let max = if self.full_leading.is_some() {
                    row
                } else {
                    leading + 24.
                };
                let offset = (drag.start_offset + delta.x).clamp(min.min(0.), max.max(0.));
                self.target.set(offset);
                self.offset.set(offset);
            }
            PointerAction::Up | PointerAction::Cancel => {
                let Some(drag) = self.drag.take() else {
                    return;
                };
                if drag.claimed {
                    let action = self.settle(self.offset.get_untracked());
                    token.dismiss();
                    if let Some(action) = action {
                        (action.borrow_mut())();
                    }
                }
            }
        }
    }

    fn on_rejected(&mut self, pointer: PointerId) {
        if self
            .drag
            .as_ref()
            .is_some_and(|drag| drag.pointer == pointer)
        {
            self.drag = None;
        }
    }
}
