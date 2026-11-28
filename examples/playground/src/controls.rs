//! The playground's small design kit: theme tokens, page sections, option
//! pickers, readouts, an event log, and manual test checklists.

use super::responsive;
use mue_components::*;
use mue_core::prelude::*;
use mue_components::system::{ControlSize, Palette};
use mue_macroquad::{
    motion::transition::Transition,
    paint::color::Color,
    paint::shader::Brush,
    style::{FontSize, Size, Styleable, StyleableExt},
};
use std::{cell::RefCell, rc::Rc};
use taffy::{AlignItems, FlexDirection};

/// Accent colors that stay the same in every theme.
pub const BLUE: Color = Color::from_rgba(110, 160, 245, 255);
pub const CORAL: Color = Color::from_rgba(245, 165, 112, 255);
pub const GREEN: Color = Color::from_rgba(111, 207, 151, 255);
pub const RED: Color = Color::from_rgba(235, 103, 103, 255);
pub const SPRING: Transition = Transition::spring(0.3, 0.1);

/// One theme: the same token names, different values.
struct ThemeValues {
    brushes: TokenSet<Brush>,
    colors: TokenSet<Color>,
    numbers: TokenSet<f32>,
}

impl ThemeValues {
    fn register(&self, theme: &Theme) {
        theme.register(&self.brushes);
        theme.register(&self.colors);
        theme.register(&self.numbers);
    }

    fn apply(&self, theme: &Theme) {
        theme.apply(&self.brushes);
        theme.apply(&self.colors);
        theme.apply(&self.numbers);
    }
}

fn light_palette() -> ThemeValues {
    let canvas = Color::from_rgba(240, 243, 248, 255);
    ThemeValues {
        brushes: token_set! {
            canvas: Brush::Solid(canvas),
            panel: Brush::Solid(Color::WHITE),
            raised: Brush::Solid(Color::from_rgba(228, 233, 242, 255)),
            border: Brush::Solid(Color::from_rgba(214, 220, 231, 255)),
            foreground: Brush::Solid(Color::from_rgba(31, 41, 55, 255)),
            muted: Brush::Solid(Color::from_rgba(96, 108, 128, 255)),
            "tile.panel": Brush::Solid(Color::from_rgba(228, 233, 242, 255)),
            "tile.accent": Brush::Solid(CORAL),
        },
        colors: token_set! { canvas: canvas },
        numbers: token_set! {
            "tile.scale.panel": 1.02_f32,
            "tile.scale.accent": 1.04_f32,
        },
    }
}

fn dark_palette() -> ThemeValues {
    let canvas = Color::from_rgba(17, 21, 30, 255);
    ThemeValues {
        brushes: token_set! {
            canvas: Brush::Solid(canvas),
            panel: Brush::Solid(Color::from_rgba(27, 33, 46, 255)),
            raised: Brush::Solid(Color::from_rgba(38, 46, 63, 255)),
            border: Brush::Solid(Color::from_rgba(45, 54, 73, 255)),
            foreground: Brush::Solid(Color::from_rgba(236, 240, 247, 255)),
            muted: Brush::Solid(Color::from_rgba(146, 157, 178, 255)),
            "tile.panel": Brush::Solid(Color::from_rgba(38, 46, 63, 255)),
            "tile.accent": Brush::Solid(BLUE),
        },
        colors: token_set! { canvas: canvas },
        numbers: token_set! {
            "tile.scale.panel": 1_f32,
            "tile.scale.accent": 1_f32,
        },
    }
}

thread_local! {
    static DARK: Signal<bool> = Scope::null().run(|| signal(true));
    // Every token is registered once, at startup; reads only hand out handles.
    static THEME: Rc<Theme> = Scope::null().run(|| {
        let theme = Theme::new();
        dark_palette().register(&theme);
        Rc::new(theme)
    });
}

/// The application theme, shared by every page.
pub fn theme() -> Rc<Theme> {
    THEME.with(Rc::clone)
}

/// Switch every token in place; existing bindings update on their own.
pub fn set_dark(dark: bool) {
    if DARK.with(|signal| signal.get_untracked()) == dark {
        return;
    }
    DARK.with(|signal| signal.set(dark));
    let palette = if dark {
        dark_palette()
    } else {
        light_palette()
    };
    THEME.with(|theme| palette.apply(theme));
}

