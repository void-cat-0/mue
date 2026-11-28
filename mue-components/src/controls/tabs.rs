//! Tabs: [`tabs`], [`tab_list`], [`tab`], [`tab_panels`], and [`tab_panel`],
//! modeled on Chakra UI's `Tabs`.
//!
//! The root holds the selected index and shares it with its parts. Tabs and
//! panels are numbered in the order they are built, so the third [`tab`]
//! shows the third [`tab_panel`]. The list draws one indicator (an
//! underline, a raised card, or a pill, by [`TabsVariant`]) that slides to
//! the selected tab. Only the selected tab is reached by Tab; the arrow
//! keys, Home, and End select and focus another tab. Colors follow the
//! [`Palette`] and the color mode; sizes are in `rem`.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::Palette;
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let selected = signal(0_usize);
//! let _settings = tabs()
//!     .index(selected)
//!     .variant(TabsVariant::SoftRounded)
//!     .palette(Palette::Green)
//!     .children((
//!         tab_list().children((tab("Account"), tab("Password"), tab("Billing").disabled(true))),
//!         tab_panels().children((
//!             tab_panel().children(text("Your profile")),
//!             tab_panel().children(text("Change your password")),
//!             tab_panel().children(text("Invoices")),
//!         )),
//!     ))
//!     .into_node();
//! # });
//! ```

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    motion::transition::Transition,
    node::{BuildNode, Callback, Node, NodeContext, NodeHandle, flexbox, text},
    paint::{color::Color, shader::Brush},
    style::{Size, Style, Styleable, StyleableExt, values::Display},
};

use crate::{
    state::{Controllable, provide_part, use_part},
    system::{self, ControlSize, Palette, white_alpha},
};

/// Visual style of [`tabs`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsVariant {
    /// An underline below the list; the selected tab's underline is colored.
    #[default]
    Line,
    /// The selected tab is a card joined to the line below the list.
    Enclosed,
    /// The selected tab sits on a tinted pill.
    SoftRounded,
}

struct TabEntry {
    handle: NodeHandle,
    disabled: Prop<bool>,
}

#[derive(Clone)]
struct TabsContext {
    state: Controllable<usize>,
    variant: TabsVariant,
    palette: Palette,
    size: ControlSize,
    fitted: bool,
    tabs: Rc<RefCell<Vec<TabEntry>>>,
    /// Bumped when a tab joins, so the indicator and tab stops recompute.
    version: Signal<u64>,
    panels: Rc<Cell<usize>>,
}

impl TabsContext {
    fn context() -> Self {
        use_part("tab", "tabs")
    }

    /// Select and focus the enabled tab found by `pick` from the current one.
    fn select_by(&self, pick: impl Fn(usize, usize) -> Vec<usize>) {
        let target = {
            let tabs = self.tabs.borrow();
            let count = tabs.len();
            if count == 0 {
                return;
            }
            pick(self.state.get_untracked().min(count - 1), count)
                .into_iter()
                .find(|&index| !tabs[index].disabled.get_untracked())
                .map(|index| (index, tabs[index].handle))
        };
        if let Some((index, handle)) = target {
            self.state.set(index);
            handle.focus();
        }
    }
}

/// Builder returned by [`tabs`].
pub struct TabsBuilder {
    index: Option<Signal<usize>>,
    default_index: usize,
    on_change: Callback<usize>,
    variant: TabsVariant,
    palette: Palette,
    size: ControlSize,
    fitted: bool,
    style: Style,
}

/// The root of a tab view: a column holding a [`tab_list`] and
/// [`tab_panels`] (pass them with `.children(..)`).
///
/// Bind the selected index with [`index`](TabsBuilder::index) (controlled)
/// or start it with [`default_index`](TabsBuilder::default_index);
/// [`on_change`](TabsBuilder::on_change) runs after each selection by the
/// user.
pub fn tabs() -> TabsBuilder {
    TabsBuilder {
        index: None,
        default_index: 0,
        on_change: Callback::new(),
        variant: TabsVariant::Line,
        palette: Palette::Blue,
        size: ControlSize::Md,
        fitted: false,
        style: Style::new(),
    }
}

impl TabsBuilder {
    /// Two-way bind the selected index (controlled).
    pub fn index(mut self, index: Signal<usize>) -> Self {
        self.index = Some(index);
        self
    }

