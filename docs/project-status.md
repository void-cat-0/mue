# 项目现状与设计记录

更新：2026-11-08。记录 `async-task` 分支当前工作树；theme 移除与同步 builder 重构已实施。本轮将前台 task 生命周期改为 caller-owned：Scope dispose 不再登记/自动取消任务；句柄 drop 取消，detach 交给 executor 管理。新增 signal/prop fallible access API，并记录 derived dependency 仍需有效的边界。此前节点构建上下文采用 `NodeContext`，`#[node]` 首参按位置注入且参数名可自由选择；`mount()` 更名为 `finish()`；构建 panic 清理 guard 删除，panic 向外传播。当前 scope 分离已实现，workspace 测试、all-target 检查和格式检查均通过，详见下文。

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

2026-11-08 最新 Task 重构已实现：`Task<T>` 直接持有 `async_task::Task<T>`，await 直接产出 `T`，所以 `Task<Result<T, E>>` 直接得到业务 `Result<T, E>`，不再经过 oneshot 结果转发或 Task 内的 `Rc` finished 标志。句柄持有共享 token（原线程、全局唯一 runnable key、原 ready queue 与 cleanup-complete 原子标志）；取消直接操作 async-task allocation，不再解释调用线程的 SlotMap key。executor 的 token/waker registry 仅用于 task 结束反注册和 shutdown 唤醒清理。普通 `spawn` 不创建 `OwnedScope`、不继承 context；每次 poll 与 future 析构都运行在 `Scope::null` + `Runtime::without_effect` 下，不存在 ambient tracker / `current_effect`，正常返回时还原调用方状态。`scope_future(future)` 是显式 opt-in：包装时立即创建独立 `OwnedScope::new_inherited()`，以弱父级链接查找 context，不保活父级也不快照 context；wrapper 在 poll 与 inner future 析构时安装自己的 scope，并于 Ready 返回前析构 inner future 和 owner，即使 wrapper 被保留。wrapper 本身不要求 `'static`，但因 owner 而 `!Send + !Sync`；standalone 只控制 scope/ownership，任务执行隔离来自 `spawn`。仅在 wrapper 下实际创建的资源归其 owner；预创建捕获资源不转移所有权，输出中的局部 reactive handles 在完成后失效。Effect callbacks rerun 时不恢复其创建 scope。creator scope dispose 不会自动取消 task；drop 取消，detach 交 executor 管理。`is_finished()` 在 future 与 opt-in scope 清理完成后才为 true。完成后重复 poll，以及 await 在 executor shutdown 时被强行终止的 task，都会 panic；未增加 fallible Task API，panic 不恢复。`AsyncResource` 继续持有状态 owner 与 task 并在自身 drop 时取消；输入 operation 仍遵循普通 `spawn` 语义，除非 caller 显式包装 `scope_future`。`detach_and_report` 使用无自动 scope / context 继承的独立 reporter 生命周期。此前加入 `ReadSignal` / `Prop` fallible read、`Signal::try_set`，供可能跨 owner 生命周期的任务安全访问直接 disposed 的 reactive handle；仍存活的 derived signal 若依赖已 disposed signal，其重算仍遵守常规生命周期约束。

Reactive / Node 句柄保留 Copy 语义并通过 !Send / !Sync 标记限制跨线程。前台 future 仍允许为 `!Send`，只在原 UI 线程 poll 与析构；`Task<T>` 在 `T: Send` 时则可跨线程移动、await/drop，future 与 task-local resources 清理由原线程执行，不会把 future 移到调用线程或在那里 poll；已完成的 output 则可在观察者线程消费/析构。跨线程 drop 后原线程下一次 tick 或 shutdown 完成取消清理。`App::frame` 在 timeouts 后、layout 前驱动最多 32 个 ready task；本 tick 中产生的 wake 留到下个 tick。App 在构建根节点前安装宿主 wake callback。`AsyncResource<T, E>` 显式持有状态 owner 与任务，暴露 Loading / Ready(Result) 响应式状态，替换或 drop 时取消，生命周期与普通 OwnedScope 一致，不因为创建时的临时 scope 结束而释放。独立 dispose 的 signal 及捕获其他 owner 的句柄仍须遵守原生命周期契约。

Scope teardown 顺序修复已实现：先标记 owned scope 子树正在销毁，并排空普通 effect 与 computed producer 的 cleanup，再释放该子树资源。Cleanup 可读取仍存活的本树 signals，读取 dirty computed 新产生的 cleanup 也在资源释放前排空；scope context 保留到本 scope 资源和 descendants 析构之后。清理期间不支持创建响应式资源、重挂 scope 或修改正在销毁的 reactive graph；cleanup panic 仍按 panic-fatal 策略处理。最终验证通过：`cargo test --workspace --offline`（mue-core 92 passed、1 ignored，普通 doctest 6 passed、compile-fail doctest 19 passed；所有 workspace tests/doctests 通过）、`cargo check --workspace --all-targets --offline`、edition 2024 Rustfmt 与 `git diff --check`。新增回归覆盖父子 scope cleanup 双向读取、重复 dispose、同 scope computed cleanup、dirty computed cleanup drain、ScopedFuture 完成/取消、创建资源的早期拒绝和 context 析构顺序。使用说明见 [前台任务](tasks.md)。

### 已知限制（未修复）

以下边界在当前实现中仍存在，本轮只记录契约，不改变行为；框架自身路径不会触发。

- **任务 poll/drop 继承 `UpdateContext`**：`Runtime::without_effect` 只清 tracker / `current_effect`，`UpdateContext` 是独立 TLS。若在 `with_update_context(...)` 内调用 `tick` 或 `Task::drop`，任务写入及 future 析构写入会把该 metadata 传给下游 effect。当前生产代码没有调用 `with_update_context`，属于潜在契约问题。若任务应完全隔离，应在 `without_effect` 中保存/清空/恢复 `UpdateContext`，并在 `Executor::new` 初始化 `UpdateContext::current()` 以固定 TLS 析构顺序；若希望 metadata 传播，则应写入任务契约。
- **executor TLS 析构期间 `spawn`/`tick` 会 abort**：`Executor::drop` 运行时 executor TLS 已标记 destroyed；future 析构器若调用 `spawn`/`tick`，会在 async-task 的 `abort_on_panic` 中触发进程 abort。框架自身析构器不这样做。当前契约是 shutdown 期间不要提交新任务；可选的后续方案是新增 fallible `try_spawn`/`try_tick`，未实现。
- **`ScopedFuture` 不得比 reactive runtime 活得更久**：wrapper 绑定当前线程 runtime；若存入比 runtime 更晚销毁的 TLS 槽位，`Drop` 的 `owner.run(...)` 会因 scope 已不存在而 panic，TLS 析构中的 panic 通常 abort。当前契约是不要这样存储；可选的后续方案是 `Drop` 的 best-effort leak guard，未实现。
- **panic 不恢复**：捕获任务 poll panic 后 `tick` 的 `in_tick` 会保持 true，ambient scope/tracker 也可能残留；不要在捕获 panic 后继续复用 runtime。

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
