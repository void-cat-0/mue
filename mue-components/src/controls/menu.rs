//! Dropdown menus: [`menu`], [`menu_button`], [`menu_list`], [`menu_item`],
//! [`menu_divider`], and [`menu_group`], modeled on Chakra UI's `Menu`.
//!
//! The root shares its open state with the parts. The [`menu_list`] is
//! teleported into [`Layer::Popover`] and anchored to the [`menu_button`];
//! it flips to the other side and shifts along it to stay inside the
//! window. It fades and scales in and out, and it closes when an item is
//! activated, on a press outside it, on Escape, and on back (the Android
//! back button, an edge swipe). While it is open the arrow keys, Home, and
//! End move the focus between items; Enter or Space activates one. Colors
//! follow the color mode; sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let _actions = menu()
//!     .children((
//!         menu_button("Actions"),
//!         menu_list().children((
//!             menu_item("Download").command("Ctrl+D").on_select(|| println!("download")),
//!             menu_item("Duplicate").on_select(|| println!("duplicate")),
//!             menu_divider(),
//!             menu_group("Danger zone").children(menu_item("Delete").disabled(true)),
//!         )),
//!     ))
//!     .into_node();
//! # let _open = menu()
//! #     .open(mue_core::prelude::signal(true))
//! #     .children((menu_button("Open"), menu_list().children(menu_item("Item"))))
//! #     .into_node();
//! # });
//! ```

use crate::motion::presence::{PresenceAnimation, presence};
use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::{
        back::use_back_handler,
        cursor::Cursor,
        focus::focused,
        keyboard::KeyCode,
        pointer::{PointerAction, use_pointer_down_observer},
    },
    layout::use_layout,
    math::{Vector, vec2},
    motion::transition::Transition,
    node::{BuildNode, IntoNode, Node, NodeContext, NodeHandle, NodeRef, flexbox, text},
    overlay::{
        anchor::{Placement, anchored},
        layer::{Layer, teleport},
    },
    paint::{
        color::Color,
        path::{PathData, StrokeStyle},
        shader::Brush,
        use_paint,
    },
    style::{BoxShadows, Size, Style, Styleable, StyleableExt, root_font_size},
};

use crate::{
    controls::button::{ButtonBuilder, ButtonVariant, button},
    state::use_part,
    system::{self, ControlSize, Palette, white_alpha},
};

/// The menu's items in build order, with their disabled state.
type Items = Rc<RefCell<Vec<(NodeHandle, Prop<bool>)>>>;

#[derive(Clone)]
struct MenuContext {
    open: Signal<bool>,
    trigger: NodeHandle,
    list: NodeHandle,
    placement: Prop<Placement>,
    close_on_select: bool,
    items: Items,
}

impl MenuContext {
    fn get(part: &str) -> Self {
        use_part(part, "menu")
    }

    /// Close the menu; focus goes back to the button if it was in the list.
    fn close(&self) {
        if !self.open.get_untracked() {
            return;
        }
        let inside = match (focused(), self.list.get_untracked()) {
            (Some(focus), Some(list)) => list.contains(focus),
            _ => false,
        };
        self.open.set(false);
        if inside {
            self.trigger.focus();
        }
    }

    /// Focus the enabled item chosen by `pick(current, count)` among the
    /// candidates it returns, in order.
    fn focus_by(&self, pick: impl Fn(Option<usize>, usize) -> Vec<usize>) {
        let items = self.items.borrow();
        let count = items.len();
        if count == 0 {
            return;
        }
        let current = focused().and_then(|focus| {
            items
                .iter()
                .position(|(handle, _)| handle.get_untracked().is_some_and(|item| item.contains(focus)))
        });
        let target = pick(current, count)
            .into_iter()
            .find(|&index| !items[index].1.get_untracked())
            .map(|index| items[index].0);
        drop(items);
        if let Some(handle) = target {
            handle.focus();
        }
    }

    fn focus_first(&self) {
        self.focus_by(|_, n| (0..n).collect());
    }

    fn focus_last(&self) {
        self.focus_by(|_, n| (0..n).rev().collect());
    }
}

/// Builder returned by [`menu`].
pub struct MenuBuilder {
    open: Option<Signal<bool>>,
    placement: Prop<Placement>,
    close_on_select: bool,
    style: Style,
}

/// The root of a dropdown menu: pass a [`menu_button`] and a [`menu_list`]
/// with `.children(..)`.
///
/// The menu keeps its own open state unless [`open`](MenuBuilder::open)
/// binds a signal.
pub fn menu() -> MenuBuilder {
    MenuBuilder {
        open: None,
        placement: Prop::Static(Placement::BottomStart),
        close_on_select: true,
        style: Style::new(),
    }
}

