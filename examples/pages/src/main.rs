//! 页面切换 + 切换动画示例。
//!
//! 运行：cargo run -p pages
//!
//! 核心思路：
//!   * 切换 = 改「页面栈」信号，children 是响应式的，节点树自己跟着变；
//!   * 动画 = 自己按帧推进一个 progress 信号，页面容器上的 opacity / transform
//!     都是响应式 Prop，会跟着变；这两个属性会沿子树继承（相乘），
//!     所以只需要写在页面容器上。
//!   * 过渡期间旧页面仍然留在页面栈里，结束时再移除，于是节点被 dispose。

use std::rc::Rc;

use macroquad::prelude::*;
use mue_core::{prelude::*, prop::PropValue};
use mue_macroquad::{
    App,
    layout::use_layout,
    math::{Matrix, Vector},
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

/// macroquad 的 Color 没有实现 PropValue（孤儿规则，这里也补不了），
/// 用一个本地 newtype 包一层，才能作为 Prop 的值。
#[derive(Clone, Copy, PartialEq, Debug)]
struct Fill(Color);

impl PropValue for Fill {}

/// 会填充背景色的容器节点：布局交给 taffy，绘制走 use_paint。
/// paint 会自动继承祖先的 transform / opacity，所以动画写在页面上就够了。
#[mue_macros::node]
fn panel(style: &mut Style, color: Fill) {
    let rect = use_layout(style).rect;
    let paint = use_paint(style);
    let shapes = paint.build(move |p| {
        let r = rect.get();
        p.fill_rect(r, color.get().0);
    });
    style.on_render.append(move |_| shapes.get_clone().draw());
}

/// 切换页面：记录旧页面、重置进度。进度没走完就忽略新的点击。
fn navigate(
    current: Signal<Page>,
    previous: Signal<Option<Page>>,
    progress: Signal<f32>,
    target: Page,
) {
    if current.get_untracked() == target || progress.get_untracked() < 1.0 {
        return;
    }
    previous.set(Some(current.get_untracked()));
    current.set(target);
    progress.set(0.0);
}

/// 单个页面：整页绝对定位铺满容器，根据自己在过场中的角色计算透明度和位移。
fn page_node(
    page: Page,
    current: Signal<Page>,
    previous: Signal<Option<Page>>,
    progress: Signal<f32>,
) -> Node {
    // 正在离场时淡出，正在进场时淡入，其余情况完全不透明。
    let alpha = computed(move |_| {
        let t = progress.get();
        if previous.get() == Some(page) {
            1.0 - t
        } else if current.get() == page {
            t
        } else {
            1.0
        }
    });

    // 旧页向左滑出、新页从右边滑入（transform 只影响绘制，不影响命中区域）。
    let offset = computed(move |_| {
        let t = progress.get();
        let dx = if previous.get() == Some(page) {
            -screen_width() * t
        } else if current.get() == page {
            screen_width() * (1.0 - t)
        } else {
            0.0
        };
        Matrix::from_translation(Vector::new(dx, 0.0))
    });

    let buttons = Page::ALL.map(|target| {
        let active = target == page;
        panel(Fill(if active {
            LIGHTGRAY
        } else {
            Color::from_rgba(0, 0, 0, 60)
        }))
        .padding((LengthPercentage::length(12.),))
        .margin((LengthPercentageAuto::length(8.),))
        .on_tap(move |_| navigate(current, previous, progress, target))
        .children(text(target.title()).font_size(22.))
        .into_node()
    });

    panel(Fill(page.color()))
        .absolute()
        .inset((LengthPercentageAuto::length(0.),))
        .size_full()
        .flex_column()
        .items_center()
        .justify_center()
        .opacity(alpha)
        .transform(offset)
        .children((
            text(page.title()).font_size(56.),
            text("tap a button to switch").font_size(18.),
            flexbox()
                .flex_row()
                .margin((LengthPercentageAuto::length(24.),))
                .children(buttons),
        ))
        .into_node()
}

#[mue_macros::component]
// #[model] 参数写成「值类型」，宏会把它变成 Signal<T>；
// 这里由外面创建信号后传进来，所以页面的过场状态在帧循环里也能读。
fn pages(
    #[model] current: Page,
    #[model] previous: Option<Page>,
    #[model] progress: f32,
) -> impl Component {
    // 过渡期间把旧页面也留在「页面栈」里，于是它还能参与布局和绘制；
    // 过渡结束后移除 -> 节点被 dispose，下次进入是全新状态。
    // 新页面排在最后：库没有 z-index，children 顺序就是绘制层级。
    let stack = computed(move |_| {
        let current = current.get();
        match previous.get() {
            Some(prev) if prev != current => Rc::<[Page]>::from([prev, current]),
            _ => Rc::<[Page]>::from([current]),
        }
    });

    // map_keyed 按 key 复用节点，列表顺序变化时复用而不是重建。
    let stack = map_keyed(
        stack,
        |page| *page,
        move |page| page_node(*page, current, previous, progress),
    );

    flexbox().size_full().children(stack)
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// 背景色也跟着过场：旧页颜色淡出、新页颜色淡入。
fn background(
    current: Signal<Page>,
    previous: Signal<Option<Page>>,
    progress: Signal<f32>,
) -> Color {
    let t = progress.get_untracked();
    match previous.get_untracked() {
        Some(prev) => lerp_color(prev.color(), current.get_untracked().color(), t),
        None => current.get_untracked().color(),
    }
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
    let current = signal(Page::Home);
    let previous = signal(None::<Page>);
    let progress = signal(1.0f32);

    let mut app = App::new(pages(current, previous, progress));

    loop {
        clear_background(background(current, previous, progress));

        // 库没有内置 transition：每帧手动推进进度，信号会带动页面上的 Prop。
        if progress.get_untracked() < 1.0 {
            progress.update(|p| (*p + get_frame_time() / TRANSITION).min(1.0));
            if progress.get_untracked() >= 1.0 {
                // 收尾：把旧页面从页面栈里移除
                previous.set(None);
            }
        }

        app.frame();
        next_frame().await;
    }
}
