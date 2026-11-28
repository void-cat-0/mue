//! An interactive tour and test bench for mue on every platform.
//!
//! Each page demonstrates one area, shows live readouts of what the platform
//! reports, and ends with a checklist of what to verify by hand. The Overview
//! page summarizes the device and the checklist results; Self tests runs
//! automated checks on the device itself.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example playground`.
use mue_components::*;
use mue_components::system::ControlSize;
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
    App, BoxShadow, BoxShadows,
    font::{self, fontdb},
    motion::transition::Transition,
    node::NodeHandle,
    paint::{color::Color, shader::Brush},
    style::{Size, Styleable, StyleableExt, Transform, TransformOp},
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

/// Space above a page's content: the frosted header floats over it.
fn header_inset(header: ReadSignal<f32>) -> Prop<Size> {
    computed(move |_| Size::Px(header.get() + 20. * responsive::scale())).into()
}

fn destination(index: usize, selected: Signal<usize>, header: ReadSignal<f32>) -> Node {
    let active = selected.map(move |page| page == index);
    build_page(active, || {
        let (title, section) = PAGES[index];
        let enter = entrance();
        let rise = enter.clone().map(|enter| {
            Transform(vec![TransformOp::Translate(Size::Px(0.), Size::Px((1. - enter) * 16.))])
        });
        let page_header = stack()
            .style(mue_macroquad::style! {
                width: 100%; min-width: 0px;
                gap: {u(4.)};
                opacity: {enter};
                transform: {rise};
            })
            .children((
                text(section.to_uppercase()).style(mue_macroquad::style! {
                    font-size: {f(12.)}; line-height: {u(16.)};
                    font-weight: 700;
                    color: {accent_color().map(Brush::Solid)};
                }),
                heading(title).style(mue_macroquad::style! {
                    font-size: {f(30.)};
                    line-height: {u(38.)};
                    color: {foreground()};
                }),
                // A short accent bar under the title.
                flexbox()
                    .style(mue_macroquad::style! {
                        width: {u(48.)}; height: {u(4.)};
                        margin-top: {u(6.)};
                        border-radius: 9999px;
                    })
                    .background(accent_gradient()),
            ));
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
                        max-width: {u(980.)};
                        min-width: 0px;
                        padding: {(u(20.), u(20.))};
                        padding-top: {header_inset(header)};
                        padding-bottom: {u(40.)};
                        gap: {u(14.)};
                        // Components without their own size (buttons, inputs) inherit this.
                        font-size: {f(15.)};
                    })
                    .children((page_header, content)),
            )
            .into_node()
    })
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

/// One navigation entry. The selection pill is drawn by [`nav_list`]
/// underneath; the entry itself only tints while hovered.
fn nav_item(index: usize, selected: Signal<usize>, handle: NodeHandle, compact: bool) -> Node {
    let active = selected.map(move |page| page == index);
    let ink = foreground();
    let label = computed(move |_| {
        if active.get() {
            Brush::Solid(Color::WHITE)
        } else {
            ink.get_clone()
        }
    });
    link_overlay()
        .node_handle(handle)
        .style(mue_macroquad::style! {
            flex: none;
            padding: {(u(12.), u(8.))};
            min-height: {u(36.)};
            border-radius: {u(10.)};
            align-items: center;
            flex-direction: row;
            transition: 160ms ease-out;
        })
        .style(if compact {
            mue_macroquad::style! {}
        } else {
            mue_macroquad::style! { width: 100%; }
        })
        .with_hovered(move |view, hovered| {
            let tint = hover_tint();
            view.style(mue_macroquad::style! {
                background: {computed(move |_| {
                    if hovered.get() && !active.get() {
                        tint.get_clone()
                    } else {
                        Brush::Solid(Color::TRANSPARENT)
                    }
                })};
            })
        })
        .on_activate(move || selected.set(index))
        .children(text(PAGES[index].0).style(mue_macroquad::style! {
            font-size: {f(14.)};
            line-height: {u(20.)};
            font-weight: {active.select(cosmic_text::Weight::SEMIBOLD, cosmic_text::Weight::NORMAL)};
            color: {label};
            text-wrap: {false};
        }))
        .into_node()
}