fn brush(name: &str) -> Prop<Brush> {
    THEME.with(|theme| theme.get(name))
}

pub fn background() -> Prop<Brush> {
    brush("canvas")
}

pub fn panel_brush() -> Prop<Brush> {
    brush("panel")
}

pub fn raised() -> Prop<Brush> {
    brush("raised")
}

pub fn border() -> Prop<Brush> {
    brush("border")
}

pub fn foreground() -> Prop<Brush> {
    brush("foreground")
}

pub fn muted() -> Prop<Brush> {
    brush("muted")
}

pub fn background_color() -> Color {
    THEME.with(|theme| theme.get::<Color>("canvas").get())
}

/// A length in design units (scaled with the window and the zoom setting).
pub fn u(value: f32) -> Prop<Size> {
    responsive::ui(value)
}

/// A font size in design units.
pub fn f(value: f32) -> Prop<FontSize> {
    responsive::font(value)
}

/// Body text.
pub fn para(content: impl Into<Prop<Rc<str>>>) -> Node {
    text(content)
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            font-size: {f(15.)}; line-height: {u(22.)};
            color: {foreground()};
        })
        .into_node()
}

/// Secondary text: explanations and hints.
pub fn hint(content: impl Into<Prop<Rc<str>>>) -> Node {
    text(content)
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            font-size: {f(13.)}; line-height: {u(19.)};
            color: {muted()};
        })
        .into_node()
}

/// Monospace readout text.
pub fn mono(content: impl Into<Prop<Rc<str>>>) -> Node {
    text(content)
        .style(mue_macroquad::style! {
            min-width: 0px;
            font-size: {f(13.)}; line-height: {u(19.)};
            font-family: monospace;
            color: {foreground()};
        })
        .into_node()
}

/// A page: an introduction followed by its cards.
pub fn page_body(intro: &str, cards: impl IntoChildren) -> Node {
    stack()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            gap: {u(16.)};
        })
        .children((hint(Rc::<str>::from(intro)), cards))
        .into_node()
}

/// A titled card with a short description of what it demonstrates.
pub fn card(title: &str, description: &str, content: impl IntoChildren) -> Node {
    let mut header = vec![
        text(Rc::<str>::from(title))
            .style(mue_macroquad::style! {
                font-size: {f(17.)}; line-height: {u(24.)};
                font-weight: 700;
                color: {foreground()};
            })
            .into_node(),
    ];
    if !description.is_empty() {
        header.push(hint(Rc::<str>::from(description)));
    }
    stack()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            padding: {(u(16.),)};
            gap: {u(12.)};
            border-radius: {u(12.)};
            border-width: 1px;
            border-color: {border()};
            background: {panel_brush()};
        })
        .children((
            stack()
                .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(2.)}; })
                .children(header),
            content,
        ))
        .into_node()
}

/// A horizontal row of controls that wraps on narrow windows.
pub fn row(children: impl IntoChildren) -> Node {
    wrap()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            gap: {u(8.)};
            align-items: center;
        })
        .children(children)
        .into_node()
}

/// A labeled control row: the label on the left, the control after it. On
/// narrow windows the label sits above the control.
pub fn field(label: &str, control: impl IntoNode + 'static) -> Node {
    let compact = responsive::compact();
    flexbox()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            flex-direction: {compact.map(|compact| if compact { FlexDirection::Column } else { FlexDirection::Row })};
            gap: {compact.map(|compact| Size::Px(if compact { 4. } else { 12. }))};
            align-items: {compact.map(|compact| Some(if compact { AlignItems::Stretch } else { AlignItems::Center }))};
        })
        .children((
            text(Rc::<str>::from(label)).style(mue_macroquad::style! {
                flex: none;
                width: {compact.map(|compact| if compact { Size::Auto } else { Size::Rem(7.) })};
                font-size: {f(13.)}; line-height: {u(18.)};
                color: {muted()};
            }),
            flexbox()
                .style(mue_macroquad::style! { flex: 1; min-width: 0px; })
                .children(control.into_node()),
        ))
        .into_node()
}

