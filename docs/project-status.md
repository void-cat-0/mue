# 项目现状与设计记录

更新：2026-11-05。记录 `async-task` 分支当前工作树；theme 移除与同步 builder 重构已实施。本轮节点构建上下文类型改为 `NodeContext`，`#[node]` 首参按位置注入且参数名可自由选择；`scope()`、`push_style()` 方法名保持不变；本轮把 `mount()` 更名为 `finish()`，回调参数名由 `init` 改为 `body`；同时删除节点构建的 panic 清理 guard，panic 一律向外传播。Cargo、格式和 diff 验证均通过。

## 已确认的方向

删除 CSS 式 selector theme 系统。应用或组件库定义普通 Rust tokens 结构体，通过显式参数、`Prop`、`map` / `computed` 和普通 `provide` / `inject` 使用它们。被否决的是让 theme 反向侵入库结构：selector、级联与 specificity、主题专用宏或语法、扩展 registry，以及为 theme 改动构建流程。

`mue-components` 的 `tokens` 模块提供一个可选的键值 store（`TokenSet` / `Theme` / `provide_theme`）：按值类型分槽、键为字符串、读出来是普通 `Prop`，切换主题就是原地写值。它只决定某个属性拿到什么值，不参与样式解析，也不要求组件知道它的存在。现行用法见[应用 tokens 与上下文](themes.md)。

原主题系统的 `theme!`、`class!`、`ThemeTarget`、options/class 匹配、resolver、`App::with_theme` 和仅服务主题的构建上下文已移除。普通样式、响应式绑定、transition、显式 hover、前景继承、布局聚合和生命周期清理继续保留。

## 当前同步构建实现

`#[component]` 与 `#[node]` 仍生成 builder。构造器和 setter 只配置输入；`into_node()` 同步执行一次声明函数体并构建节点。参数默认值只求值一次。组件返回链共享一个真实节点的 owner 和累计样式。

