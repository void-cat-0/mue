//! cargo run -p navigation
//! cargo run -p navigation -- --stack

#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::Animation,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt, percent, vh, vw},
};

const CANVAS: Color = Color::from_rgba(19, 25, 34, 255);
const SURFACE: Color = Color::from_rgba(29, 39, 52, 255);
const BLUE: Color = Color::from_rgba(48, 79, 116, 255);
const TRANSITION: Animation = Animation::spring(0.5, 0.2);

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
        .padding((responsive::ui(14.), responsive::ui(10.)))
        .min_height(responsive::ui(44.))
        .min_width(0.)
        .max_width(percent(100.))
        .flex_shrink(0.)
        .margin_bottom(vh(1.5))
        .background(BLUE)
        .radius(responsive::ui(8.))
        .on_activate(action)
        .children(
            text(label)
                .font_size(responsive::font(18.))
                .line_height(responsive::ui(28.))
                .min_width(0.),
        )
}

fn back(path: Signal<Vec<Route>>) {
    path.update(|routes| {
        routes.pop();
    });
}

fn library(path: Signal<Vec<Route>>) -> impl Component {
    scroll_area().size_full().background(SURFACE).children(
        stack().w_full().padding((percent(5.), vh(4.))).children((
            heading("Field notes")
                .font_size(responsive::font(30.))
                .line_height(responsive::ui(40.))
                .margin_bottom(vh(4.)),
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
    scroll_area().size_full().background(CANVAS).children(
        stack().w_full().padding((percent(5.), vh(4.))).children((
            wrap().column_gap(percent(3.)).children((
                button("Back", move || back(path)),
                button("Library", move || path.set(Vec::new())),
            )),
            heading(route.title())
                .font_size(responsive::font(32.))
                .line_height(responsive::ui(44.))
                .margin_top(vh(2.5))
                .margin_bottom(vh(3.)),
            text(count.map(|n| format!("Count: {n}")))
                .font_size(responsive::font(24.))
                .line_height(responsive::ui(34.))
                .margin_bottom(vh(2.)),
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
            .size_full()
            .font_size(responsive::root_font())
            .children((
                flexbox()
                    .w_full()
                    .flex_none()
                    .padding((vw(2.5), vh(2.5)))
                    .background(SURFACE)
                    .children(
                        text(path.map(|routes| {
                            std::iter::once("Library".to_string())
                                .chain(routes.into_iter().map(Route::title))
                                .collect::<Vec<_>>()
                                .join(" > ")
                        }))
                        .font_size(responsive::font(18.))
                        .line_height(responsive::ui(28.))
                        .min_width(0.)
                        .flex_1()
                        .text_wrap(false)
                        .text_max_lines(Some(1))
                        .text_overflow(TextOverflow::EllipsisMiddle),
                    ),
                navigation_stack(path, root, screen)
                    .transition_animation(TRANSITION)
                    .split_view(
                        280. * responsive::scale(),
                        // The split-view API takes fixed lengths; keep retained screens intact.
                        (if std::env::args().any(|arg| arg == "--stack") {
                            10000.
                        } else {
                            720.
                        }) * responsive::scale(),
                    )
                    .height(taffy::Dimension::auto())
                    .flex_1()
                    .min_height(0.),
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
