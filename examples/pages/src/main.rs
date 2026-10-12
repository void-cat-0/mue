//! 页面切换示例。运行：cargo run -p pages
//! 过渡暂时在示例内实现，由主循环推进信号，使用布局位移移动整个页面。

use std::rc::Rc;

use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    layout::use_layout,
    node::*,
    paint::use_paint,
    style::{Style, Styleable, StyleableExt},
};
use taffy::{LengthPercentage, LengthPercentageAuto};

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
            Page::Home => Color::from_rgba(37, 58, 94, 255),
            Page::About => Color::from_rgba(46, 82, 63, 255),
            Page::Settings => Color::from_rgba(88, 52, 84, 255),
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

#[mue_macros::node]
fn panel(style: &mut Style, color: Color) {
    let rect = use_layout(style).rect;
    let paint = use_paint(style);
    let shapes = paint.build(move |p| p.fill_rect(rect.get(), color.get()));
    style.on_render.append(move |_| shapes.get_clone().draw());
}

fn page_node(page: Page, target: Signal<Page>, offsets: ReadSignal<[f32; 3]>) -> Node {
    let buttons = Page::ALL.map(|destination| {
        let hovered = signal(false);
        let pressed = signal(false);
        panel(computed(move |_| {
            if pressed.get() {
                return Color::from_rgba(15, 20, 30, 255);
            }
            match (destination == page, hovered.get()) {
                (true, true) => Color::from_rgba(100, 135, 180, 255),
                (true, false) => Color::from_rgba(70, 95, 130, 255),
                (false, true) => Color::from_rgba(55, 65, 85, 255),
                (false, false) => Color::from_rgba(25, 30, 40, 255),
            }
        }))
        .padding((LengthPercentage::length(12.),))
        .margin((LengthPercentageAuto::length(8.),))
        .use_hovered(hovered)
        .use_pressed(pressed)
        .on_tap(move |_| target.set(destination))
        .children(text(destination.title()).font_size(22.))
        .into_node()
    });

    panel(mue_core::prop::Prop::Static(page.color()))
        .absolute()
        .inset_top(LengthPercentageAuto::length(0.))
        .inset_left(computed(move |_| {
            LengthPercentageAuto::percent(offsets.get()[page as usize])
        }))
        .size_full()
        .flex_column()
        .items_center()
        .justify_center()
        .children((
            text(page.title()).font_size(56.),
            text("tap a button to switch").font_size(18.),
            flexbox()
                .flex_row()
                .justify_center()
                .margin((LengthPercentageAuto::length(24.),))
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
            the_main().await;
        },
    );
}

async fn the_main() {
    let target = signal(Page::Home);
    let mut transition = PageTransition::new(Page::Home);
    let mut app = App::new(pages(target, transition.stack, transition.offsets));

    loop {
        clear_background(BLACK);
        transition.advance(target.get_untracked(), get_frame_time());
        app.frame();
        next_frame().await;
    }
}
