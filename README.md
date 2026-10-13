# mue

基于 [macroquad](https://macroquad.rs) 的响应式声明式 UI / 组件库，用 Rust 编写。
布局交给 [taffy](https://github.com/DioxusLabs/taffy)，文本由 [cosmic-text](https://github.com/pop-os/cosmic-text) 排版。

## 特性

- **细粒度响应式**：`signal` / `computed` / `watch_effect`，配合 `scope` 自动追踪与释放依赖。
- **声明式节点树**：`flexbox` / `text` / `image` / `circle`，所有样式属性都可以接收响应式 `Prop`。
- **组件宏**：`#[mue_macros::component]` 把普通函数变成一个可组合的节点，参数自动支持字面量或 signal。
- **事件与手势**：点击、hover、滚轮、长按、拖动、滑动、双指缩放与旋转，以及基于 taffy 布局的命中测试。
- **自定义字体**：按字面（family / weight）注册 `ttf` / `otf`，字重本身也能是响应式的。
- **Factory 与主题**：具名参数、实例级覆盖、按身份匹配的 `theme!` 分支、局部 Provider 和响应式换肤。
- **文档节点**：`document` / `markdown` 使用 cosmic-text 排版多段文本，支持富文本 span、标题、粗体、斜体、代码与列表。
- **状态动画**：`with_animation` 事务、节点 `.animation(...)`、线性/缓动/弹簧曲线，`App::frame` 自动推进，可在途中改变目标。

## 工作区结构

| crate | 说明 |
| --- | --- |
| `mue-core` | 与渲染无关的响应式内核：signal、effect、scope、prop。可通过 feature 接入 taffy / cosmic-text。 |
| `mue-macros` | `#[component]` / `#[node]` 属性过程宏。 |
| `mue-macroquad` | 渲染与运行时：节点、样式、布局、事件、手势、字体、shader。核心导出 `App`。 |
| `examples/basics` | 响应式样式、图片、字体（Space Mono 400/700）示例。 |
| `examples/events` | 简单的点击、长按、hover、滚轮与 `set_timeout` 示例。 |
| `examples/gestures` | 八个等大可视化面板：点击、长按、hover、滚轮、拖动、滑动、双指缩放与旋转。 |
| `examples/map` | 图片网格地图，支持拖动、惯性滑动、双指缩放与旋转。 |
| `examples/pages` | 页面切换与切换动画。 |
| `examples/tab` | 固定底部 Tab 栏，页面切换与切换动画。 |
| `examples/theme` | 练习计数、按钮变体、嵌套主题与明暗主题切换。 |
| `examples/document` | Markdown 阅读视图、字号与内容切换、富文本署名。 |
| `examples/animation` | 尺寸、圆角与颜色的状态动画，弹簧与缓动切换。 |

## 快速开始

需要 Rust 1.89+。

```bash
cargo run -p basics
cargo run -p events
cargo run -p gestures
cargo run -p map
cargo run -p pages
cargo run -p tab
cargo run -p theme
cargo run -p document
cargo run -p animation
```

`Factory` 复用组件定义；主题只覆盖选中的样式字段。`theme!` 克隆分支里的 Factory 句柄，原句柄仍可用来创建实例：

```rust
use mue_macroquad::{App, color::Color, factory::factory, node::text, style::Styleable, theme::{provide_theme, theme}};

let label = factory(|caption: String| text(caption));
let appearance = theme!(|c, s| match c {
    label => s.font_size(18.).color(Color::WHITE),
    _ => s,
});
let app = App::new(provide_theme(appearance, move || label.create("Hello")));
```

`provide_theme` 也接受 `Signal<Theme>` / `ReadSignal<Theme>`；替换信号值会重算样式，未设置的字段继续从外层主题和 Factory 默认样式回退。参见 `cargo run -p theme`。

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
        macroquad::prelude::clear_background(macroquad::prelude::BLACK);
        app.frame(); // 布局、更新、命中测试、绘制都在这里发生
        macroquad::prelude::next_frame().await;
    }
}
```

## 文档与 Markdown

`document("第一段\n\n第二段")` 显示纯文本；`markdown(source)` 将 CommonMark 转成 cosmic-text 的富文本 spans。两者都接受响应式属性，使用现有的字体、字号、行高、对齐、颜色及宽度样式。改变内容、宽度或字体会重新排版，自动高度包含全部段落。正文颜色可使用现有的 Brush，span 颜色可单独覆盖。

```rust
use mue_macroquad::{node::{document, markdown, Document, Span}, style::Styleable};