/// A segmented picker bound to `model`: soft-rounded [`tabs`] without
/// panels, so the selection pill slides between the options. The options
/// wrap onto more lines on narrow windows.
pub fn picker<T: Clone + PartialEq + 'static>(
    options: Vec<(&'static str, T)>,
    model: Signal<T>,
) -> Node {
    let options = Rc::new(options);
    let find = {
        let options = options.clone();
        move |value: &T| options.iter().position(|(_, option)| option == value).unwrap_or(0)
    };
    let index = signal(find(&model.get_clone_untracked()));
    // Follow changes of the model made elsewhere.
    watch(*model, move |model| {
        let at = find(&model.get_clone());
        if index.get_untracked() != at {
            index.set(at);
        }
    });
    let labels = options
        .iter()
        .map(|(name, _)| tab(*name))
        .collect::<Vec<_>>();
    tabs()
        .index(index)
        .variant(TabsVariant::SoftRounded)
        .size(ControlSize::Sm)
        .palette(Palette::Blue)
        .on_change(move |at: &usize| model.set(options[*at].1.clone()))
        .children(tab_list().style(mue_macroquad::style! { flex-wrap: wrap; }).children(labels))
        .into_node()
}

/// Pick one of several values with a labeled [`picker`]; every option is
/// visible and the first one starts selected.
pub fn segmented<T: Clone + PartialEq + 'static>(
    label: &str,
    options: Vec<(&'static str, T)>,
) -> (ReadSignal<T>, Node) {
    let model = signal(options[0].1.clone());
    (*model, field(label, picker(options, model)))
}

/// A small labeled [`switch`]; several fit in one `row`.
pub fn option_switch(label: &str, initial: bool) -> (Signal<bool>, Node) {
    let value = signal(initial);
    let control = switch()
        .checked(value)
        .label(Rc::<str>::from(label))
        .size(ControlSize::Sm)
        .into_node();
    (value, control)
}

/// A small labeled value tile.
pub fn stat(label: &str, value: impl Into<Prop<Rc<str>>>) -> Node {
    stack()
        .style(mue_macroquad::style! {
            flex: none;
            min-width: {u(132.)};
            padding: {(u(12.), u(8.))};
            gap: {u(2.)};
            border-radius: {u(8.)};
            background: {raised()};
        })
        .children((
            text(Rc::<str>::from(label)).style(mue_macroquad::style! {
                font-size: {f(11.)}; line-height: {u(15.)};
                color: {muted()};
            }),
            text(value).style(mue_macroquad::style! {
                font-size: {f(15.)}; line-height: {u(21.)};
                font-family: monospace;
                color: {foreground()};
            }),
        ))
        .into_node()
}

/// A rectangular demo surface.
pub fn surface() -> FlexboxBuilder {
    flexbox().style(mue_macroquad::style! {
        width: 100%; min-width: 0px;
        border-radius: {u(10.)};
        background: {raised()};
    })
}

// ----- Event log ---------------------------------------------------------

type LogLines = Rc<[(u32, Rc<str>)]>;

thread_local! {
    static LOG: Signal<LogLines> = Scope::null().run(|| signal(Rc::from(Vec::new())));
    static LOG_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Append a line to the shared event log (the newest 8 lines are kept).
pub fn log_event(line: impl Into<String>) {
    let id = LOG_ID.with(|id| {
        id.set(id.get() + 1);
        id.get()
    });
    let line: Rc<str> = Rc::from(line.into());
    LOG.with(|log| {
        let mut lines = log.get_clone_untracked().to_vec();
        lines.insert(0, (id, line));
        lines.truncate(8);
        log.set(Rc::from(lines));
    });
}

/// The shared event log: what the platform just reported.
pub fn event_log() -> Node {
    let log = LOG.with(|log| *log);
    stack()
        .style(mue_macroquad::style! {
            width: 100%; min-width: 0px;
            min-height: {u(60.)};
            padding: {(u(12.), u(8.))};
            gap: {u(2.)};
            border-radius: {u(8.)};
            background: {raised()};
        })
        .children((
            text(log.map(|lines| {
                Rc::<str>::from(if lines.is_empty() {
                    "Event log: interact with the demos above"
                } else {
                    "Event log (newest first)"
                })
            }))
            .style(mue_macroquad::style! {
                font-size: {f(11.)}; line-height: {u(15.)};
                color: {muted()};
            }),
            map_keyed(log, |entry| entry.0, |entry| mono(entry.1.clone())),
        ))
        .into_node()
}

// ----- Manual checklists -------------------------------------------------

/// The result of a manual check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Untested,
    Pass,
    Fail,
}

struct CheckItem {
    page: &'static str,
    label: &'static str,
    state: Signal<Check>,
}

