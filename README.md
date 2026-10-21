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
- **样式过渡**：节点 `.transition(...)`、线性/缓动/弹簧曲线，`App::frame` 自动推进，可在途中改变目标。

## 工作区结构

| crate | 说明 |
| --- | --- |
| `mue-core` | 与渲染无关的响应式内核：signal、effect、scope、prop。可通过 feature 接入 taffy / cosmic-text。 |
| `mue-macros` | `#[component]` / `#[node]` 属性过程宏与 style 风格的 `style!`。 |
| `mue-macroquad` | 渲染与运行时：节点生命周期、样式、布局、事件、手势、字体、shader。核心导出 `App`。 |
| `mue-components` | 布局、排版与图形组件，包括滚动区域、图片、形状、圆和文档；重导出组件宏与 `style!`。 |
| `examples/basics` | 响应式样式、图片、字体（Space Mono 400/700）示例。 |
| `examples/events` | 简单的点击、长按、hover、滚轮与 `set_timeout` 示例。 |
| `examples/gestures` | 八个等大可视化面板：点击、长按、hover、滚轮、拖动、滑动、双指缩放与旋转。 |
| `examples/map` | 图片网格地图，支持拖动、惯性滑动、双指缩放与旋转。 |
| `examples/pages` | 页面切换与切换动画。 |
| `examples/tab` | 固定底部 Tab 栏，页面切换与切换动画。 |
| `examples/style` | style 声明、响应式变换，以及滚动区域、图片、形状、圆和文档组件。 |
| `examples/theme` | 练习计数、按钮变体、嵌套主题与明暗主题切换。 |
| `examples/document` | Markdown 阅读视图、字号与内容切换、富文本署名。 |
| `examples/animation` | 尺寸、圆角与颜色的状态动画，弹簧与缓动切换。 |
| `examples/navigation` | 导航栈、自适应双栏、返回、深链接与页面状态保留。 |
| `examples/nodes` | 布局、排版、动态列表、嵌套滚动与链接卡片。 |
| `examples/color` | 纯色、渐变、图片与移动半调线条 shader。 |
| `examples/playground` | 多页节点与属性练习、分页、独立动画开关。 |

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
cargo run -p style-example
cargo run -p document
cargo run -p animation
cargo run -p navigation
cargo run -p nodes
cargo run -p color
cargo run -p playground
```

`Factory` 复用组件定义；主题只覆盖选中的样式字段。`theme!` 克隆分支里的 Factory 句柄，原句柄仍可用来创建实例：

```rust
use mue_components::text;
use mue_macroquad::{App, color::Color, factory::factory, style::Styleable, theme::{provide_theme, theme}};

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
use mue_components::prelude::*;
use mue_macroquad::App;

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

## style 风格的 `style!`

保留 Rust 组件 builder，通过 `.style(style! { ... })` 应用样式，不引入模板语言。
属性使用 style 的 kebab-case 名称；同一字段后声明覆盖前声明，也遵循 `.style(...)` 与后续 builder 调用的顺序。

```rust
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::style::Size;

let width = signal(Size::Percent(80.));
let card = flexbox()
    .style(style! {
        display: flex;
        flex-direction: column;
        width: {width};
        max-width: 640px;
        margin: 0 auto;
        padding: 16px 24px;
        gap: 12px;
        background-color: #eef4ff;
        border-radius: 12px;
        transform: translateY(8px) rotate(-2deg);
        transform-origin: center top;
    })
    .children(text("Hello").style(style! {
        font-size: 1.25rem;
        line-height: 1.5;
        color: rgb(20 40 80 / 90%);
    }));
width.set(Size::Px(480.));
```

`{Rust 表达式}` 接受对应属性的原有类型、`Prop` 或 signal，保留响应式更新；每个表达式只求值一次。
`padding` / `margin` / `inset` / `border-width` 使用 style 的一至四值顺序：上、右、下、左。
`gap: 8px 16px` 分别设置行、列间距。它们与原 builder 的 tuple 顺序无关。

支持尺寸与定位、flex 布局、overflow、颜色、透明度、字体、行高、字距、文本对齐与装饰，以及二维和有限三维变换。
长度支持 `px`、`%`、`vw`、`vh`（取决于属性），`font-size` 另支持 `em` / `rem`；零可省略单位。
颜色支持十六进制、`rgb` / `rgba` 和常用颜色名。`line-height` 的无单位数和百分比按当前字号计算。

