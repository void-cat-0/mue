//! An interactive tour and test bench for mue on every platform.
//!
//! Each page demonstrates one area, shows live readouts of what the platform
//! reports, and ends with a checklist of what to verify by hand. The Overview
//! page summarizes the device and the checklist results; Self tests runs
//! automated checks on the device itself.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example playground`.
use mue_components::*;
mod assets;
mod controls;
mod feedback;
mod fonts;
mod graphics;
#[path = "../../shared/halftone.rs"]
mod halftone;
mod layout;
mod motion;
mod overlays;
mod overview;
mod pointer;
#[path = "../../shared/responsive.rs"]
mod responsive;
mod scrolling;
mod selftest;
mod styles;
mod text_input;
mod theming;
mod typography;
use controls::*;
use macroquad::prelude::{clear_background, get_time, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    font::{self, fontdb},
    paint::shader::Brush,
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;

/// Every page: its title and the section of the navigation it belongs to.
const PAGES: [(&str, &str); 14] = [
    ("Overview", "Start"),
    ("Self tests", "Start"),
    ("Layout", "Rendering"),
    ("Typography", "Rendering"),
    ("Graphics", "Rendering"),
    ("Motion", "Rendering"),
    ("Text input", "Input"),
    ("Pointer & gestures", "Input"),
    ("Scrolling", "Input"),
    ("Overlays & focus", "Input"),
    ("Feedback & directives", "Input"),
    ("Theme & media", "App"),
    ("Style declarations", "App"),
    ("Font licenses", "App"),
];

thread_local! {
    static FPS: Signal<u32> = Scope::null().run(|| signal(0));
    static SELECTED: Signal<usize> = Scope::null().run(|| signal(0));
}

/// Frames per second, averaged over half a second.
pub fn fps() -> ReadSignal<u32> {
    FPS.with(|fps| **fps)
}

/// Open a page by index.
pub fn open_page(index: usize) {
    SELECTED.with(|selected| selected.set(index.min(PAGES.len() - 1)));
}

fn destination(index: usize) -> Node {
    let content = match index {
        0 => overview::page(),
        1 => selftest::page(),
        2 => layout::page(),
        3 => typography::page(),
        4 => graphics::page(),
        5 => motion::page(),
        6 => text_input::page(),
        7 => pointer::page(),
        8 => scrolling::page(),
        9 => overlays::page(),
        10 => feedback::page(),
        11 => theming::page(),
        12 => styles::page(),
        _ => fonts::licenses(),
    };
    scroll_area()
        .style(mue_macroquad::style! { width: 100%; height: 100%; })
        .options(ScrollOptions {
            touch: true,
            ..Default::default()
        })
        .children(
            container()
                .style(mue_macroquad::style! {
                    width: 100%;
                    max-width: {u(960.)};
                    min-width: 0px;
                    padding: {(u(20.), u(20.))};
                    gap: {u(8.)};
                    // Components without their own size (buttons, inputs) inherit this.
                    font-size: {f(15.)};
                })
                .children((
                    heading(PAGES[index].0).style(mue_macroquad::style! {
                        font-size: {f(26.)};
                        line-height: {u(34.)};
                        color: {foreground()};
                    }),
                    content,
                )),
        )
        .into_node()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - playground".into(),
            high_dpi: true,
            ..Default::default()
        },
        run(),
    );
}

pub async fn run() {
    responsive::sync();
    // Font parsing is CPU-heavy. Build the database on a blocking thread and
    // keep a text-free loading screen until it is ready.
    let mut font_db = Some(AsyncResource::<fontdb::Database, String>::new_blocking(
        || Ok(fonts::database()),
    ));
    let mut loading_app = Some(App::new(loading_screen()));
    let mut real_app: Option<App> = None;
    let mut frames = 0;
    let mut sampled_at = get_time();
    loop {
        responsive::sync();
        styles::update_animation_time(get_time() as f32);
        clear_background(background_color().into());
        if real_app.is_none() {
            let ready = font_db.as_ref().and_then(|resource| {
                match resource.state().get_clone_untracked() {
                    AsyncState::Ready(Ok(db)) => Some(db),
                    _ => None,
                }
            });
            if let Some(db) = ready {
                font::initialize("zh-CN", db)
                    .expect("initialize bundled fonts before creating text");
                real_app = Some(App::new(build_ui()));
                loading_app = None;
                font_db = None;
            }
        }
        if let Some(app) = real_app.as_mut() {
            app.frame();
        } else if let Some(app) = loading_app.as_mut() {
            app.frame();
        }
        next_frame().await;
        frames += 1;
        let now = get_time();
        let elapsed = now - sampled_at;
        if elapsed >= 0.5 {
            FPS.with(|fps| fps.set((frames as f64 / elapsed).round() as u32));
            frames = 0;
            sampled_at = now;
        }
    }
}

