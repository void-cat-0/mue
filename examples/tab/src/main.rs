//! 固定底部 Tab 栏示例。运行：cargo run -p tab
//! State-driven page transitions, advanced by App::frame.
use mue_components::*;

#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    shader::Brush,
    style::{Styleable, StyleableExt, percent, vh, vw},
    transition::Transition,
};

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
    flexbox()
        .style(mue_macroquad::style! {
        background: {page.color()};
        transition: {Transition::ease_in_out(TRANSITION)};
        })
        .absolute()
        .style(mue_macroquad::style! {
        top: 0px;
        left: {computed(move |_| {
                    percent(100. * (page as usize as f32 - target.get() as usize as f32))
                })};
        width: 100%; height: 100%;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: {(vw(4.),)};
        })
        .children((text(page.title()).style(mue_macroquad::style! {
        font-size: {responsive::font(56.)};
        line-height: {responsive::ui(72.)};
        max-width: 100%;
        }),))
        .into_node()
}

#[mue_macros::component]
fn pages(#[model] target: Page) -> impl Component {
    let buttons = Page::ALL.map(|destination| {
        let hovered = signal(false);
        let pressed = signal(false);
        flexbox()
            .style(mue_macroquad::style! {
            background: {computed(move |_| {
                            Brush::Solid(if pressed.get() {
                                PRESSED
                            } else {
                                match (destination == target.get(), hovered.get()) {
                                    (true, true) => HIGHLIGHT,
                                    (true, false) => ACTIVE,
                                    (false, true) => HOVER,
                                    (false, false) => SURFACE,
                                }
                            })
                        })};
            padding: {(responsive::ui(12.), responsive::ui(12.))};
            min-height: {responsive::ui(44.)};
            min-width: 0px;
            max-width: 100%;
            transition: {Transition::ease_in_out(TRANSITION)};
            justify-content: center;
            })
            .use_hovered(hovered)
            .use_pressed(pressed)
            .on_activate(move || target.set(destination))
            .children(text(destination.title()).style(mue_macroquad::style! {
            font-size: {responsive::font(22.)};
            line-height: {responsive::ui(32.)};
            min-width: 0px;
            }))
            .into_node()
    });

    let children = Page::ALL.map(|page| page_node(page, target));
    flexbox()
        .style(mue_macroquad::style! {
        width: 100%; height: 100%;
        font-size: {responsive::root_font()};
        flex-direction: column;
        })
        .children((
            flexbox()
                .relative()
                .style(mue_macroquad::style! {
                width: 100%;
                flex: 1;
                min-height: 0px;
                overflow-x: {taffy::Overflow::Hidden};
                overflow-y: {taffy::Overflow::Hidden};
                })
                .children(children),
            wrap()
                .style(mue_macroquad::style! {
                width: 100%;
                justify-content: center;
                flex: none;
                padding: {(vw(2.), vh(2.5))};
                column-gap: {vw(2.)};
                row-gap: {vh(1.)};
                })
                .children(buttons),
        ))
}

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue - tab".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();
    let target = signal(Page::Home);
    let mut app = App::new(pages(target));

    loop {
        responsive::sync();
        clear_background(CANVAS.into());
        app.frame();
        next_frame().await;
    }
}