同步构建 API 使用 `BuildNode` trait 与 `NodeContext` 类型。终端 `#[node]` 按位置注入 `NodeContext` 作为第一个参数；原始类型标注保留并由 Rust 检查，支持限定路径和类型别名。参数名可为任意简单标识符，包括 `mut`，例如 `ctx: NodeContext`；调用 `ctx.finish(defaults, body)` 返回真实 `Node`。该参数不进入 builder 构造器或 setter，不能标注 `#[default]` / `#[model]`，且 defaults 在注入前求值，不能引用实际第一个参数名。普通参数名 `build` 和 `style` 仍暂时保留；注入参数可使用 `build`、`style`、`cx`、`ctx` 等名称。调用构造器和 setter 只配置；`into_node()` 同步执行一次。返回另一 builder 的声明使用 `#[component]`，其返回链继续共享同一个 owner 和累计样式。具体接口见[节点教程](components.md#author-a-node)。

```rust
#[node]
fn stack(ctx: NodeContext) -> Node {
    ctx.finish(Style::new().flex_column().gap(8.), move |style| {
        use_container(style);
    })
}
```

`NodeContext::finish(defaults, body)` 接收普通 `Style` 默认值，支持依赖响应式参数的默认值；无默认值时可用 `Style::new()`。不新增 `DefaultStyle` 类型或宏默认样式属性。手写 `BuildNode` 与宏声明共享同一 finish 协议，并通过 blanket impl 自动获得 `IntoNode`（宏不再生成 `impl IntoNode`，`Component` 也不再要求 `IntoNode` 超界）。

当前 finish 顺序是：

1. 进入父组件返回链和终端节点函数，在同一真实节点的 scope 中求参数、创建资源并准备 defaults。共享 `provide` / context 必须在 `ctx.finish(...)` 前，由作者 body 或 component body 建立。
2. `finish` 先合并 defaults，再执行 `bind_style` -> `bind_node_style`，随后在借用 `NodeInner` 之前取得 font / transform props；这些响应式投影可能执行用户代码。之后转移父节点值与 hover 状态，完成 transition、opacity、font、transform 等绑定。
3. 在父 scope 中物化声明的 `ChildrenPlan`，子节点继承普通 scope context 并取得各自 owner。普通 `.children(...)` 在此前只保存计划，不构建孩子。
4. 执行 `body` 回调，初始化布局、绘制和事件消费者，随后挂接 hooks。回调中可操作已物化的 children；若需包装或重组，在 `use_layout` / `use_container` 捕获之前完成 `take_children()` / `set_children(...)`。

样式优先级为调用方/外层显式值 > 内层返回 builder 显式值 > 节点默认值 > 框架默认值或继承。同一 builder 对同属性后写覆盖先写。`Style` 恢复为普通字段；构造器、setter 和宏配置阶段只配置，`into_node()` 同步执行一次。访问或取出字段遵循普通样式数据的语义，父子构建顺序由显式 finish 流程保证。

`Style` 恢复普通公开字段和 hook 字段，例如 `style.on_render.append(callback)`、`style.on_wheel_event.append(callback)`；链式 `.on_render(...)` 等 setter 仍保留。留存的 hooks、effects 和 child 计划遵守各自生命周期约束。

`ChildrenPlan` 在父上下文和绑定建立后同步构建普通子树。`set_children` 必须在布局捕获 children 之前完成；布局建立后替换整个 children 列表不受支持。`map_keyed` 保留同 key 节点和局部状态，删除 key 释放资源；`show_if` 首次物化即构建一次，即使初始隐藏，之后隐藏仍保留状态。

预构建 `Node` 保留创建时的上下文。组件函数体中手动调用 `into_node()` 创建的独立节点也遵循这一规则：随后返回链建立父节点的最终样式和上下文时，不会追溯重建或修改先前独立节点。需要最终父上下文的普通子树应保留 builder 并交给 `.children(...)` 计划。

## 本轮迁移状态

本轮采用 `NodeContext` 作为节点构建上下文类型；`#[node]` 注入的首参按位置识别，参数名任意，声明类型保留并由 Rust 检查；普通参数的既有 `build` / `style` 规则保持不变。方法 `scope()`、`push_style()` 未更名，`mount()` 更名为 `finish()`，回调参数 `init` 更名为 `body`。`#[component]` 仍返回 builder，并沿同一 owner 和样式链继续构建。本轮删除节点构建的 panic 清理 `FinishGuard` 及其 3 条 panic 恢复测试：panic 一律向外传播，运行时不尝试恢复，同时把 `Scope::run`、`UpdateContext::run` 和 `batch` 的 Drop 守卫改为普通顺序还原，并删除 5 条只断言 unwind 还原的 core 测试，把 `map_keyed` 那条混合测试改写为只验证留存/释放语义；`EffectRun` 的 `completed` 标志及其两处 unwind 分支也改为无条件执行。同时清理历史遗留 API：`Signal::shallow` 更名为 `Signal::new_always`（无比较器、每次 set 必通知，用于非 `PartialEq` 载荷），删除没有独占能力的 `create_scope`（含 prelude 导出，相关生命周期测试改用 `OwnedScope`）和 `Style::merge`（统一用 `provide_defaults`，需要函数式组合时再加 `Style::with_defaults`），并删除从未接线的 `mue-macroquad/src/frame_timing.rs`、`examples/playground/src/timing.rs`、`TIMING.md` 及其离线分析脚本 `analyze_timings.py` / `test_analyze_timings.py`（连同误提交的 `__pycache__` 字节码）。trait 收敛为单向：`BuildNode` 是 authoring 钩子，`impl<T: BuildNode> IntoNode for T` 提供 `into_node`；`Node` 仍单独实现 `IntoNode`（identity），`ForegroundProvider` 与测试桩只实现 `IntoNode`。宏对每个声明只生成 `impl BuildNode`，删除了 4 处手写转发行和 `#host::node::build(self)` 生成代码。构建入口收面：`Node::build` / `build_with_style` 更名为 `Node::empty()` / `Node::leaf(style, body)`（前者现在没有任何 "build" 命名），参考实现与组件里注入的上下文参数统一从 `build` 改为 `ctx`（`BuildNode::build(self, ctx)` 同），`Screen::new` 的闭包参数改为 `view`。命名整理：`render_container` / `render_spans` 改为 `use_container` / `use_spans`（它们只安装布局与绘制绑定，不立即渲染），`node::after_children` 改为 `node::on_after_children`，`hit_test::do_hit_test` 改为 `hit_test_into`，`hook::HookFn` 改为 `hook::CallbackList`，`Scope::new()` 改为 `Scope::new_attached()`（与新 `new_detached()` 对称），`node::build` 补 `#[doc(hidden)]`。验证：`cargo test -p mue-macros`、`cargo test --workspace`、`cargo check --workspace --all-targets`、Rustfmt 和 `git diff --check` 均通过。

## 生命周期与异步边界

节点拥有本次构建创建的资源；构建 panic 不做恢复：本库不使用 `catch_unwind`，也不保留 panic 清理 guard，未完成的节点及其 owner、已注册的布局/手势条目都会留下，节点栈不回退。`Scope`、signal 与动态 `Prop` 句柄本身不保活 owner。`OwnedScope::new_inherited()` 继承上下文查找，不继承资源所有权。

`Scope::run` 只安装同步上下文，future 以后 poll 或 `await` 后不会自动恢复它。异步计算/加载仍需独立设计 owner、取消、完成结果的有效性和 UI 线程更新；保留延迟 children 不等于提供异步任务能力。

## 验证与历史入口

此前源码迁移曾通过 workspace check（含 all targets）、build、test、五项串行 release ignored 基准、变更 Rust 文件的 rustfmt 检查及 timing 分析器的 11 项测试。基准资源清理回到基线；不据此宣称普遍性能收益。上述性能结果不代表本轮有性能收益。本轮 `NodeContext` 重命名与首参位置注入的 Cargo、格式和 diff 检查均通过；未运行 release 性能基准。

旧 selector theme 的设计定稿、实施规格、讨论历史、验证记录与性能记录已从当前文档树删除，不再维护归档副本。历史版本保留在 Git 中，例如：

```bash
git show b4c8588:docs/theme-refactor.md
git show b4c8588:docs/theme-refactor-implementation.md
git show b4c8588:docs/theme-refactor-history.md
git show b4c8588:docs/theme-refactor-validation.md
git show b4c8588:docs/theme-refactor-performance.md
```

现行教程：[组件与节点](components.md)、[应用 tokens](themes.md)、[可复用组件声明](factories.md)、[入门](getting-started.md)。