`transform` 是节点样式，作用于整个子树，绘制和命中测试使用同一变换。
支持 `translate` / `translateX` / `translateY`、`translate3d` / `translateZ`、`scale` / `scaleX` / `scaleY` / `scale3d`、`rotate` / `rotateX` / `rotateY`、`skew` / `skewX` / `skewY`、`perspective`、`matrix`、`matrix3d` 和 `none`。
按 style 顺序组合；translate 百分比相对节点自身 border box，默认变换原点为中心；变换不改变布局占位。
角度支持 `deg` / `rad` / `turn` / `grad`。`transform: {value}` 和 `.transform(value)` 都接受
Rust 矩阵、`Transform` 函数列表及其响应式属性，并参与现有动画；
同类、同单位的列表及 `none` 与列表之间可插值，不兼容列表立即切换。

这是当前画布渲染器支持的 style 声明子集，不包含 `calc()`、grid 或完整的 3D 场景渲染；3D 变换会投影到 2D 画布。
`background` 当前只接受颜色或 Rust Brush 表达式，圆角只支持统一半径；`white-space` 只控制换行，不实现空白折叠。
`overflow` 是布局属性，交互滚动使用 `scroll_area` 组件。不支持的属性或值在编译期报错。
运行 `cargo run -p style-example` 查看组合示例。

## 长度单位

`mue_macroquad::style` 的长度是纯值 `Size`：`Size::Px`、`Size::Percent`、`Size::Vw`、`Size::Vh` 和 `Size::Auto`。公开百分比以 100 表示整体，不限制为 `0..=100`（例如 `Size::Percent(50.)` 表示 50%），视口单位分别相对逻辑视口宽度和高度。`px`、`percent`、`vw`、`vh` 是对应的纯值构造器，不会把值预先换算成当前视口像素。

```rust
use mue_components::text;
use mue_macroquad::style::{FontSize, Styleable, percent, px, vw};

let label = text("Hello")
    .width(percent(80.))
    .font_size(FontSize::px(18.))
    .radius(vw(1.))
    .padding((px(12.), px(8.)));
```

裸 `f32` 及 `Signal<f32>` / `ReadSignal<f32>` 均表示像素；显式的动态像素值可写成 `signal.map(Size::Px)`。需要保留单位并在运行时切换的值，应使用 `Signal<Size>`（或 `Signal<FontSize>`）并单独回写，例如在 `Px`、`Percent`、`Vw`、`Vh` 和 `Auto` 之间切换。信号保存的是声明本身，视口变化只影响随后解析，不会把声明改写成像素。

```rust
use mue_core::prelude::*;
use mue_components::text;
use mue_macroquad::style::{FontSize, Size, Styleable};

let width = signal(Size::Percent(50.));
let font_size = signal(FontSize::Em(1.25));
let label = text("Editable units").width(width).font_size(font_size);
width.set(Size::Px(240.));
font_size.set(FontSize::Rem(1.5));
```

字号使用独立的纯值 `FontSize`：`FontSize::Px`、`Percent`、`Em`、`Rem`、`Vw`、`Vh`，也提供 `FontSize::px`、`percent`、`em`、`rem`、`vw`、`vh` 构造器。默认根字号为 32px；未声明字号继承父节点的计算值，`Percent` / `Em` 相对父字号，`Rem` 相对根字号，根节点的相对单位以 32px 为基准。

所有布局长度及 radius、line-height、paragraph-spacing、letter-spacing 都保留 `Prop<Size>` 声明，在具体消费边界解析。布局支持 `Percent` 和 `Auto` 的地方保留 Taffy 语义；标量长度支持 `Px`、`Vw`、`Vh`，对 `Percent` 和 `Auto` 明确拒绝，不隐式转成像素。`opacity`、flex 权重和宽高比仍是无量纲数值，不能传长度。

示例默认以 `Percent`、`Vw`、`Vh`、`flex` 和 `wrap` 组织布局，优先让内容随逻辑视口和父容器变化；`max_width` / `max_height` 只作为桌面端阅读宽度或演示上限。地图坐标、绘图形状、动画位移，以及滚动示例中刻意制造溢出的超宽内容仍使用固定值，因为它们是对应 API 的演示参数而不是界面尺寸。移动端验证建议使用窄屏窗口并分别测试横屏、竖屏和高分辨率设备。

