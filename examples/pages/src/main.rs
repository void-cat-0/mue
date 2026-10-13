//! 页面切换示例。运行：cargo run -p pages
//! 过渡暂时在示例内实现，由主循环推进信号，使用布局位移移动整个页面。

#[path = "../../palette.rs"]
mod palette;

use std::rc::Rc;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt},
};
use taffy::LengthPercentageAuto;

/// 过场时长（秒）
const TRANSITION: f32 = 0.28;

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
            Page::Home => palette::BLUE,
            Page::About => palette::GREEN,
            Page::Settings => palette::PLUM,
        }
    }
}

struct PageTransition {
    stack: Signal<Rc<[Page]>>,
    offsets: Signal<[f32; 3]>,
    target: Page,
    from: [f32; 3],
    to: [f32; 3],
    elapsed: f32,
}

impl PageTransition {
    fn new(page: Page) -> Self {
        let mut offsets = [1.0; 3];
        offsets[page as usize] = 0.0;
        Self {
            stack: signal(Rc::from([page])),
            offsets: signal(offsets),
            target: page,
            from: offsets,
            to: offsets,
            elapsed: TRANSITION,
        }
    }

    fn advance(&mut self, target: Page, dt: f32) {
        batch(|| {
            if target != self.target {
                self.from = self.offsets.get_untracked();
                let mut stack = self.stack.get_clone_untracked().to_vec();
                if !stack.contains(&target) {
                    self.from[target as usize] = if (target as usize) > (self.target as usize) {
                        1.0
                    } else {
                        -1.0
                    };
                    stack.push(target);
                }
                let destination = self.from[target as usize];
                for page in Page::ALL {
                    let i = page as usize;
                    self.to[i] = if page == target {
                        0.0
                    } else if self.from[i] < destination {
                        -1.0
                    } else {
                        1.0
                    };
                }
                self.target = target;
                self.elapsed = 0.0;
                self.stack.set(stack.into());
            }
            if self.elapsed >= TRANSITION {
                return;
            }
            self.elapsed = (self.elapsed + dt.max(0.0)).min(TRANSITION);
            let t = self.elapsed / TRANSITION;
            let eased = t * t * (3.0 - 2.0 * t);
            self.offsets.set(std::array::from_fn(|i| {
                self.from[i] + (self.to[i] - self.from[i]) * eased
            }));
            if self.elapsed >= TRANSITION {
                self.stack.set(Rc::from([self.target]));
            }
        });
    }
}

fn page_node(page: Page, target: Signal<Page>, offsets: ReadSignal<[f32; 3]>) -> Node {
    let buttons = Page::ALL.map(|destination| {
        let hovered = signal(false);
        let pressed = signal(false);
        flexbox()
            .background(computed(move |_| {
                Brush::Solid(if pressed.get() {
                    palette::PRESSED
                } else {
                    match (destination == page, hovered.get()) {
                        (true, true) => palette::HIGHLIGHT,
                        (true, false) => palette::ACTIVE,
                        (false, true) => palette::HOVER,
                        (false, false) => palette::SURFACE,
                    }
                })
            }))
            .padding((12.,))
            .margin((8.,))
            .use_hovered(hovered)
            .use_pressed(pressed)
            .on_tap(move |_| target.set(destination))
            .children(text(destination.title()).font_size(22.))
            .into_node()
    });

    flexbox()
        .background(page.color())
        .absolute()
        .inset_top(0.)
        .inset_left(computed(move |_| {
            LengthPercentageAuto::percent(offsets.get()[page as usize])
        }))
        .size_full()
        .flex_column()
        .items_center()
        .justify_center()
        .children((
            text(page.title()).font_size(56.),
            text("Tap a button to switch pages").font_size(18.),
            flexbox()
                .flex_row()
                .justify_center()
                .margin((24.,))
                .children(buttons),
        ))
        .into_node()
}

#[mue_macros::component]
fn pages(
    #[model] target: Page,
    #[model] stack: Rc<[Page]>,
    #[model] offsets: [f32; 3],
) -> impl Component {
    let children = map_keyed(
        *stack,
        |page| *page,
        move |page| page_node(*page, target, *offsets),
    );
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
        async {
            run().await;
        },
    );
}

async fn run() {
    let target = signal(Page::Home);
    let mut transition = PageTransition::new(Page::Home);
    let mut app = App::new(pages(target, transition.stack, transition.offsets));

    loop {
        clear_background(palette::CANVAS.into());
        transition.advance(target.get_untracked(), get_frame_time());
        app.frame();
        next_frame().await;
    }
}
