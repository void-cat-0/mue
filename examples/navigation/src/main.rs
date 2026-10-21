//! cargo run -p navigation
//! cargo run -p navigation -- --stack
use mue_components::*;

#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    style::{Styleable, StyleableExt, percent, vh, vw},
    transition::Transition,
};

const CANVAS: Color = Color::from_rgba(19, 25, 34, 255);
const SURFACE: Color = Color::from_rgba(29, 39, 52, 255);
const BLUE: Color = Color::from_rgba(48, 79, 116, 255);
const TRANSITION: Transition = Transition::spring(0.5, 0.2);

#[derive(Clone, Copy, PartialEq)]
enum Route {
    Place(&'static str),
    Note { place: &'static str, index: usize },
}

impl Route {
    fn title(self) -> String {
        match self {
            Self::Place(place) => place.into(),
            Self::Note { index, .. } => format!("Note #{index}"),
        }
    }
}

fn button(label: &'static str, action: impl FnMut() + 'static) -> impl Component {
    flexbox()
        .style(mue_macroquad::style! {
        padding: {(responsive::ui(14.), responsive::ui(10.))};
        min-height: {responsive::ui(44.)};
        min-width: 0px;
        max-width: 100%;
        flex-shrink: 0;
        margin-bottom: {vh(1.5)};
        background: {BLUE};
        border-radius: {responsive::ui(8.)};
        })
        .on_activate(action)
        .children(text(label).style(mue_macroquad::style! {
        font-size: {responsive::font(18.)};
        line-height: {responsive::ui(28.)};
        min-width: 0px;
        }))
}

fn back(path: Signal<Vec<Route>>) {
    path.update(|routes| {
        routes.pop();
    });
}

fn library(path: Signal<Vec<Route>>) -> impl Component {
    scroll_area()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        background: {SURFACE};
        })
        .children(
            stack()
                .style(mue_macroquad::style! {
                width: 100%;
                padding: {(percent(5.), vh(4.))};
                })
                .children((
                    heading("Field notes").style(mue_macroquad::style! {
                    font-size: {responsive::font(30.)};
                    line-height: {responsive::ui(40.)};
                    margin-bottom: {vh(4.)};
                    }),
                    ["Coast", "Forest", "Mountains"].map(|place| {
                        button(place, move || path.set(vec![Route::Place(place)])).into_node()
                    }),
                    button("Open Forest notes", move || {
                        path.set(vec![
                            Route::Place("Forest"),
                            Route::Note {
                                place: "Forest",
                                index: 1,
                            },
                        ]);
                    }),
                )),
        )
}

fn destination(path: Signal<Vec<Route>>, route: Route) -> impl Component {
    let count = signal(0);
    let place = match route {
        Route::Place(place) | Route::Note { place, .. } => place,
    };
    scroll_area()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        background: {CANVAS};
        })
        .children(
            stack()
                .style(mue_macroquad::style! {
                width: 100%;
                padding: {(percent(5.), vh(4.))};
                })
                .children((
                    wrap()
                        .style(mue_macroquad::style! { column-gap: 3%; })
                        .children((
                            button("Back", move || back(path)),
                            button("Library", move || path.set(Vec::new())),
                        )),
                    heading(route.title()).style(mue_macroquad::style! {
                    font-size: {responsive::font(32.)};
                    line-height: {responsive::ui(44.)};
                    margin-top: {vh(2.5)};
                    margin-bottom: {vh(3.)};
                    }),
                    text(count.map(|n| format!("Count: {n}"))).style(mue_macroquad::style! {
                    font-size: {responsive::font(24.)};
                    line-height: {responsive::ui(34.)};
                    margin-bottom: {vh(2.)};
                    }),
                    button("Increment", move || count.update(|n| *n += 1)),
                    button("New note", move || {
                        path.update(|routes| {
                            let index = routes
                                .iter()
                                .filter_map(|route| match route {
                                    Route::Note { index, .. } => Some(*index),
                                    _ => None,
                                })
                                .max()
                                .unwrap_or(0)
                                + 1;
                            routes.push(Route::Note { place, index });
                        });
                    }),
                    button("Replace with Coast", move || {
                        path.update(|routes| {
                            if let Some(top) = routes.last_mut() {
                                *top = Route::Place("Coast");
                            }
                        });
                    }),
                )),
        )
}

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - navigation".into(),
            window_width: 960,
            window_height: 640,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();
    let path = signal(Vec::<Route>::new());
    let root = move || library(path);
    let screen = move |route: &Route| destination(path, *route);
    let mut app = App::new(
        stack()
            .style(mue_macroquad::style! {
            width: 100%; height: 100%;
            font-size: {responsive::root_font()};
            })
            .children((
                flexbox()
                    .style(mue_macroquad::style! {
                    width: 100%;
                    flex: none;
                    padding: {(vw(2.5), vh(2.5))};
                    background: {SURFACE};
                    })
                    .children(
                        text(path.map(|routes| {
                            std::iter::once("Library".to_string())
                                .chain(routes.into_iter().map(Route::title))
                                .collect::<Vec<_>>()
                                .join(" > ")
                        }))
                        .style(mue_macroquad::style! {
                        font-size: {responsive::font(18.)};
                        line-height: {responsive::ui(28.)};
                        min-width: 0px;
                        flex: 1;
                        text-wrap: {false};
                        text-max-lines: {Some(1)};
                        })
                        .text_overflow(TextOverflow::EllipsisMiddle),
                    ),
                navigation_stack(path, root, screen)
                    .transition_curve(TRANSITION)
                    .split_view(
                        280. * responsive::scale(),
                        // The split-view API takes fixed lengths; keep retained screens intact.
                        (if std::env::args().any(|arg| arg == "--stack") {
                            10000.
                        } else {
                            720.
                        }) * responsive::scale(),
                    )
                    .style(mue_macroquad::style! {
                    height: {taffy::Dimension::auto()};
                    flex: 1;
                    min-height: 0px;
                    }),
            )),
    );

    loop {
        responsive::sync();
        if is_key_pressed(KeyCode::Escape) {
            back(path);
        }
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
