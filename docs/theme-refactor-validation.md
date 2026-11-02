# Theme 重构实施与验证记录

日期：2026-10-31。依据 [首版实施规格](theme-refactor-implementation.md)，变更保留在当前工作树，未创建提交。此记录区分实现、自动验证和宿主实测的覆盖边界。

## 已实施

- 删除 factory 模块及 Factory/FactoryInstance/FactoryUse、ThemeSource/provider、旧 match selector、SelectorValue 与 factory sibling identity 协议；组件、示例和使用文档迁移到新入口。
- 声明宏生成显式 PrepareNode 返回链、RootBuild、NodePlan 与 PreparedNode；参数默认值与 prepare 仅执行一次，终端 seal 后才建立消费者和物化 children。
- 只有显式 theme 标记生成 ThemeTarget；支持具体 type/const 泛型身份、allowlist Options、私有 Prop 字段和 tracked owned getters。无顶层主题时不构造 options/resolver。
- 有序规则与独立 selector specificity；跨共根 target 按规则去重，外层显式 > 内层显式 > theme > fallback，动态增删规则通过稳定字段投影更新已有节点。
- ClassName/class!、整集合 classes 替换、compound/逗号 selector、递归 dollar-option 引用；无 ID、结构组合器或嵌套主题。
- 顶层 App::with_theme 接收 Prop<Theme>，build 回调资源由应用拥有；deferred tuple/Vec/array children、keyed 创建上下文恢复、每 key/per-screen 独立 scope。
- detached owning scope 与 context lookup 分离、scope/Node 栈 unwind guard，scheduler 临时状态恢复；构建失败清理覆盖配置、prepare、seal、child/parent mount，布局 ID 在首个响应式读取前登记。
- 提前 hover 资源移交、共根 canonical/alias 同批更新、最上层真实 Node 路径、拖动与捕获分离、触摸过滤、每帧重命中、窗口离开/焦点清理、NodeRef 只读查询。
- 规则重算的派生图由当前 resolver generation 拥有；父 opacity 在 child mount 前绑定；Tween 销毁时撤销 registry 的弱引用，不等待下一帧回收分配块。

## 自动验证

在当前 Linux/WSL2 宿主执行：

- cargo fmt --all --check
- git diff --check
- cargo check --workspace --all-targets
- cargo test --workspace
- cargo test -p mue-macroquad --lib --release node::build::perf -- --ignored --nocapture --test-threads=1
- node --check vendor/miniquad/js/gl.js

工作区 check 与普通 test 均通过。单独 ignored 性能基线已运行，8 个场景的实测 p50/p95、分配/释放、resolver/projection/transition 计数、销毁后资源存量见 [性能记录](theme-refactor-performance.md)。没有设定未经测量的性能门槛；无主题 fast path 保留。

测试包括真实宏跨 crate 展开、泛型/const/alias、非 Clone 非 option 参数、model accessor、class 编译诊断、selector 语法拒绝；实际获胜声明、动态 fallback、真实 Taffy/font/transform 消费者、不会重跑 prepare/mount、children key 复用与删除、hover 原子可见性、节点/布局/手势/core 资源失败矩阵。

Vendor macroquad 输入桥接的独立 replay 测试通过（1 项），通过 workspace 既有依赖构建 test harness：cargo rustc -p macroquad --lib --crate-type bin -- --test。未依赖独立 vendor manifest 重新拉取全平台 git 依赖。

## 图形 smoke 与平台边界

- cargo run -p atlas -- --frames 4 --glyphs 100 --max-size 28 --verify：通过，内嵌字体实际绘制 4 帧；oversized/exhausted/raster rejected 均为 0。
- style-example --frames=3：宿主未提供默认字体，cosmic-text 报 no default font found；该项不计为通过。使用 atlas 内嵌字体 smoke 覆盖基础节点迁移后的真实绘制路径。
- Linux X11 enter/leave 使用已有平台事件来源，Macroquad 队列/重放桥接已修复；Wayland 新增 pointer enter/leave 转发，host 编译通过，但未做 Wayland 会话人工交互。
- 浏览器新增 enter/leave exports 和鼠标 PointerEvent 转发，触摸兼容鼠标过滤；JavaScript 语法检查通过，未做 wasm target 编译或真实浏览器交互。
- Windows 新增 PT_MOUSE client tracking、WM_MOUSELEAVE/pointer enter/leave；macOS content view tracking area 支持拖动中 entered/exited。两个平台已有焦点转发保留，源文件经格式/语法解析；宿主未安装对应 Rust targets，未声称通过其目标编译或窗口实测。

本轮未引入实例 ID、结构组合器、局部 provider、多指针、非 Clone option 或借用 accessor。外部 signal 仍由其原 scope 管理，节点销毁不销毁外部源。