/// The navigation entries with a gradient selection pill that slides (with
/// a spring) to the selected entry. Vertical lists group the entries under
/// their section names.
fn nav_list(selected: Signal<usize>, horizontal: bool) -> (Node, Rc<Vec<NodeHandle>>) {
    let list = NodeHandle::new();
    let handles: Rc<Vec<NodeHandle>> = Rc::new((0..PAGES.len()).map(|_| NodeHandle::new()).collect());
    // The selected entry's box relative to the list (tracked, so the pill
    // follows the selection and every layout change).
    let geometry = {
        let handles = handles.clone();
        computed(move |_| {
            let own = list.get()?.rect()?.get();
            let entry = handles.get(selected.get())?.get()?.rect()?.get();
            (entry.w > 0.).then(|| (entry.x - own.x, entry.y - own.y, entry.w, entry.h))
        })
    };
    let part = move |pick: fn((f32, f32, f32, f32)) -> f32| {
        geometry.map(move |geometry| Size::Px(geometry.map_or(0., pick)))
    };
    // Slide only once the pill has been placed, so it does not fly in.
    let ready = signal(false);
    let accent = accent_color();
    let pill = flexbox()
        .absolute()
        .inset_left(part(|g| g.0))
        .inset_top(part(|g| g.1))
        .width(part(|g| g.2))
        .height(part(|g| g.3))
        .background(accent_gradient())
        .box_shadow(computed(move |_| {
            let accent = accent.get();
            BoxShadows::from(BoxShadow::new(0., 4., 16., Color::new(accent.r, accent.g, accent.b, 0.45)))
        }))
        .opacity(ready.map(|ready| if ready { 1. } else { 0. }))
        .transition(ready.map(|ready| {
            if ready { Transition::spring(0.42, 0.16) } else { Transition::Instant }
        }))
        .on_update(move |_| {
            if !ready.get_untracked() && geometry.get_untracked().is_some() {
                ready.set(true);
            }
        })
        .style(mue_macroquad::style! { border-radius: {u(10.)}; });
    let mut children: Vec<Node> = vec![pill.into_node()];
    let mut section = "";
    for (index, (_, group)) in PAGES.iter().enumerate() {
        if !horizontal && *group != section {
            section = group;
            children.push(
                text(group.to_uppercase())
                    .style(mue_macroquad::style! {
                        padding: {(u(12.), u(4.))};
                        margin-top: {u(if index == 0 { 0. } else { 12. })};
                        font-size: {f(11.)};
                        line-height: {u(16.)};
                        font-weight: 700;
                        color: {muted()};
                    })
                    .into_node(),
            );
        }
        children.push(nav_item(index, selected, handles[index], horizontal));
    }
    let node = flexbox()
        .node_handle(list)
        .style(mue_macroquad::style! {
            position: relative;
            gap: {u(2.)};
        })
        .style(if horizontal {
            mue_macroquad::style! { flex-direction: row; align-items: center; }
        } else {
            mue_macroquad::style! { width: 100%; flex-direction: column; }
        })
        .children(children)
        .into_node();
    (node, handles)
}

/// The frosted sidebar for wide windows, grouped by section.
fn sidebar(selected: Signal<usize>, header: ReadSignal<f32>) -> Node {
    let (list, _) = nav_list(selected, false);
    scroll_area()
        .style(mue_macroquad::style! {
            width: {u(216.)};
            height: 100%;
            flex: none;
            background: {glass()};
            backdrop-filter: blur(18px);
            border-right-width: 1px;
            border-color: {border()};
        })
        .content_style(mue_macroquad::style! {
            padding: {(u(10.), u(10.))};
            padding-top: {header_inset(header)};
        })
        .children(list)
        .into_node()
}