let article = markdown("# Field notes\n\nA **quiet** morning by the *sea*.")
    .width(600.).font_size(18.).line_height(28.);
let signature = document(Document::rich([
    Span::new("Mara Ellis").bold(),
    Span::new("\nOctober 13, 2026").italic(),
]));
```

Markdown 支持段落、标题、嵌套强调、行内/块代码、引用、列表及软/硬换行。链接和图片只显示标签，HTML 显示为文字；不提供编辑、点击链接、图片加载、表格或代码高亮。cosmic-text 负责字形与双向文本，CommonMark 解析由 pulldown-cmark 完成。

## 动画

```rust
use mue_core::prelude::*;
use mue_macroquad::{animation::{Animation, with_animation}, node::text, style::{Styleable, StyleableExt}};

let visible = signal(true);
let label = text("Hello")
    .opacity(visible.select(1., 0.2))
    .on_activate(move || {
        with_animation(Animation::ease_in_out(0.3), || visible.toggle());
    });
```

`with_animation` 让状态修改引起的可插值样式变化使用指定曲线，包括 computed 链、batch 内的事件与 Factory 主题。状态立即改变，显示值逐帧更新；在动画途中改变目标会从当前显示值继续。`with_animation(None, ...)` 立即切换。节点的 `.animation(Animation::spring(0.8, 0.3))` 为该节点的可插值属性提供默认动画，显式事务优先。

支持 `Animation::linear(seconds)`、`ease_in_out(seconds)` 和 `spring(seconds, bounce)`。可动画属性包括同单位尺寸/边距/内距/位置、透明度、圆角、字号、行高、变换矩阵、纯色及结构一致的渐变。`auto`、不同单位及不同种类的 Brush 立即切换。弹簧是按时长结束的阻尼曲线；重新指定目标保持位置连续，不保留速度。矩阵按系数插值。此版本不含关键帧、匹配几何或自动挂载/卸载过渡。

自定义动画数据可实现 `Animatable`。`animated(*signal)` 返回 `Animated { value, running }`，也可用 `animated_with(*signal, curve)` 指定默认曲线。`value` 可用于 computed 或 Prop，`running` 可用于完成后的响应式逻辑。资源跟随创建它的 scope 释放。`App::frame` 自动在布局前调用 `mue_macroquad::animation::advance(dt)`。动画曲线、插值和帧驱动位于 mue-macroquad，core 仅负责传播通用更新上下文。

`pages` 与 `tab` 保留三个页面，用同一个状态事务移动页面；不再维护示例内的计时器。`theme` 展示每日练习计数、按钮变体、嵌套主题以及明暗主题的 0.3 秒颜色过渡。

## 手势

通过 `Styleable` 为节点添加手势回调：

```rust
let position = signal(mue_macroquad::math::Vector::ZERO);
let direction = signal(String::new());
let scale = signal(1.0_f32);
let rotation = signal(0.0_f32);