    /// The starting index when no [`index`](Self::index) signal is bound.
    pub fn default_index(mut self, index: usize) -> Self {
        self.default_index = index;
        self
    }

    /// Called with the new index after each selection by the user.
    pub fn on_change(self, handler: impl FnMut(&usize) + 'static) -> Self {
        self.on_change.add(handler);
        self
    }

    pub fn variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The selected color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Small, medium, or large tabs; `Xs` draws like `Sm`.
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Stretch the tabs to share the list's width (Chakra's `isFitted`).
    pub fn fitted(mut self, fitted: bool) -> Self {
        self.fitted = fitted;
        self
    }
}

impl Styleable for TabsBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TabsBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            index,
            default_index,
            on_change,
            variant,
            palette,
            size,
            fitted,
            style,
        } = self;
        let index = index.unwrap_or_else(|| signal(default_index));
        provide_part(TabsContext {
            state: Controllable::new(index, on_change),
            variant,
            palette,
            size,
            fitted,
            tabs: Rc::default(),
            version: signal(0),
            panels: Rc::default(),
        });
        flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .style(style)
            .build(ctx)
    }
}

/// Builder returned by [`tab_list`].
pub struct TabListBuilder {
    style: Style,
}

/// The row of [`tab`]s inside [`tabs`]; it draws the sliding indicator.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _view = tabs().variant(TabsVariant::Enclosed).children((
///     tab_list().children((tab("One"), tab("Two"))),
///     tab_panels().children((tab_panel().children(text("1")), tab_panel().children(text("2")))),
/// ));
/// ```
pub fn tab_list() -> TabListBuilder {
    TabListBuilder { style: Style::new() }
}

impl Styleable for TabListBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TabListBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let tabs = TabsContext::context();
        let list = NodeHandle::new();
        let dark = system::is_dark();
        let palette = tabs.palette;
        let variant = tabs.variant;
        let line = match variant {
            TabsVariant::Line => 2.,
            TabsVariant::Enclosed => 1.,
            TabsVariant::SoftRounded => 0.,
        };
        // The selected tab's box relative to the list, from the published
        // layout rects (tracked, so it follows selection and layout).
        let geometry = {
            let tabs = tabs.clone();
            computed(move |_| {
                tabs.version.get();
                let index = tabs.state.get();
                let handle = tabs.tabs.borrow().get(index).map(|entry| entry.handle)?;
                let own = list.get()?.rect()?.get();
                let tab = handle.get()?.rect()?.get();
                if tab.w <= 0. {
                    return None;
                }
                let (x, y) = (tab.x - own.x, tab.y - own.y);
                Some(match variant {
                    TabsVariant::Line => (x, tab.bottom() - own.y, tab.w, line),
                    // Cover the list's line below the selected card.
                    TabsVariant::Enclosed => (x, y, tab.w, tab.h + line),
                    TabsVariant::SoftRounded => (x, y, tab.w, tab.h),
                })
            })
        };
        let part = move |pick: fn((f32, f32, f32, f32)) -> f32| {
            geometry.map(move |geometry| Size::Px(geometry.map_or(0., pick)))
        };
        // Slide only once the indicator has been placed, so it does not fly
        // in from the corner.
        let ready = signal(false);
        let slide = ready.map(|ready| {
            if ready { Transition::spring(0.35, 0.12) } else { Transition::Instant }
        });
        let mut indicator = flexbox()
            .absolute()
            .inset_left(part(|g| g.0))
            .inset_top(part(|g| g.1))
            .width(part(|g| g.2))
            .height(part(|g| g.3))
            .opacity(ready.map(|ready| if ready { 1. } else { 0. }))
            .transition(slide)
            .on_update(move |_| {
                if !ready.get_untracked() && geometry.get_untracked().is_some() {
                    ready.set(true);
                }
            });
        indicator = match variant {
            TabsVariant::Line => indicator
                .background(system::mode(palette.shade(600), palette.shade(300)))
                .z_index(1),
            TabsVariant::Enclosed => indicator
                .z_index(-1)
                .radius_top(system::radius::MD)
                .border_top(Size::Px(1.))
                .border_left(Size::Px(1.))
                .border_right(Size::Px(1.))
                .border_color(system::border())
                .background(system::bg()),
            TabsVariant::SoftRounded => indicator
                .z_index(-1)
                .radius(system::radius::FULL)
                .background(computed(move |_| {
                    Brush::Solid(if dark.get() { palette.alpha(200, 0.16) } else { palette.shade(100) })
                })),
        };
        // The caller's tabs and styles go to the row; the outer node holds
        // the line and the indicator.
        let mut row = flexbox().flex_row().items_stretch().flex_grow(1.);
        if variant == TabsVariant::SoftRounded {
            row = row.gap(Size::Rem(0.25));
        }
        let row = row.style(self.style);
        let mut root = flexbox()
            .node_handle(list)
            .relative()
            .flex_row()
            .width(Size::Percent(100.));
        if variant != TabsVariant::SoftRounded {
            root = root.border_bottom(Size::Px(line)).border_color(system::border());
        }
        root.children((row, indicator)).build(ctx)
    }
}