/// Horizontally scrolling navigation for narrow windows; it keeps the
/// selected entry in view.
fn tab_strip(selected: Signal<usize>) -> Node {
    let (list, handles) = nav_list(selected, true);
    let strip = ScrollAreaHandle::new();
    {
        let strip = strip.clone();
        watch(*selected, move |selected| {
            if let Some(rect) = handles.get(selected.get()).and_then(|handle| handle.rect()) {
                strip.scroll_into_view(rect, true);
            }
        });
    }
    scroll_area()
        .axis(ScrollAxis::Horizontal)
        .handle(strip)
        .style(mue_macroquad::style! {
            width: 100%;
            height: {u(50.)};
            flex: none;
        })
        .options(ScrollOptions {
            scrollbars: false,
            axis: ScrollAxis::Horizontal,
            ..Default::default()
        })
        .content_style(mue_macroquad::style! {
            padding: {(u(10.), u(0.))};
            align-items: center;
        })
        .children(list)
        .into_node()
}

/// The app's logo: a gradient tile whose glow breathes slowly.
fn logo() -> Node {
    let time = styles::animation_time();
    center()
        .style(mue_macroquad::style! {
            flex: none;
            width: {u(30.)}; height: {u(30.)};
            border-radius: {u(9.)};
        })
        .background(accent_gradient())
        .box_shadow(time.map(|time| {
            let pulse = 0.5 + 0.5 * (time * 1.6).sin();
            BoxShadows::from(BoxShadow::new(
                0.,
                2.,
                10. + 8. * pulse,
                Color::new(ACCENT_A.r, ACCENT_A.g, ACCENT_A.b, 0.35 + 0.25 * pulse),
            ))
        }))
        .children(text("m").style(mue_macroquad::style! {
            font-size: {f(17.)}; line-height: {u(20.)};
            font-weight: 800;
            color: #ffffff;
        }))
        .into_node()
}

/// A small rounded readout in the top bar.
fn pill(label: impl Into<Prop<Rc<str>>>) -> FlexboxBuilder {
    flexbox()
        .style(mue_macroquad::style! {
            flex: none;
            padding: {(u(8.), u(2.))};
            border-radius: 9999px;
            border-width: 1px;
            border-color: {border()};
            background: {hover_tint()};
        })
        .children(text(label).style(mue_macroquad::style! {
            font-size: {f(12.)}; line-height: {u(18.)};
            font-family: monospace;
            color: {muted()};
            text-wrap: {false};
        }))
}

fn top_bar(selected: Signal<usize>, compact: ReadSignal<bool>) -> Node {
    let zoom = responsive::zoom();
    let color = use_color_mode();
    let wide = compact.map(|compact| !compact);
    hstack()
        .style(mue_macroquad::style! {
            width: 100%;
            flex: none;
            min-height: {u(56.)};
            padding: {(u(16.), u(8.))};
            gap: {u(10.)};
            align-items: center;
        })
        .children((
            logo(),
            hstack()
                .style(mue_macroquad::style! { flex: none; gap: {u(6.)}; align-items: baseline; })
                .children((
                    text("mue").style(mue_macroquad::style! {
                        font-size: {f(17.)};
                        line-height: {u(22.)};
                        font-weight: 800;
                        color: {foreground()};
                    }),
                    text("playground")
                        .style(mue_macroquad::style! {
                            font-size: {f(15.)};
                            line-height: {u(22.)};
                            color: {muted()};
                        })
                        .show_if(wide),
                )),
            pill(selected.map(|index| Rc::<str>::from(format!("{} / {}", index + 1, PAGES.len()))))
                .show_if(wide),
            flexbox().style(mue_macroquad::style! { flex: 1; min-width: 0px; }),
            pill(fps().map(|fps| Rc::<str>::from(format!("{fps} fps")))),
            // Narrow windows navigate with the tabs and back.
            hstack()
                .style(mue_macroquad::style! { flex: none; gap: {u(2.)}; align-items: center; })
                .children((
                    tooltip(
                        "Previous page",
                        icon_button(text("‹"))
                            .size(ControlSize::Sm)
                            .on_activate(move || selected.update(|index| *index = index.saturating_sub(1))),
                    ),
                    tooltip(
                        "Next page",
                        icon_button(text("›"))
                            .size(ControlSize::Sm)
                            .on_activate(move || {
                                selected.update(|index| *index = (*index + 1).min(PAGES.len() - 1))
                            }),
                    ),
                ))
                .show_if(wide),
            tooltip(
                "Zoom: design units and rem scale with it",
                button(zoom.map(|zoom| Rc::<str>::from(format!("{:.0}%", zoom * 100.))))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .on_activate(responsive::cycle_zoom),
            ),
            button(color.scheme.map(|scheme| {
                Rc::<str>::from(match scheme {
                    ColorScheme::Dark => "Dark",
                    ColorScheme::Light => "Light",
                })
            }))
            .variant(ButtonVariant::Outline)
            .size(ControlSize::Sm)
            .on_activate(move || color.toggle()),
        ))
        .into_node()
}