text("Pan").on_pan(move |event| position.set(event.translation));
text("Swipe").on_swipe(move |event| direction.set(format!("{:?}", event.direction)));
text("Pinch").on_pinch(move |event| scale.set(event.scale));
text("Rotate").on_rotate(move |event| rotation.set(event.rotation));
```

- `on_pan`：单指或鼠标拖动，默认超过 10 像素后开始。提供位置、总位移及本次增量。
- `on_swipe`：释放时触发，默认位移至少 50 像素、平均速度至少 300 像素/秒。提供方向、位移及平均速度。
- `on_pinch`：双指距离变化超过 10 像素后开始。`scale` 为相对双指初始距离的比例，`delta` 为比例的加法增量。
- `on_rotate`：双指旋转超过 0.1 弧度后开始。`rotation` 为累计弧度（屏幕坐标中顺时针为正），`delta` 为本次弧度增量。

连续手势通过 `GesturePhase::{Start, Update, End, Cancel}` 报告生命周期，只有识别成功后才发送结束或取消事件。首次回调的增量包含触发阈值前的移动。坐标和距离均使用屏幕像素。双指手势在任一手指释放或取消时结束；再次识别需要两次新的按下。

手势沿用独占竞争规则：先成功认领指针的手势获胜，其余手势被拒绝。因此同一节点上的 pan 会阻止随后释放时的 swipe；pinch 和 rotate 也不会同时触发。同一节点同时达到阈值时，pinch 优先于 rotate，两者优先于 pan。示例为四种手势提供独立区域。

## 调试示例

`gestures` 和 `map` 使用深色背景、清晰的边框和高对比度状态颜色。青色表示交互，黄色表示待识别，绿色表示识别成功，红色表示取消。反馈显示在窗口内，不输出控制台日志。

### Events

`cargo run -p events` 运行简单的文本事件示例，显示点击计数、按下、hover、长按和滚轮状态。

### Gestures

```bash
cargo run -p gestures
```

八个面板采用相同尺寸、标题位置和读数位置，整个面板都可以接收输入。

| 面板 | 操作与反馈 |
| --- | --- |
| Click | 点击后累计计数，按下时圆形填充。 |
| Long press | 按住 0.5 秒，圆环显示黄色等待状态；触发后变绿并计数。提前释放、取消或识别前移动超过 15 像素时显示红色取消标记。成功状态保留到下一次按下。 |
| Hover | 鼠标进入时填充圆形，并显示 Inside；离开后显示 Outside。 |
| Scroll | 滚轮移动线条与滑块，显示累计垂直滚动量。 |
| Pan | 拖动圆形，显示总位移。 |
| Swipe | 绘制滑动线段，识别成功后变绿并显示方向与速度。 |
| Pinch | 双指圆形跟随触点，显示相对缩放比例。 |
| Rotate | 双指圆形跟随触点，显示累计角度。 |

Pinch 与 Rotate 需要多点触控输入；鼠标可以测试其余面板。长按使用当前默认识别阈值（0.5 秒、15 像素）。

### Map

```bash
cargo run -p map
```

地图重复绘制仓库根目录的 `deer.png`，形成 16 × 16 网格。图片通过 `include_bytes!` 嵌入，无需从特定工作目录启动。

| 输入 | 操作 |
| --- | --- |
| 鼠标或单指拖动 | 平移地图 |
| 快速拖动后释放 | 惯性滑动，逐渐减速 |
| 双指移动 | 同时平移、缩放和旋转 |
| 滚轮 | 以指针位置为中心缩放 |
| Shift + 滚轮 | 以指针位置为中心旋转 |
| 底部减号 / 加号 | 缩小 / 放大 |
| 底部旋转按钮 | 每次旋转 15° |
| Reset view 或 R | 恢复初始位置、缩放和角度 |

右上角显示缩放百分比与旋转角度，缩放范围为 25%–400%。地图通过 `on_pointer_event` 组合交互，以支持同一表面上的拖动、惯性和同步双指变换；它不组合互相竞争的独占手势回调。

## 验证

```bash
cargo check --workspace
cargo clippy -p events -p gestures -p map --all-targets --no-deps
```

## 备注

- `macroquad` / `miniquad` 目前指向 fork 的 git 源，见根 `Cargo.toml` 的 `[workspace.dependencies]`。
- 示例字体使用 vendored 的 Space Mono（SIL OFL 1.1，见 `examples/basics/assets/fonts/OFL-SpaceMono.txt`）。
- 可变字体在 `fontdb` 中只按默认实例登记字重；演示非默认字重时请使用按字重拆分的静态字体文件。

## License

MIT，见 [LICENSE](LICENSE)。

## 简洁的响应式属性

`expanded.select(480., 180.)` 根据布尔状态选择值，可直接传给 `.width(...)`。响应式像素值不需要手动构造 Taffy 尺寸。`text(open.select("Open", "Closed"))` 和 `text(count.map(|n| format!("{n}")))` 自动转换文本类型。

`state.toggle()` 切换布尔状态，`.toggle_on_tap(state)` 绑定点击切换。`.on_activate(move || ...)` 接受没有事件参数的点击回调。`document` / `markdown` 及其内容类型位于 `mue_macroquad::node`。