示例排版统一使用 `examples/shared/responsive.rs`：以视口短边 480 为设计基准，`scale = max(min(width, height) / 480, 0.75)`，根字号为 `32 * scale`，子字号通过 `font(...)` 转为 `Rem`，行高、触控目标及相关留白通过 `ui(...)` 同比缩放。各入口在构建前和每帧布局前同步尺寸，旋转不改变缩放比例，高分辨率不设置缩放上限；这是示例级视口适配，不是系统 DPI 或无障碍字号换算。

自定义 `#[node]` / `#[component]` 参数直接声明为 `Size` 或 `FontSize`，宏会生成接受 `impl Into<Prop<T>>` 的构造器和 setter。默认值可写为 `#[default(Size::Px(8.))]`，或使用 `#[default]` 采用该类型的默认值。`#[model]` 参数接收可回写的 `Signal<T>`。

`Size` 和 `FontSize` 定义在 `mue_core::units`，并由 `mue_macroquad::style` 重新导出。标准 `From/Into` 支持裸 `f32`、`Prop<f32>` 和数值信号；启用 `impl-taffy` feature 时，`Size` 也接受 Taffy 长度输入。`mue-macroquad` 已启用该 feature。

## 文档与 Markdown

`document("第一段\n\n第二段")` 显示纯文本；`markdown(source)` 将 CommonMark 转成 cosmic-text 的富文本 spans。两者都接受响应式属性，使用现有的字体、字号、行高、对齐、颜色及宽度样式。改变内容、宽度或字体会重新排版，自动高度包含全部段落。正文颜色可使用现有的 Brush，span 颜色可单独覆盖。

```rust
use mue_components::{document, markdown, Document, Span};
use mue_macroquad::style::Styleable;

let article = markdown("# Field notes\n\nA **quiet** morning by the *sea*.")
    .width(600.).font_size(18.).line_height(28.);
let signature = document(Document::rich([
    Span::new("Mara Ellis").bold(),
    Span::new("\nOctober 13, 2026").italic(),
]));
```

Markdown 支持段落、标题、嵌套强调、行内/块代码、引用、列表及软/硬换行。链接和图片只显示标签，HTML 显示为文字；不提供编辑、点击链接、图片加载、表格或代码高亮。cosmic-text 负责字形与双向文本，CommonMark 解析由 pulldown-cmark 完成。

## 过渡

```rust
use mue_core::prelude::*;
use mue_components::text;
use mue_macroquad::{transition::Transition, style::{Styleable, StyleableExt}};

let visible = signal(true);
let label = text("Hello")
    .opacity(visible.select(1., 0.2))
    .transition(Transition::ease_in_out(0.3))
    .on_activate(move || visible.toggle());
```

`.transition(...)` 为节点的可插值属性提供默认动画。状态立即改变，显示值逐帧更新；在动画途中改变目标会从当前显示值继续。`Transition::Instant` 立即切换。

支持 `Transition::linear(seconds)`、`ease_in_out(seconds)` 和 `spring(seconds, bounce)`。可动画属性包括同单位 `Size` 尺寸/边距/内距/位置、透明度、圆角、`FontSize` 字号、行高、变换矩阵、纯色及结构一致的渐变。同一单位按数值插值；不同单位以及 `Auto` 离散切换，不做隐式单位换算。不同种类的 Brush 立即切换。弹簧是按时长结束的阻尼曲线；重新指定目标保持位置连续，不保留速度。矩阵按系数插值。导航节点负责屏幕进入/退出过渡；普通节点不自动添加挂载/卸载过渡。此版本不含关键帧或匹配几何。

实现 `mue_macroquad::transition::Interpolatable` 可以扩展可过渡值。`App::frame` 会在布局前调用 `mue_macroquad::transition::advance(dt)`。

更完整的主题、factory、组件和示例说明位于 [`docs/`](docs/)。

`pages` 示例使用独立的同级页面组件，顶部按钮改变选中索引，所有页面保留局部状态；`tab` 示例自行组合底部按钮与位置动画。`theme` 展示每日练习计数、按钮变体、嵌套主题以及明暗主题的 0.3 秒颜色过渡。

## 同级页面

`pages(selected, children)` 用静态或响应式 `usize` 索引选择固定列表中的页面。页面按传入顺序排列，索引增加时向左滑动，减少时向右滑动；跨页跳转经过中间页。全部页面只构建一次并保留到组件释放，切换后计数器等局部状态不会重置。按钮栏由调用方组合，不提供滑动手势。