/// Builder returned by [`tab`].
pub struct TabBuilder {
    label: Prop<Rc<str>>,
    disabled: Prop<bool>,
    style: Style,
}

/// One tab inside a [`tab_list`]; activating it shows the panel with the
/// same position.
///
/// It is a hit target of at least 44 × 44 logical pixels and activates on a
/// click, a tap, or Enter or Space while focused.
///
/// # Panics
///
/// When built outside [`tabs`].
pub fn tab(label: impl Into<Prop<Rc<str>>>) -> TabBuilder {
    TabBuilder {
        label: label.into(),
        disabled: Prop::Static(false),
        style: Style::new(),
    }
}

impl TabBuilder {
    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }
}

impl Styleable for TabBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TabBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            label,
            disabled,
            style,
        } = self;
        let tabs = TabsContext::context();
        let handle = NodeHandle::new();
        let index = {
            let mut entries = tabs.tabs.borrow_mut();
            entries.push(TabEntry { handle, disabled });
            entries.len() - 1
        };
        tabs.version.update(|version| *version += 1);
        let selected = {
            let state = tabs.state.clone();
            computed(move |_| state.get() == index)
        };
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let palette = tabs.palette;
        let variant = tabs.variant;
        let fitted = tabs.fitted;
        let foreground = computed(move |_| {
            let dark = dark.get();
            let color = if selected.get() {
                match (variant, dark) {
                    (TabsVariant::SoftRounded, false) => palette.shade(700),
                    (TabsVariant::SoftRounded, true) => palette.shade(200),
                    (_, false) => palette.shade(600),
                    (_, true) => palette.shade(300),
                }
            } else if hovered.get() && !disabled.get() {
                if dark { white_alpha(0.92) } else { Palette::Gray.shade(800) }
            } else if dark {
                Palette::Gray.shade(400)
            } else {
                Palette::Gray.shade(600)
            };
            Brush::Solid(color)
        });
        let background = computed(move |_| {
            let tint = pressed.get() && !disabled.get() && !selected.get();
            Brush::Solid(if !tint {
                Color::TRANSPARENT
            } else if dark.get() {
                white_alpha(0.06)
            } else {
                Palette::Gray.shade(100)
            })
        });
        let size = tabs.size;
        let (pad_y, font_size, line_height) = match size {
            ControlSize::Xs | ControlSize::Sm => (0.25, system::font_size::SM, 1.25),
            ControlSize::Md => (0.5, system::font_size::MD, 1.5),
            ControlSize::Lg => (0.75, system::font_size::LG, 1.75),
        };
        let tab_index = {
            let tabs = tabs.clone();
            computed(move |_| {
                tabs.version.get();
                let current = tabs.state.get();
                let entries = tabs.tabs.borrow();
                let stop = if entries.get(current).is_some_and(|entry| !entry.disabled.get()) {
                    current
                } else {
                    entries.iter().position(|entry| !entry.disabled.get()).unwrap_or(0)
                };
                if stop == index { 0 } else { -1 }
            })
        };
        let select = {
            let state = tabs.state.clone();
            move || {
                if !disabled.get_untracked() {
                    state.set(index);
                }
            }
        };
        let mut root = flexbox()
            .node_handle(handle)
            .flex_row()
            .items_center()
            .justify_center()
            .flex_shrink(0.)
            .padding_x(Size::Rem(1.))
            .padding_y(Size::Rem(pad_y))
            .background(background)
            .opacity(disabled.map(|disabled| if disabled { 0.4 } else { 1. }))
            .transition(Transition::ease_out(0.15))
            .focusable(disabled.map(|disabled| !disabled))
            .tab_index(tab_index)
            .min_hit_size(44.)
            .cursor(disabled.map(|disabled| {
                if disabled { Cursor::NotAllowed } else { Cursor::Pointer }
            }))
            .use_pressed(pressed)
            .on_hover_event(move |event| match event.action() {
                PointerAction::Down => hovered.set(true),
                PointerAction::Up => hovered.set(false),
                _ => {}
            })
            .on_activate({
                let select = select.clone();
                move || select()
            })
            .on_key_down(move |event| {
                match event.key {
                    KeyCode::Enter | KeyCode::KpEnter | KeyCode::Space => {
                        if !event.repeat {
                            select();
                        }
                    }
                    KeyCode::Right | KeyCode::Down => {
                        tabs.select_by(|from, n| (1..n).map(|k| (from + k) % n).collect());
                    }
                    KeyCode::Left | KeyCode::Up => {
                        tabs.select_by(|from, n| (1..n).map(|k| (from + n - k) % n).collect());
                    }
                    KeyCode::Home => tabs.select_by(|_, n| (0..n).collect()),
                    KeyCode::End => tabs.select_by(|_, n| (0..n).rev().collect()),
                    _ => return,
                }
                event.stop_propagation();
            });
        root = match variant {
            TabsVariant::Line => root,
            TabsVariant::Enclosed => root.radius_top(system::radius::MD),
            TabsVariant::SoftRounded => root
                .radius(system::radius::FULL)
                .focus_ring(2., system::focus_ring(), 2.),
        };
        if variant != TabsVariant::SoftRounded {
            root = root.focus_ring(2., system::focus_ring(), -2.);
        }
        if fitted {
            root = root.flex_grow(1.).flex_shrink(1.);
        }
        let weight = if variant == TabsVariant::SoftRounded {
            cosmic_text::Weight::SEMIBOLD
        } else {
            cosmic_text::Weight::MEDIUM
        };
        root.children(
            text(label)
                .font_size(font_size)
                .line_height(Size::Rem(line_height))
                .font_weight(weight)
                .text_wrap(false)
                .color(foreground),
        )
        .style(style)
        .build(ctx)
    }
}