/// One navigation entry.
fn nav_item(index: usize, selected: Signal<usize>, compact: bool) -> Node {
    let active = selected.map(move |page| page == index);
    link_overlay()
        .style(mue_macroquad::style! {
            flex: none;
            padding: {(u(12.), u(7.))};
            min-height: {u(34.)};
            border-radius: {u(8.)};
            background: {active.pick(Brush::Solid(BLUE), Brush::Solid(mue_macroquad::paint::color::Color::TRANSPARENT))};
            align-items: center;
            flex-direction: row;
            transition: 120ms ease-out;
        })
        .style(if compact {
            mue_macroquad::style! {}
        } else {
            mue_macroquad::style! { width: 100%; }
        })
        .on_activate(move || selected.set(index))
        .children(text(PAGES[index].0).style(mue_macroquad::style! {
            font-size: {f(14.)};
            line-height: {u(20.)};
            color: {active.pick(Brush::Solid(mue_macroquad::paint::color::Color::from_rgba(19, 23, 32, 255)), foreground())};
            text-wrap: {false};
        }))
        .into_node()
}

/// The sidebar for wide windows, grouped by section.
fn sidebar(selected: Signal<usize>) -> Node {
    let mut children: Vec<Node> = Vec::new();
    let mut section = "";
    for (index, (_, group)) in PAGES.iter().enumerate() {
        if *group != section {
            section = group;
            children.push(
                text(*group)
                    .style(mue_macroquad::style! {
                        padding: {(u(12.), u(4.))};
                        margin-top: {u(10.)};
                        font-size: {f(11.)};
                        line-height: {u(16.)};
                        font-weight: 700;
                        color: {muted()};
                    })
                    .into_node(),
            );
        }
        children.push(nav_item(index, selected, false));
    }
    scroll_area()
        .style(mue_macroquad::style! {
            width: {u(200.)};
            height: 100%;
            flex: none;
            background: {panel_brush()};
            border-right-width: 1px;
            border-color: {border()};
        })
        .content_style(mue_macroquad::style! {
            padding: {(u(8.), u(8.))};
            gap: {u(2.)};
        })
        .children(children)
        .into_node()
}

/// Horizontally scrolling tabs for narrow windows.
fn tab_strip(selected: Signal<usize>) -> Node {
    scroll_area()
        .axis(ScrollAxis::Horizontal)
        .style(mue_macroquad::style! {
            width: 100%;
            height: {u(50.)};
            flex: none;
            background: {panel_brush()};
        })
        .options(ScrollOptions {
            scrollbars: false,
            axis: ScrollAxis::Horizontal,
            ..Default::default()
        })
        .content_style(mue_macroquad::style! {
            gap: {u(4.)};
            padding: {(u(8.), u(0.))};
            align-items: center;
        })
        .children(
            (0..PAGES.len())
                .map(|index| nav_item(index, selected, true))
                .collect::<Vec<_>>(),
        )
        .into_node()
}