```rust
use mue_core::prelude::*;
use mue_components::{pages, text, PagesTransition};
use mue_macroquad::style::StyleableExt;

let selected = signal(0usize);
let view = pages(selected, [text("Home"), text("Notes"), text("Settings")])
    .transition(PagesTransition::Slide);
selected.set(2);
selected.set(1); // 向右滑动回到 Notes
```

`.page_transition(...)` 支持 `Slide`（默认）、`SlideVertical` 和 `None`；`.transition_curve(...)` 配置曲线。垂直切换时索引增加向上、减少向下。动画中途切换从当前显示位置重新定向，位移限制在起点和终点之间；尺寸变化时立即结束过渡并重新布局。只有视口内页面参与布局与绘制，仅选中的页面参与命中测试。空列表显示空内容；越界索引按最后一页显示，不修改选中信号。

`pages` 用于同级页面；需要 push/back、详情层级或分栏时使用 `navigation_stack`。

## 导航

`navigation_stack` 用 `Signal<Vec<Route>>` 描述根页面之上的路径；`.split_view(280., 720.)` 在容器宽度达到 720 像素时并排显示根页面与当前目标页面，窄屏自动折叠为栈。可用 `.split_view(280., 720.)` 调整侧栏宽度和断点。空路径显示完整的根页面。

```rust
use mue_core::prelude::*;
use mue_components::{navigation_stack, text};

let path = signal(Vec::<String>::new());
let view = navigation_stack(
    path,
    || text("Library"),
    |route: &String| text(route.clone()),
).split_view(280., 720.);
path.update(|routes| routes.push("Details".into())); // push
path.update(|routes| { routes.pop(); });             // back
path.set(vec!["Details".into(), "Notes".into()]);   // deep link
path.set(Vec::new());                               // pop to root
```

根页面与目标页面通过闭包创建，可在其中创建局部 signal。路径不变的前缀保留页面和局部状态；移除或替换路径项会释放对应页面。相同值在不同深度有独立状态，同一深度的相同值视为同一页面。隐藏页面不参与布局、绘制或命中测试，切换单双栏不会重建页面。返回按钮、标题和其他导航控件由页面自行组合。

运行 `cargo run -p navigation` 查看双栏示例，调整窗口宽度查看折叠；`cargo run -p navigation -- --stack` 使用普通导航栈。页面计数器展示 push / back 后的状态保留，Escape 返回上一层。

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

`gestures` 和 `map` 的交互读数显示在窗口内，不输出控制台日志。

### Events

`cargo run -p events` 运行简单的文本事件示例，显示点击计数、按下、hover、长按和滚轮状态。

### Gestures

```bash
cargo run -p gestures
```

八个面板采用相同尺寸、标题位置和读数位置，整个面板都可以接收输入。