/// Builder returned by [`tab_panels`].
pub struct TabPanelsBuilder {
    style: Style,
}

/// The container of [`tab_panel`]s inside [`tabs`].
///
/// ```
/// use mue_components::prelude::*;
///
/// let _panels = tab_panels().children((tab_panel().children(text("First")),));
/// ```
pub fn tab_panels() -> TabPanelsBuilder {
    TabPanelsBuilder { style: Style::new() }
}

impl Styleable for TabPanelsBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TabPanelsBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        flexbox()
            .flex_column()
            .width(Size::Percent(100.))
            .style(self.style)
            .build(ctx)
    }
}

/// Builder returned by [`tab_panel`].
pub struct TabPanelBuilder {
    style: Style,
}

/// The content shown while the [`tab`] at the same position is selected;
/// pass it with `.children(..)`. It fades in when selected and keeps its
/// state while hidden.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _panel = tab_panel().children((text("Hello"), button("Save")));
/// ```
///
/// # Panics
///
/// When built outside [`tabs`].
pub fn tab_panel() -> TabPanelBuilder {
    TabPanelBuilder { style: Style::new() }
}

impl Styleable for TabPanelBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for TabPanelBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let tabs: TabsContext = use_part("tab_panel", "tabs");
        let index = tabs.panels.get();
        tabs.panels.set(index + 1);
        let selected = {
            let state = tabs.state.clone();
            computed(move |_| state.get() == index)
        };
        flexbox()
            .flex_column()
            .padding((Size::Rem(1.),))
            .color(system::fg())
            .display(selected.map(|selected| if selected { Display::Flex } else { Display::None }))
            .opacity(selected.map(|selected| if selected { 1. } else { 0. }))
            .transition(Transition::ease_out(0.2))
            .style(self.style)
            .build(ctx)
    }
}