fn top_bar(selected: Signal<usize>) -> Node {
    let zoom = responsive::zoom();
    let color = use_color_mode();
    hstack()
        .style(mue_macroquad::style! {
            width: 100%;
            flex: none;
            padding: {(u(16.), u(8.))};
            gap: {u(8.)};
            align-items: center;
            background: {panel_brush()};
            border-bottom-width: 1px;
            border-color: {border()};
        })
        .children((
            text("mue playground").style(mue_macroquad::style! {
                font-size: {f(15.)};
                line-height: {u(22.)};
                font-weight: 700;
                color: {foreground()};
                text-wrap: {false};
            }),
            text(selected.map(|index| Rc::<str>::from(format!("{} / {}", index + 1, PAGES.len()))))
                .style(mue_macroquad::style! {
                    flex: 1; min-width: 0px;
                    font-size: {f(13.)};
                    line-height: {u(20.)};
                    color: {muted()};
                }),
            text(fps().map(|fps| Rc::<str>::from(format!("{fps} FPS")))).style(
                mue_macroquad::style! {
                    font-size: {f(13.)};
                    line-height: {u(20.)};
                    font-family: monospace;
                    color: {muted()};
                    text-wrap: {false};
                },
            ),
            chip("‹", false, move || {
                selected.update(|index| *index = index.saturating_sub(1))
            }),
            chip("›", false, move || {
                selected.update(|index| *index = (*index + 1).min(PAGES.len() - 1))
            }),
            chip(
                zoom.map(|zoom| Rc::<str>::from(format!("Zoom {:.0}%", zoom * 100.))),
                false,
                responsive::cycle_zoom,
            ),
            chip(
                color.scheme.map(|scheme| {
                    Rc::<str>::from(match scheme {
                        ColorScheme::Dark => "Dark",
                        ColorScheme::Light => "Light",
                    })
                }),
                false,
                move || color.toggle(),
            ),
        ))
        .into_node()
}

/// Builds a view when it is converted into a node, not when it is created.
struct Deferred<F>(F);

impl<F: FnOnce() -> Node> IntoNode for Deferred<F> {
    fn into_node(self) -> Node {
        (self.0)()
    }
}

/// The whole UI. It is built lazily inside `App::new`.
///
/// The App's layer host makes each of its children a `rem` root, and its
/// layer surfaces have the default font size, so a teleported overlay there
/// would reset the `rem` base. The UI therefore brings its own layer host,
/// below the node that sets the root font size, and builds everything inside
/// it (`Deferred`), so teleports find that host.
fn build_ui() -> impl IntoNode + 'static {
    mue_macroquad::inherit::provide_foreground(foreground(), || {
        Theme::install(&theme());
        // The app follows use_color_mode (light, dark, or the system scheme).
        let color = use_color_mode();
        watch(color.scheme, |scheme| {
            set_dark(scheme.get() == ColorScheme::Dark)
        });
        flexbox()
            .style(mue_macroquad::style! {
                width: 100%; height: 100%;
                font-size: {responsive::root_font()};
            })
            .children(layer_host(Deferred(app_shell)))
            .into_node()
    })
}

/// The navigation chrome around the pages.
fn app_shell() -> Node {
    let selected = SELECTED.with(|selected| *selected);
    // Tabs replace the sidebar when it would leave the page less than
    // 560 design units (narrow phones, or a high zoom setting).
    let viewport = mue_macroquad::app::viewport::use_viewport();
    let zoom = responsive::zoom();
    let compact = computed(move |_| {
        let viewport = viewport.get();
        viewport.width / zoom.get() < 760.
    });
    let page_view = pages(selected, (0..PAGES.len()).map(destination))
        .page_transition(PagesTransition::Slide)
        .transition_curve(SPRING)
        .into_node();
    // Back steps to the previous page before the app closes.
    mue_macroquad::input::back::use_back_handler(selected.map(|index| index > 0), move || {
        selected.update(|index| *index = index.saturating_sub(1))
    });
    stack()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            gap: 0px;
            background: {background()};
        })
        .children((
            top_bar(selected),
            tab_strip(selected).show_if(compact),
            hstack()
                .style(mue_macroquad::style! {
                    width: 100%;
                    flex: 1;
                    min-width: 0px;
                    min-height: 0px;
                    gap: 0px;
                    align-items: stretch;
                })
                .children((
                    sidebar(selected).show_if(compact.map(|compact| !compact)),
                    flexbox()
                        .style(mue_macroquad::style! {
                            flex: 1;
                            min-width: 0px;
                            min-height: 0px;
                            height: 100%;
                        })
                        .children(page_view),
                )),
        ))
        .into_node()
}

fn loading_screen() -> Node {
    stack()
        .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            background: {background()};
            align-items: center;
            justify-content: center;
        })
        .children(
            flexbox()
                .style(mue_macroquad::style! {
                    width: 48px;
                    height: 48px;
                    border-radius: 24px;
                    background: {panel_brush()};
                })
                .into_node(),
        )
        .into_node()
}