impl MenuBuilder {
    /// Two-way bind the open state (controlled).
    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    /// The preferred side of the list (below, aligned to the start, by default).
    pub fn placement(mut self, placement: impl Into<Prop<Placement>>) -> Self {
        self.placement = placement.into();
        self
    }

    /// Close the menu when an item is activated (default on).
    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }
}

impl Styleable for MenuBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for MenuBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            open,
            placement,
            close_on_select,
            style,
        } = self;
        let open = open.unwrap_or_else(|| signal(false));
        let context = MenuContext {
            open,
            trigger: NodeHandle::new(),
            list: NodeHandle::new(),
            placement,
            close_on_select,
            items: Rc::default(),
        };
        // Back (and Escape, when nothing else takes it) closes the menu.
        {
            let context = context.clone();
            use_back_handler(open, move || context.close());
        }
        // A press outside the list and the button closes the menu.
        {
            let context = context.clone();
            use_pointer_down_observer(move |position, path| {
                if !context.open.get_untracked() {
                    return;
                }
                let inside = [context.list, context.trigger]
                    .iter()
                    .filter_map(|handle| handle.get_untracked())
                    .any(|node| press_is_inside(node, position, path.iter()));
                if !inside {
                    context.close();
                }
            });
        }
        crate::state::provide_part(context);
        flexbox().flex_row().flex_shrink(0.).style(style).build(ctx)
    }
}

fn press_is_inside<'a>(
    node: NodeRef,
    position: Vector,
    path: impl Iterator<Item = &'a NodeRef> + Clone,
) -> bool {
    if path.clone().any(|hit| node.contains(*hit)) {
        return true;
    }
    let within = node
        .rect()
        .and_then(|rect| rect.try_get_untracked())
        .is_some_and(|rect| rect.contains(&position));
    within && path.clone().all(|hit| hit.contains(node))
}

/// A painted downward chevron in the inherited text color.
struct Chevron {
    color: Prop<Brush>,
    style: Style,
}

impl Styleable for Chevron {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for Chevron {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { color, style } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let rect = use_layout(style).rect;
            let paint = use_paint(style);
            style.on_render.append(move |_| {
                let rect = rect.get_untracked();
                let d = rect.w.min(rect.h);
                if d <= 0. {
                    return;
                }
                let c = rect.center();
                let path = PathData::polyline([
                    c + vec2(-d * 0.3, -d * 0.12),
                    c + vec2(0., d * 0.18),
                    c + vec2(d * 0.3, -d * 0.12),
                ]);
                let brush = color.get_clone_untracked();
                paint.draw(|p| p.stroke(&path, &StrokeStyle::round((d * 0.12).max(1.)), brush.shader(rect)));
            });
        })
    }
}

/// Builder returned by [`menu_button`].
pub struct MenuButtonBuilder {
    button: ButtonBuilder,
    chevron: bool,
}

/// The button that opens and closes its [`menu`]: an outline [`button`]
/// with a chevron after the label.
///
/// Enter, Space, or Down opens the menu and focuses the first item; Up
/// focuses the last one.
///
/// # Panics
///
/// When built outside [`menu`].
pub fn menu_button(label: impl Into<Prop<Rc<str>>>) -> MenuButtonBuilder {
    MenuButtonBuilder {
        button: button(label).variant(ButtonVariant::Outline),
        chevron: true,
    }
}

impl MenuButtonBuilder {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.button = self.button.variant(variant);
        self
    }

    pub fn palette(mut self, palette: Palette) -> Self {
        self.button = self.button.palette(palette);
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.button = self.button.size(size);
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.button = self.button.disabled(disabled);
        self
    }

    /// Draw the chevron after the label (default on).
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }
}

impl Styleable for MenuButtonBuilder {
    fn style_mut(&mut self) -> &mut Style {
        self.button.style_mut()
    }
}

impl BuildNode for MenuButtonBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self { mut button, chevron } = self;
        let menu = MenuContext::get("menu_button");
        let open = menu.open;
        if chevron {
            button = button.right_icon(
                Chevron {
                    color: system::fg(),
                    style: Style::new(),
                }
                .width(Size::Rem(1.))
                .height(Size::Rem(1.))
                .flex_shrink(0.),
            );
        }
        let keys = menu.clone();
        button
            .node_handle(menu.trigger)
            .on_activate(move || {
                if open.get_untracked() {
                    menu.close();
                } else {
                    open.set(true);
                }
            })
            .on_key_down(move |event| {
                let last = match event.key {
                    KeyCode::Down => false,
                    KeyCode::Up => true,
                    _ => return,
                };
                event.stop_propagation();
                keys.open.set(true);
                // The list is kept mounted; focus once it is displayed.
                let keys = keys.clone();
                mue_macroquad::app::runtime::set_timeout(0., move || {
                    if last { keys.focus_last() } else { keys.focus_first() }
                });
            })
            .build(ctx)
    }
}

