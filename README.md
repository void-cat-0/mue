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
cargo run -p document
cargo run -p animation
cargo run -p navigation
cargo run -p nodes
cargo run -p color
cargo run -p playground
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

## 长度单位

`mue_macroquad::style` 的长度是纯值 `Size`：`Size::Px`、`Size::Percent`、`Size::Vw`、`Size::Vh` 和 `Size::Auto`。公开百分比以 100 表示整体，不限制为 `0..=100`（例如 `Size::Percent(50.)` 表示 50%），视口单位分别相对逻辑视口宽度和高度。`px`、`percent`、`vw`、`vh` 是对应的纯值构造器，不会把值预先换算成当前视口像素。

```rust
use mue_macroquad::{node::text, style::{FontSize, Styleable, percent, px, vw}};

let label = text("Hello")
    .width(percent(80.))
    .font_size(FontSize::px(18.))
    .radius(vw(1.))
    .padding((px(12.), px(8.)));
```

裸 `f32` 及 `Signal<f32>` / `ReadSignal<f32>` 均表示像素；显式的动态像素值可写成 `signal.map(Size::Px)`。需要保留单位并在运行时切换的值，应使用 `Signal<Size>`（或 `Signal<FontSize>`）并单独回写，例如在 `Px`、`Percent`、`Vw`、`Vh` 和 `Auto` 之间切换。信号保存的是声明本身，视口变化只影响随后解析，不会把声明改写成像素。

```rust
use mue_core::prelude::*;
use mue_macroquad::{node::text, style::{FontSize, Size, Styleable}};

let width = signal(Size::Percent(50.));
let font_size = signal(FontSize::Em(1.25));
let label = text("Editable units").width(width).font_size(font_size);
width.set(Size::Px(240.));
font_size.set(FontSize::Rem(1.5));
```

字号使用独立的纯值 `FontSize`：`FontSize::Px`、`Percent`、`Em`、`Rem`、`Vw`、`Vh`，也提供 `FontSize::px`、`percent`、`em`、`rem`、`vw`、`vh` 构造器。默认根字号为 32px；未声明字号继承父节点的计算值，`Percent` / `Em` 相对父字号，`Rem` 相对根字号，根节点的相对单位以 32px 为基准。

所有布局长度及 radius、line-height、paragraph-spacing、letter-spacing 都保留 `Prop<Size>` 声明，在具体消费边界解析。布局支持 `Percent` 和 `Auto` 的地方保留 Taffy 语义；标量长度支持 `Px`、`Vw`、`Vh`，对 `Percent` 和 `Auto` 明确拒绝，不隐式转成像素。`opacity`、flex 权重和宽高比仍是无量纲数值，不能传长度。

自定义 `#[node]` / `#[component]` 参数直接声明为 `Size` 或 `FontSize`，宏会生成接受 `impl Into<Prop<T>>` 的构造器和 setter。默认值可写为 `#[default(Size::Px(8.))]`，或使用 `#[default]` 采用该类型的默认值。`#[model]` 参数接收可回写的 `Signal<T>`。

`Size` 和 `FontSize` 定义在 `mue_core::units`，并由 `mue_macroquad::style` 重新导出。标准 `From/Into` 支持裸 `f32`、`Prop<f32>` 和数值信号；启用 `impl-taffy` feature 时，`Size` 也接受 Taffy 长度输入。`mue-macroquad` 已启用该 feature。

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

支持 `Animation::linear(seconds)`、`ease_in_out(seconds)` 和 `spring(seconds, bounce)`。可动画属性包括同单位 `Size` 尺寸/边距/内距/位置、透明度、圆角、`FontSize` 字号、行高、变换矩阵、纯色及结构一致的渐变。同一单位按数值插值；不同单位以及 `Auto` 离散切换，不做隐式单位换算。不同种类的 Brush 立即切换。弹簧是按时长结束的阻尼曲线；重新指定目标保持位置连续，不保留速度。矩阵按系数插值。导航节点负责屏幕进入/退出过渡；普通节点不自动添加挂载/卸载过渡。此版本不含关键帧或匹配几何。

自定义动画数据可实现 `Animatable`。`animated(*signal)` 返回 `Animated { value, running }`，也可用 `animated_with(*signal, curve)` 指定默认曲线。`value` 可用于 computed 或 Prop，`running` 可用于完成后的响应式逻辑。资源跟随创建它的 scope 释放。`App::frame` 自动在布局前调用 `mue_macroquad::animation::advance(dt)`。动画曲线、插值和帧驱动位于 mue-macroquad，core 仅负责传播通用更新上下文。

`pages` 与 `tab` 使用导航节点切换页面，分别把控件放在顶部与底部；根页面保留局部状态，替换目标页面会重建该页面。`theme` 展示每日练习计数、按钮变体、嵌套主题以及明暗主题的 0.3 秒颜色过渡。

## 导航

`navigation_stack` 用 `Signal<Vec<Route>>` 描述根页面之上的路径；`.split_view(280., 720.)` 在容器宽度达到 720 像素时并排显示根页面与当前目标页面，窄屏自动折叠为栈。可用 `.split_view(280., 720.)` 调整侧栏宽度和断点。空路径显示完整的根页面。

```rust
use mue_core::prelude::*;
use mue_macroquad::node::{navigation_stack, text};

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


## 布局与排版节点

节点按 `node::layout`、`node::typography`、`node::graphics` 分组，原来的
`node::*`、`node::text`、`node::document` 和 `node::image` 导入仍然有效。
每个文件放一种节点，相关子类型和构建器留在同一文件。

| 分组 | 节点 |
| --- | --- |
| 布局 | `navigation_stack`、children 工具、`aspect_ratio`、`center`、`container`、`div`、`flexbox`、`float`、`scroll_area`、`separator`、`stack` / `hstack` / `vstack`、`wrap` |
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
use mue_macroquad::{math::Vector, node::*, style::{Style, Styleable, StyleableExt}};

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
`NavigationTransition::None` 立即切换。`.transition_animation(Animation::spring(0.5, 0.2))`
设置动画曲线。退出页面在过渡结束后释放，已保留页面的局部状态不变；过渡中的旧页面不接收输入。
导航内部驱动过渡，示例不再逐帧维护独立的 offset。

`cargo run -p color` 包含渐变、图片和移动半调线条 GPU 材质。
图片来自 Picsum / Unsplash，作者与来源见 `examples/color/assets/README.md`，资源已保存在本地，运行无需联网。
材质示例位于 `examples/shared/halftone.rs`；`Shapes::build` 可创建供 `shape` 节点复用的局部坐标几何。