| 面板 | 操作与反馈 |
| --- | --- |
| Tap | 点击后累计计数。 |
| Long press | 按住 0.5 秒后累计计数。 |
| Hover | 显示鼠标是否位于面板内。 |
| Wheel | 显示累计垂直滚轮量。 |
| Pan | 显示拖动阶段与总位移。 |
| Swipe | 显示识别到的滑动方向。 |
| Pinch | 显示双指相对缩放比例。 |
| Rotate | 显示双指累计旋转角度。 |

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
# 需要图形环境；校验新字形在同帧提前 flush 时可见
cargo run -p atlas -- --verify-upload
# 内置 Noto，不依赖系统字体；限时压力测试并检查图集拒绝计数
cargo run -p atlas -- --frames 120 --glyphs 4000 --max-size 28 --verify
# 交互模式；可切换到超预算场景观察拒绝计数
cargo run -p atlas
```

字形图集按需创建 1024² RGBA 页面，最多 8 页，满载时约占 32 MiB GPU 图集和 32 MiB 常驻 CPU 镜像，另有上传临时缓冲、字体源数据及驱动开销。每个文本节点先合并上传新字形，再提交几何，后续自定义节点提前 flush 也能读取正确像素；本轮已引用的字形在下一轮提交已有绘制后才允许回收。淘汰候选在发生压力时每轮只排序一次，并逐步回收。单个字形超出页面或单轮工作集超过预算时跳过该字形，不覆盖已排队内容，也不因缓存压力 panic；交互模式中的 4000 字形、大字号组合可能刻意超出此预算。连续字号动画仍按原始精度栅格化，本版本没有字号量化。

## 备注

- `macroquad` / `miniquad` 目前指向 fork 的 git 源，见根 `Cargo.toml` 的 `[workspace.dependencies]`。
- `basics` 使用 vendored Space Mono（SIL OFL 1.1，见 `examples/basics/assets/fonts/OFL-SpaceMono.txt`）。`playground` 自包含嵌入 Noto Sans / Serif / Mono、Sans CJK SC、Sans Arabic 和 Symbols 2，不依赖移动端系统字体；基础三类映射到对应 Noto 家族。字体资源约 37.1 MiB，覆盖示例文字而非所有语言，不包含彩色 Emoji。进入 playground 的 **Font licenses** 页面可查看随二进制嵌入的版权和完整 OFL 1.1 协议；固定来源、版本及 SHA-256 见 `examples/playground/assets/fonts/sources.json`。
- 无系统字体环境可先构建 `mue_macroquad::font::fontdb::Database`，加载字体并设置通用家族，再调用 `font::initialize(locale, db)`。必须在当前 UI 线程加载其他字体或创建文本前调用；重复初始化会返回错误，避免使现有文本的字体 ID 失效。
- 可变字体在 `fontdb` 中只按默认实例登记字重；演示非默认字重时请使用按字重拆分的静态字体文件。

## License

MIT，见 [LICENSE](LICENSE)。

## 简洁的响应式属性

`expanded.select(480., 180.)` 根据布尔状态选择值，可直接传给 `.width(...)`。响应式像素值不需要手动构造 Taffy 尺寸。`text(open.select("Open", "Closed"))` 和 `text(count.map(|n| format!("{n}")))` 自动转换文本类型。

`state.toggle()` 切换布尔状态，`.toggle_on_tap(state)` 绑定点击切换。`.on_activate(move || ...)` 接受没有事件参数的点击回调。`document` / `markdown` 及其内容类型位于 `mue_components`。


## 布局与排版节点

所有组件的实现与构建器都位于 `mue-components`，按 `layout`、`typography`、`graphics` 分组，
也可以从 crate 根或 `mue_components::prelude::*` 导入。原来的 `mue_macroquad::node` 组件导入需迁移到这里。
`mue-macroquad::node` 只保留节点生命周期、自定义节点和 children 树工具；字体排版与 GPU 绘制仍由渲染器负责。

| 分组 | 节点 |
| --- | --- |
| 布局 | `pages`、`navigation_stack`、`aspect_ratio`、`center`、`container`、`div`、`flexbox`、`float`、`scroll_area`、`separator`、`stack` / `hstack` / `vstack`、`wrap` |
| 排版 | `blockquote`、`code`、`code_block`、`em`、`heading`、`link`、`link_overlay`、`list` / `list_item`、`text`、`document` / `markdown` |
| 图形 | `circle`、`shape`、`image` |

`stack()` 默认纵向排列，`wrap()` 自动换行，两者默认间距为 8 像素。
`.gap(...)` 同时设置行列间距，也可以分别设置 `.row_gap(...)` 和 `.column_gap(...)`。
`container()` 默认最大宽度 1280 像素、水平居中、左右内边距 24 像素。
`aspect_ratio().ratio(16. / 9.)` 在一个尺寸为自动值时约束宽高比；
`float().placement(FloatPlacement::TopEnd)` 将子节点中心锚定在父节点右上角。
所有默认样式都可以通过现有样式 API 覆盖。

`heading` 使用 `HeadingLevel`；`list().marker(ListMarker::Ordered(1))` 给任意子节点添加编号。
`code_block` 默认保留长行，可以放入横向滚动区域，或用 `.text_wrap(true)` 开启换行。
`link` 和 `link_overlay` 通过 `.on_activate(...)` 绑定应用自己的导航操作，
不会自动打开浏览器；卡片内的子链接优先响应点击。
这些是画布节点，不提供浏览器 DOM 的键盘焦点和无障碍语义。

`Document::blocks(...)` 组合段落；`Document::heading`、`Document::em`、
`Document::code`、`Document::blockquote` 和 `Document::list` 复用相同的排版约定。
文档仍由现有 cosmic-text 富文本实现渲染，支持同一段落内的多种样式。
Markdown 保持只读：链接显示标签，代码不做语法高亮，引用不绘制独立节点的边线。

```rust
use mue_core::prelude::*;
use mue_components::prelude::*;
use mue_macroquad::math::Vector;

