# mue

基于 [macroquad](https://macroquad.rs) 的响应式声明式 UI / 组件库，用 Rust 编写。
布局交给 [taffy](https://github.com/DioxusLabs/taffy)，文本由 [cosmic-text](https://github.com/pop-os/cosmic-text) 排版。

## 特性

- **细粒度响应式**：`signal` / `computed` / `watch_effect`，配合 `scope` 自动追踪与释放依赖。
- **声明式节点树**：`flexbox` / `text` / `image` / `circle`，所有样式属性都可以接收响应式 `Prop`。
- **组件宏**：`#[mue_macros::component]` 把普通函数变成一个可组合的节点，参数自动支持字面量或 signal。
- **事件与手势**：点击、hover、滚轮、长按，以及基于 taffy 布局的命中测试。
- **自定义字体**：按字面（family / weight）注册 `ttf` / `otf`，字重本身也能是响应式的。

## 工作区结构

| crate | 说明 |
| --- | --- |
| `mue-core` | 与渲染无关的响应式内核：signal、effect、scope、prop。可通过 feature 接入 taffy / cosmic-text。 |
| `mue-macros` | `#[component]` / `#[node]` 属性过程宏。 |
| `mue-macroquad` | 渲染与运行时：节点、样式、布局、事件、手势、字体、shader。核心导出 `App`。 |
| `examples/basics` | 响应式样式、图片、字体（Space Mono 400/700）示例。 |
| `examples/events` | 点击 / 长按 / hover / 滚轮与 `set_timeout` 示例。 |
| `examples/pages` | 页面切换与切换动画。 |
| `examples/tab` | 固定底部 Tab 栏，页面切换与切换动画。 |

## 快速开始

需要 Rust 1.89+。

```bash
cargo run -p basics
cargo run -p events
cargo run -p pages
cargo run -p tab
```

在 `macroquad` 的窗口回调里构建并驱动一棵节点树：

```rust
use mue_core::prelude::*;
use mue_macroquad::{node::*, style::{Styleable, StyleableExt}, App};

#[mue_macros::component]
fn counter() -> impl Component {
    let count = signal(0);

    text(computed(move |_| format!("clicked {} times", count.get()).into()))
        .on_tap(move |_| count.update(|c| *c += 1))
}

async fn the_main() {
    let mut app = App::new(counter());

    loop {
        clear_background(macroquad::prelude::BLACK);
        app.frame(); // 布局、更新、命中测试、绘制都在这里发生
        macroquad::prelude::next_frame().await;
    }
}
```

## 备注

- `macroquad` / `miniquad` 目前指向 fork 的 git 源，见根 `Cargo.toml` 的 `[workspace.dependencies]`。
- 示例字体使用 vendored 的 Space Mono（SIL OFL 1.1，见 `examples/basics/assets/fonts/OFL-SpaceMono.txt`）。
- 可变字体在 `fontdb` 中只按默认实例登记字重；演示非默认字重时请使用按字重拆分的静态字体文件。

## License

MIT，见 [LICENSE](LICENSE)。