/// The animated background behind everything: a gradient, two soft color
/// glows, and Phira's moving blinds.
fn backdrop() -> Node {
    let glow_box = |which: &str| {
        flexbox()
            .absolute()
            .width(Size::Vh(90.))
            .height(Size::Vh(90.))
            .background(glow_brush(which))
    };
    stack()
        .style(mue_macroquad::style! {
            position: absolute;
            left: 0px; top: 0px; width: 100%; height: 100%;
            overflow: hidden;
            background: {backdrop_brush()};
        })
        .children((
            glow_box("a").inset_left(Size::Vh(-30.)).inset_top(Size::Vh(-35.)),
            glow_box("b").inset_right(Size::Vh(-30.)).inset_bottom(Size::Vh(-40.)),
            stripes()
                .angle(0.66)
                .period(56.)
                .duty(0.5)
                .speed(10.)
                .color(stripe_color())
                .style(mue_macroquad::style! {
                    position: absolute;
                    left: 0px; top: 0px; width: 100%; height: 100%;
                }),
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

/// The navigation chrome around the pages: the animated backdrop, a
/// frosted header (top bar, plus tabs on narrow windows) that the pages
/// scroll under, and a frosted sidebar on wide windows.
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
    let header = NodeHandle::new();
    let header_height = computed(move |_| {
        header
            .get()
            .and_then(|node| node.rect())
            .map_or(0., |rect| rect.get().h)
    });
    let page_view = pages(
        selected,
        (0..PAGES.len()).map(|index| destination(index, selected, header_height)),
    )
    .page_transition(PagesTransition::Slide)
    .transition_curve(SPRING)
    .into_node();
    // Back steps to the previous page before the app closes.
    mue_macroquad::input::back::use_back_handler(selected.map(|index| index > 0), move || {
        selected.update(|index| *index = index.saturating_sub(1))
    });
    stack()
        .style(mue_macroquad::style! {
            position: relative;
            width: 100%; height: 100%;
            gap: 0px;
            background: {background()};
        })
        .children((
            backdrop(),
            hstack()
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: 100%;
                    min-width: 0px;
                    min-height: 0px;
                    gap: 0px;
                    align-items: stretch;
                })
                .children((
                    sidebar(selected, header_height).show_if(compact.map(|compact| !compact)),
                    flexbox()
                        .style(mue_macroquad::style! {
                            flex: 1;
                            min-width: 0px;
                            min-height: 0px;
                            height: 100%;
                        })
                        .children(page_view),
                )),
            stack()
                .node_handle(header)
                .style(mue_macroquad::style! {
                    position: absolute;
                    left: 0px; top: 0px;
                    width: 100%;
                    gap: 0px;
                    background: {glass()};
                    backdrop-filter: blur(20px);
                    border-bottom-width: 1px;
                    border-color: {border()};
                })
                .children((top_bar(selected, compact), tab_strip(selected).show_if(compact))),
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
