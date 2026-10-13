//! 页面切换示例。运行：cargo run -p pages
//! State-driven page transitions, advanced by App::frame.

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    animation::{Animation, with_animation},
    color::Color,
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt},
};
use taffy::LengthPercentageAuto;

const TRANSITION: f32 = 0.28;

const ACTIVE: Color = Color::from_rgba(70, 95, 130, 255);
const BLUE: Color = Color::from_rgba(37, 58, 94, 255);
const CANVAS: Color = Color::BLACK;
const GREEN: Color = Color::from_rgba(46, 82, 63, 255);
const HIGHLIGHT: Color = Color::from_rgba(100, 135, 180, 255);
const HOVER: Color = Color::from_rgba(55, 65, 85, 255);
const PLUM: Color = Color::from_rgba(88, 52, 84, 255);
const PRESSED: Color = Color::from_rgba(15, 20, 30, 255);
const SURFACE: Color = Color::from_rgba(25, 30, 40, 255);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Page {
    Home,
    About,
    Settings,
}

impl Page {
    const ALL: [Page; 3] = [Page::Home, Page::About, Page::Settings];

    fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::About => "About",
            Page::Settings => "Settings",
        }
    }

    fn color(self) -> Color {
        match self {
            Page::Home => BLUE,
            Page::About => GREEN,
            Page::Settings => PLUM,
        }
    }
}

fn page_node(page: Page, target: Signal<Page>) -> Node {
    let buttons = Page::ALL.map(|destination| {
        let hovered = signal(false);
        let pressed = signal(false);
        flexbox()
            .background(computed(move |_| {
                Brush::Solid(if pressed.get() {
                    PRESSED
                } else {
                    match (destination == page, hovered.get()) {
                        (true, true) => HIGHLIGHT,
                        (true, false) => ACTIVE,
                        (false, true) => HOVER,
                        (false, false) => SURFACE,
                    }
                })
            }))
            .padding((12.,))
            .margin((8.,))
            .use_hovered(hovered)
            .use_pressed(pressed)
            .on_activate(move || {
                with_animation(Animation::ease_in_out(TRANSITION), || {
                    target.set(destination)
                })
            })
            .children(text(destination.title()).font_size(22.))
            .into_node()
    });

    flexbox()
        .background(page.color())
        .absolute()
        .inset_top(0.)
        .inset_left(computed(move |_| {
            LengthPercentageAuto::percent(page as usize as f32 - target.get() as usize as f32)
        }))
        .size_full()
        .flex_column()
        .items_center()
        .justify_center()
        .children((
            text(page.title()).font_size(56.),
            flexbox()
                .flex_row()
                .justify_center()
                .margin((24.,))
                .children(buttons),
        ))
        .into_node()
}

#[mue_macros::component]
fn pages(#[model] target: Page) -> impl Component {
    let children = Page::ALL.map(|page| page_node(page, target));
    flexbox().size_full().children(children)
}

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - pages".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let target = signal(Page::Home);
    let mut app = App::new(pages(target));

    loop {
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
