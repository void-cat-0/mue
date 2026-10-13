//! cargo run -p navigation
//! cargo run -p navigation -- --stack

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::Animation,
    color::Color,
    node::*,
    style::{Styleable, StyleableExt},
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
        .padding((14., 10.))
        .margin_bottom(10.)
        .background(BLUE)
        .radius(8.)
        .on_activate(action)
        .children(text(label).font_size(18.))
}

fn back(path: Signal<Vec<Route>>) {
    path.update(|routes| {
        routes.pop();
    });
}

fn library(path: Signal<Vec<Route>>) -> impl Component {
    stack()
        .size_full()
        .padding((24.,))
        .background(SURFACE)
        .children((
            heading("Field notes").font_size(30.).margin_bottom(24.),
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
        ))
}

fn destination(path: Signal<Vec<Route>>, route: Route) -> impl Component {
    let count = signal(0);
    let place = match route {
        Route::Place(place) | Route::Note { place, .. } => place,
    };
    stack()
        .size_full()
        .padding((24.,))
        .background(CANVAS)
        .children((
            flexbox().children((
                button("Back", move || back(path)).margin_right(12.),
                button("Library", move || path.set(Vec::new())),
            )),
            heading(route.title())
                .font_size(32.)
                .margin_top(16.)
                .margin_bottom(20.),
            text(count.map(|n| format!("Count: {n}")))
                .font_size(24.)
                .margin_bottom(14.),
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
        ))
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
    let path = signal(Vec::<Route>::new());
    let root = move || library(path);
    let screen = move |route: &Route| destination(path, *route);
    let mut app = App::new(
        stack().size_full().children((
            flexbox()
                .w_full()
                .flex_none()
                .padding((24., 16.))
                .background(SURFACE)
                .children(
                    text(path.map(|routes| {
                        std::iter::once("Library".to_string())
                            .chain(routes.into_iter().map(Route::title))
                            .collect::<Vec<_>>()
                            .join(" > ")
                    }))
                    .font_size(18.)
                    .min_width(0.)
                    .flex_1()
                    .text_wrap(false)
                    .text_max_lines(Some(1))
                    .text_overflow(TextOverflow::EllipsisMiddle),
                ),
            navigation_stack(path, root, screen)
                .transition_animation(TRANSITION)
                .split_view(
                    280.,
                    if std::env::args().any(|arg| arg == "--stack") {
                        10000.
                    } else {
                        720.
                    },
                )
                .height(taffy::Dimension::auto())
                .flex_1()
                .min_height(0.),
        )),
    );

    loop {
        if is_key_pressed(KeyCode::Escape) {
            back(path);
        }
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