thread_local! {
    static CHECKS: RefCell<Vec<Rc<CheckItem>>> = const { RefCell::new(Vec::new()) };
    // Bumped when a page registers checks, so summaries recount.
    static CHECKS_VERSION: Signal<u32> = Scope::null().run(|| signal(0));
}

fn check_item(page: &'static str, label: &'static str) -> Rc<CheckItem> {
    let existing = CHECKS.with(|checks| {
        checks
            .borrow()
            .iter()
            .find(|item| item.page == page && item.label == label)
            .cloned()
    });
    if let Some(item) = existing {
        return item;
    }
    let item = Rc::new(CheckItem {
        page,
        label,
        state: Scope::null().run(|| signal(Check::Untested)),
    });
    CHECKS.with(|checks| checks.borrow_mut().push(item.clone()));
    // Outside the borrow: summaries recount as soon as the version changes.
    CHECKS_VERSION.with(|version| version.update(|v| *v += 1));
    item
}

/// Results per page: (page, passed, failed, total). Tracked.
pub fn check_summary() -> Vec<(&'static str, usize, usize, usize)> {
    CHECKS_VERSION.with(|version| version.get());
    let items: Vec<Rc<CheckItem>> = CHECKS.with(|checks| checks.borrow().clone());
    let mut summary: Vec<(&'static str, usize, usize, usize)> = Vec::new();
    for item in items {
        let state = item.state.get();
        let index = match summary.iter().position(|entry| entry.0 == item.page) {
            Some(index) => index,
            None => {
                summary.push((item.page, 0, 0, 0));
                summary.len() - 1
            }
        };
        let entry = &mut summary[index];
        entry.3 += 1;
        match state {
            Check::Pass => entry.1 += 1,
            Check::Fail => entry.2 += 1,
            Check::Untested => {}
        }
    }
    summary
}

/// Mark every manual check untested.
pub fn reset_checks() {
    CHECKS.with(|checks| {
        for item in checks.borrow().iter() {
            item.state.set(Check::Untested);
        }
    });
}

/// What a tester should verify on this page. Tap a line to cycle it through
/// untested, pass, and fail; the overview page sums the results.
pub fn checklist(page: &'static str, labels: &[&'static str]) -> Node {
    let rows = labels
        .iter()
        .map(|label| {
            let item = check_item(page, label);
            let state = item.state;
            let mark = state.map(|state| {
                Rc::<str>::from(match state {
                    Check::Untested => "○",
                    Check::Pass => "✓",
                    Check::Fail => "✗",
                })
            });
            let tint = state.map(|state| match state {
                Check::Untested => Brush::Solid(Color::from_rgba(146, 157, 178, 255)),
                Check::Pass => Brush::Solid(GREEN),
                Check::Fail => Brush::Solid(RED),
            });
            link_overlay()
                .style(mue_macroquad::style! {
                    width: 100%; min-width: 0px;
                    padding: {(u(10.), u(6.))};
                    gap: {u(10.)};
                    border-radius: {u(8.)};
                    flex-direction: row;
                    align-items: center;
                })
                .with_hovered(move |view, hovered| {
                    view.style(mue_macroquad::style! {
                        background: {hovered.map(|on| Brush::Solid(if on {
                            Color::from_rgba(128, 140, 170, 30)
                        } else {
                            Color::TRANSPARENT
                        }))};
                    })
                })
                .on_activate(move || {
                    state.update(|state| {
                        *state = match state {
                            Check::Untested => Check::Pass,
                            Check::Pass => Check::Fail,
                            Check::Fail => Check::Untested,
                        }
                    })
                })
                .children((
                    text(mark).style(mue_macroquad::style! {
                        flex: none;
                        width: {u(18.)};
                        font-size: {f(15.)}; line-height: {u(20.)};
                        font-family: monospace;
                        color: {tint};
                    }),
                    text(*label).style(mue_macroquad::style! {
                        flex: 1; min-width: 0px;
                        font-size: {f(14.)}; line-height: {u(20.)};
                        color: {foreground()};
                    }),
                ))
                .into_node()
        })
        .collect::<Vec<_>>();
    card(
        "Manual checks",
        "Tap a line to mark it ✓ passed, ✗ failed, or ○ untested. The Overview page totals them.",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: 0px; })
            .children(rows),
    )
}