/// Builder returned by [`menu_list`].
pub struct MenuListBuilder {
    style: Style,
}

/// The floating list of a [`menu`]; pass [`menu_item`]s,
/// [`menu_divider`]s, and [`menu_group`]s with `.children(..)`.
///
/// Builder styles apply to the list's panel.
///
/// # Panics
///
/// When built outside [`menu`].
pub fn menu_list() -> MenuListBuilder {
    MenuListBuilder { style: Style::new() }
}

impl Styleable for MenuListBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for MenuListBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let menu = MenuContext::get("menu_list");
        let open = menu.open;
        let offset = root_font_size().get_untracked() * 0.25;
        let mut panel = Some(self.style);
        teleport(Layer::Popover, move || {
            let mut panel = panel.take();
            let menu = menu.clone();
            presence(open, move || {
                let style = panel.take().unwrap_or_default();
                let keys = menu.clone();
                anchored(menu.trigger)
                    .placement(menu.placement)
                    .offset(offset)
                    .children(
                        flexbox()
                            .node_handle(menu.list)
                            .flex_column()
                            .min_width(Size::Rem(14.))
                            .padding_y(Size::Rem(0.5))
                            .radius(system::radius::MD)
                            .border((Size::Px(1.),))
                            .border_color(system::border())
                            .background(system::bg_panel())
                            .box_shadow(BoxShadows::elevation(8.))
                            .font_size(system::font_size::MD)
                            .line_height(Size::Rem(1.5))
                            .color(system::fg())
                            // Focus the panel itself on open; the arrow keys
                            // move to the items.
                            .focusable(true)
                            .tab_index(-1)
                            .auto_focus(true)
                            // Swallow taps between items so they do not reach the page.
                            .on_tap(|_| {})
                            .on_key_down(move |event| {
                                match event.key {
                                    KeyCode::Down => keys.focus_by(|current, n| match current {
                                        Some(i) => (1..=n).map(|k| (i + k) % n).collect(),
                                        None => (0..n).collect(),
                                    }),
                                    KeyCode::Up => keys.focus_by(|current, n| match current {
                                        Some(i) => (1..=n).map(|k| (i + n - k) % n).collect(),
                                        None => (0..n).rev().collect(),
                                    }),
                                    KeyCode::Home => keys.focus_first(),
                                    KeyCode::End => keys.focus_last(),
                                    KeyCode::Escape | KeyCode::Tab => keys.close(),
                                    _ => return,
                                }
                                event.stop_propagation();
                            })
                            .style(style),
                    )
            })
            .keep_mounted(true)
            .animation(PresenceAnimation::fade().scale(0.95))
            .transition(Transition::ease_out(0.15))
        })
        .build(ctx)
    }
}

/// Builder returned by [`menu_item`].
pub struct MenuItemBuilder {
    label: Prop<Rc<str>>,
    icon: Option<Node>,
    command: Option<Prop<Rc<str>>>,
    disabled: Prop<bool>,
    on_select: Option<Box<dyn FnMut()>>,
    style: Style,
}

/// One action in a [`menu_list`]. Activating it (click, tap, Enter, or
/// Space) runs [`on_select`](MenuItemBuilder::on_select) and closes the menu.
///
/// It highlights while hovered or focused and is at least 44 logical pixels
/// tall for touch.
///
/// # Panics
///
/// When built outside [`menu`].
pub fn menu_item(label: impl Into<Prop<Rc<str>>>) -> MenuItemBuilder {
    MenuItemBuilder {
        label: label.into(),
        icon: None,
        command: None,
        disabled: Prop::Static(false),
        on_select: None,
        style: Style::new(),
    }
}

impl MenuItemBuilder {
    /// An icon before the label.
    pub fn icon(mut self, icon: impl IntoNode) -> Self {
        self.icon = Some(icon.into_node());
        self
    }