let offset = signal(Vector::ZERO);
let view = scroll_area()
    .height(420.)
    .offset(offset)
    .options(ScrollOptions {
        mouse_drag: true,
        ..Default::default()
    })
    .content_style(Style::new().gap(16.).padding((20.,)))
    .children((
        heading("Notes").level(HeadingLevel::H2),
        text("Content keeps its existing styles."),
        code_block("let view = stack().gap(16.);"),
    ));
```

滚动区域需要有限高度；横向滚动还需要有限宽度。默认纵向滚动、启用触摸和滚轮、
显示可拖动滚动条，不启用鼠标拖动内容。`.axis(ScrollAxis::Horizontal)` 和
`ScrollAxis::Both` 切换方向。`ScrollOptions` 提供触摸/鼠标/滚轮开关、拖动阈值、
方向锁定、惯性、摩擦系数、速度上限、滚轮倍率、滚动条与边界传递选项。
超过拖动阈值才争夺手势，取消和第二触点会停止滚动。已获得手势的拖动保持所有权直到释放；
嵌套触摸区域在开始拖动时若已到边界，可以交给祖先。滚轮会把未消费的距离传给祖先。
边界使用硬停止，不做弹性回弹。滚轮沿用系统惯性，不额外模拟；Macroquad 未区分滚轮和触控板单位，
可通过 `wheel_speed` 调整。裁剪采用轴对齐矩形，不随绘制 transform 旋转。
外部写入 `.offset(...)` 的信号可跳转位置，内容变化后会重新限制范围并停止旧惯性。

运行 `cargo run -p nodes` 查看布局、排版、动态列表、嵌套滚动和链接卡片示例。
组件职责参考 [Chakra UI](https://github.com/chakra-ui/chakra-ui)，
滚动输入选项参考 [egui ScrollArea](https://github.com/emilk/egui/blob/main/crates/egui/src/containers/scroll_area.rs)；
实现复用 Mue 现有 Taffy 布局、cosmic-text 和手势系统。


## 示例与交互练习

`cargo run -p playground` 打开按页分组的交互练习：布局、排版、图形、动画、滚动、输入与主题。
属性可单独切换，数值属性使用弹簧动画；底部可以前进、后退、切换过渡方向与分栏。
滚动页提供分页和 `ScrollOptions` 的各项输入/物理参数。配置选项切换会重建相应视口，
共享 offset 保留并重新限制滚动位置。其他示例只保留各自主题需要的代码。

排版增加 `.text_wrap_mode(TextWrap::Word / Glyph / WordOrGlyph)`、
`.text_overflow(TextOverflow::Visible / Clip / EllipsisStart / EllipsisMiddle / EllipsisEnd)`、
`.text_max_lines(Some(n))`、`.text_vertical_align(TextVerticalAlign::Top / Center / Bottom)`、
`.letter_spacing(px)`、`.paragraph_spacing(px)`、`.tab_width(n)` 与 `.text_strikethrough(bool)`。
`.text_wrap(false)` 仍可关闭换行。段落间距按换行分隔的段落计算；行限制裁剪超出的内容，
默认 Visible 保持原有溢出显示；Clip 在内容边界裁剪。省略号通过 cosmic-text 排版，未设置行限制时默认使用一行。
这些样式也适用于 document、markdown、heading、code 等共享文本渲染的节点。
行列间距、段落间距、字距与宽高比现已加入样式动画绑定。不同枚举值仍然立即切换。

导航默认使用弹簧横向滑动；`.transition(NavigationTransition::SlideVertical)` 纵向切换，
`NavigationTransition::None` 立即切换。`.transition_curve(Transition::spring(0.5, 0.2))`
设置动画曲线。退出页面在过渡结束后释放，已保留页面的局部状态不变；过渡中的旧页面不接收输入。
导航内部驱动过渡，示例不再逐帧维护独立的 offset。

`cargo run -p color` 包含渐变、图片和移动半调线条 GPU 材质。
图片来自 Picsum / Unsplash，作者与来源见 `examples/color/assets/README.md`，资源已保存在本地，运行无需联网。
材质示例位于 `examples/shared/halftone.rs`；`Shapes::build` 可创建供 `shape` 节点复用的局部坐标几何。