    /// A keyboard shortcut hint at the end (Chakra's `command`).
    pub fn command(mut self, command: impl Into<Prop<Rc<str>>>) -> Self {
        self.command = Some(command.into());
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    pub fn on_select(mut self, action: impl FnMut() + 'static) -> Self {
        self.on_select = Some(Box::new(action));
        self
    }
}

impl Styleable for MenuItemBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for MenuItemBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            label,
            icon,
            command,
            disabled,
            mut on_select,
            style,
        } = self;
        let menu = MenuContext::get("menu_item");
        let handle = NodeHandle::new();
        menu.items.borrow_mut().push((handle, disabled));
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let activate = Rc::new(RefCell::new({
            let menu = menu.clone();
            move || {
                if disabled.get_untracked() {
                    return;
                }
                if let Some(action) = on_select.as_mut() {
                    action();
                }
                if menu.close_on_select {
                    menu.close();
                }
            }
        }));
        let mut root = flexbox()
            .node_handle(handle)
            .flex_row()
            .items_center()
            .gap(Size::Rem(0.75))
            .width(Size::Percent(100.))
            .padding_x(Size::Rem(0.75))
            .padding_y(Size::Rem(0.375))
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .focusable(disabled.map(|disabled| !disabled))
            .min_hit_size(44.)
            .cursor(disabled.map(|disabled| {
                if disabled { Cursor::NotAllowed } else { Cursor::Pointer }
            }))
            .use_pressed(pressed)
            .on_hover_event(move |event| match event.action() {
                PointerAction::Down => {
                    hovered.set(true);
                    // Like Chakra, hovering moves the focus to the item.
                    if !disabled.get_untracked() && focused().is_some_and(|f| handle.get_untracked() != Some(f)) {
                        handle.focus();
                    }
                }
                PointerAction::Up => hovered.set(false),
                _ => {}
            })
            .on_activate({
                let activate = activate.clone();
                move || (*activate.borrow_mut())()
            })
            .on_key_down(move |event| {
                if !event.repeat
                    && matches!(event.key, KeyCode::Enter | KeyCode::KpEnter | KeyCode::Space)
                {
                    event.stop_propagation();
                    (*activate.borrow_mut())();
                }
            })
            .transition(Transition::ease_out(0.1));
        root = root.with_focus(|root, focus| {
            let background = computed(move |_| {
                let lit = (hovered.get() || focus.focused.get()) && !disabled.get();
                let color = match (lit, pressed.get(), dark.get()) {
                    (false, _, _) => Color::TRANSPARENT,
                    (true, false, false) => Palette::Gray.shade(100),
                    (true, true, false) => Palette::Gray.shade(200),
                    (true, false, true) => white_alpha(0.08),
                    (true, true, true) => white_alpha(0.16),
                };
                Brush::Solid(color)
            });
            root.background(background)
        });
        let mut children: Vec<Node> = Vec::new();
        children.extend(icon);
        children.push(
            text(label)
                .flex_grow(1.)
                .text_wrap(false)
                .color(system::fg())
                .into_node(),
        );
        if let Some(command) = command {
            children.push(
                text(command)
                    .font_size(system::font_size::SM)
                    .text_wrap(false)
                    .color(system::fg_muted())
                    .into_node(),
            );
        }
        root.children(children).style(style).build(ctx)
    }
}

/// Builder returned by [`menu_divider`].
pub struct MenuDividerBuilder {
    style: Style,
}

/// A thin line between groups of [`menu_item`]s.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _line = menu_divider();
/// ```
pub fn menu_divider() -> MenuDividerBuilder {
    MenuDividerBuilder { style: Style::new() }
}

impl Styleable for MenuDividerBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for MenuDividerBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        flexbox()
            .width(Size::Percent(100.))
            .height(Size::Px(1.))
            .margin_y(Size::Rem(0.5))
            .background(system::border())
            .style(self.style)
            .build(ctx)
    }
}

/// Builder returned by [`menu_group`].
pub struct MenuGroupBuilder {
    title: Prop<Rc<str>>,
    style: Style,
}

/// A titled group of [`menu_item`]s; pass the items with `.children(..)`.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _profile = menu_group("Profile").children((menu_item("My account"), menu_item("Payments")));
/// ```
pub fn menu_group(title: impl Into<Prop<Rc<str>>>) -> MenuGroupBuilder {
    MenuGroupBuilder {
        title: title.into(),
        style: Style::new(),
    }
}

impl Styleable for MenuGroupBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for MenuGroupBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self { title, style } = self;
        // The caller's items go to an inner column after the title.
        let items = flexbox().flex_column().width(Size::Percent(100.)).style(style);
        flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .children((
                text(title)
                    .margin_x(Size::Rem(0.75))
                    .margin_y(Size::Rem(0.5))
                    .font_size(system::font_size::SM)
                    .line_height(Size::Rem(1.25))
                    .font_weight(cosmic_text::Weight::SEMIBOLD)
                    .color(system::fg()),
                items,
            ))
            .build(ctx)
    }
}
