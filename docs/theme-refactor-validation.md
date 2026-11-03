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

## 2026-11-02：identity 索引与初步优化

- Selector 增加可选具体 ThemeKey，具名类型宏分支使用 Selector::for_target；通用/手写 Selector::new 保持通用候选语义。Theme::new/compose 建立不可变规则位置索引，clone 共享；不缓存匹配结果或动态声明值。
- 单桶/全覆盖候选直接借用，多桶一次 reserve 后按源码位置去重；短链 key 去重留在栈上，超过 8 个 target 使用 HashSet，避免长链前缀扫描。
- 新增 10 项回归测试，含 512 组 indexed/linear 稀疏属性与声明执行顺序对照、泛型/alias 元数据、同 target 条件、动态 class/options/hover 和主题替换，以及长链重复 key。跨 crate host fixture 同步新构造 API。
- 本次实际通过：cargo test --workspace（含跨 crate 展开与 doctests）；cargo check --workspace --all-targets；10 项 theme::tests；单独 release resolver_identity_scaling；单独 release cpu_performance_baseline；改动 Rust 文件的 rustfmt --check；git diff --check。Cargo 使用显式 /root/mue/Cargo.toml；cargo fmt 在此环境中选中父 workspace，改为 scoped rustfmt，未将失败的 cargo fmt 记为通过。
- 原有 8 个 pipeline 基线场景均通过，无主题 resolver/projection 仍为 0，动态更新不重跑 prepare/mount，所有场景清理后 reactive arenas 回到基线。动态创建 Theme 会付出索引初始化成本；resolver 规模对照与测量边界见性能记录。
- 本次没有执行平台图形测试。用户已确认 Windows/macOS 测试完成；本记录不据此补造 target、命令或交互覆盖明细。wasm 按用户要求不纳入本轮。

## 2026-11-03：按需属性投影

- seal 保存共享 resolver、fallback Rc 快照、seal scope 与继承前景输入；不预建所有属性计算。declared_*/take_* 首次消费建立并缓存最终 Prop，重复消费共享投影与 transition 包装。矩形 side 与合成值均缓存，公开 setter 使合成缓存失效。
- 节点 opacity/font-size/transform 在 mount 消费，其余由 layout/paint/text 按需消费。五处直接 visual 字段读取已改用 accessor；同步计算在借用 NodeInner 前初始化。原始 Style 的稀疏声明和无主题路径保持原行为。
- 显式与按需主题属性共用 transition 字段策略，保留 matrix 列数组插值和动态曲线。投影持有独立 fallback Rc，资源归 seal scope；临时 getter scope 不接管资源。已 seal Style 的公共合并先物化字段以保留级联。
- 新增 8 项回归，覆盖零字段 seal、重复读取/矩形共享、临时调用 scope、fallback 派生值保活、seal 前景捕获、动态 transition、矩阵插值、矩形 setter 失效与已 seal 样式合并。既有动态新增/撤销字段、相等抑制及失败清理测试通过。
- 最终实际通过 cargo check --workspace --all-targets、cargo test --workspace（含跨 crate 与 doctests）、release theme_projection_demand、release cpu_performance_baseline；命令使用显式 manifest。性能记录保留同机变更前后 p50/p95、资源与分配结果；不把单次阶段耗时当百分位结果。未执行图形/平台测试。

## 2026-11-03：静态 Instant 快速路径

- 只有 Prop::Static(Transition::Instant) 直接复用原 Prop；动态曲线即使当前为 Instant 仍创建 watcher，保留后续切换动画能力。矩阵静态路径在列数组转换前返回。
- 新增 scalar/matrix 静态 bypass、无 presentation signal/tween、dynamic Instant→Linear 插值回归和 ignored 同 binary build/layout/drop 对照基准。
- 最终 cargo check --workspace --all-targets、cargo test --workspace、转场专项和 lazy projection 专项均通过；release instant_transition_baseline 通过。实际测量及其边界见性能记录。

## 2026-11-03：Taffy 聚合计算合并

- build_taffy 不再为 gap、size/min/max、overflow、margin/padding/border/inset 分别创建中间 computed；最终 Taffy Style 直接读取字段 Prop 并组装。每字段投影、transition 与最终 computed 相等值抑制保持。
- 新增 5 项 layout 回归，验证多字段 batch 更新最终 observer 仅通知一次、Px/Vw/Vh 等价抑制、空动态主题新增/删除字段恢复 fallback、矩形四边 transition 同步及资源清理。
- 验证通过：workspace all-targets check 与完整 workspace tests，含 doctests；改动文件 rustfmt --check、git diff --check 均通过。原有八场景 release baseline 通过，但本次与紧邻前一次运行的所有绝对 p50 均变化，包括 no-theme fast path；不将跨运行耗时差异归因于代码改动。资源/语义证据及复现命令见性能记录。

## 2026-11-03：布局变慢排查

- 同一份未重编译的优化后二进制固定 CPU 重跑，空主题 p50 从历史 52.45 us 回到 30.8–32.4 us，证明跨运行条件足以产生明显变化；具体物理原因未测定。
- 新增仅测试编译的 legacy Taffy 路径、最终 computed/实际 set_style 写入计数，以及 CPU 0 同 binary 交替顺序 A/B 基准（3 轮，每路径 2,000 构建与 2,048 更新）。每步比较 Taffy Style、实际布局尺寸、消费者计数及清理后资源。
- 配对比较中动态场景 folded 较快，无主题静态场景约 0.5–0.7% 较慢且闭包多 104 bytes；未发现历史约 1.7 倍整体回退。默认 dynamic transition 的 6 次最终计算/布局写入在两路径均存在，显式 batch/静态 Instant 的单次通知结论不能外推到所有主题更新。
- workspace all-targets check、完整 tests、release 配对基准和格式/diff 检查通过。参考实现/计数均 cfg(test)，不增加普通应用运行期资源；细节见性能记录。

## 2026-11-03：transition presentation 合并

- transition 输出改为 source/frame 显式依赖的 equality-aware computed，避免各 watch 单独写 signal 导致布局观察到逐字段中间状态。Tween 保存当前呈现值，advance 仍批量发布帧更新。
- core SignalInner 增加 u64 通知版本与 doc(hidden) untracked reader；每次 on_update 在通知订阅者前递增。frame 不会把 NaN 目标误判为新 source，也不把纯静默写当作重定向；无新增每属性 signal/effect。
- 新增 6 项回归：core 通知版本语义、动态 Instant 主题多属性新增/删除、动画批量布局、动画中断与曲线快照、batch 内同步读取与更新 metadata、NaN 目标完成。既有矩阵、动态曲线、失败清理与外部源生命周期测试保持通过。
- 实际通过：显式 manifest 的 cargo test --workspace（含跨 crate 和 doctests）、cargo check --workspace --all-targets；release transition_publication_compare 三轮同 binary/CPU 0 交替 A/B；原有八场景 release cpu_performance_baseline。动态默认曲线更新从六次最终布局计算/写入降至一次，p50 约快 21%；空主题构建 p50 约慢 10%，多 1,043 分配字节/构建，signals/effects 不变。完整数据与 shared-core 测量边界见性能记录。
- 本次未运行图形或平台交互测试；未改全局 scheduler 的调度策略。旧 transition 参考实现仅 cfg(test)，用于可复现对照。

## 2026-11-03：延迟 transition 帧资源

- transition binding 保持一个 source tracked computed；绑定入口捕获原 scope，但只有 source 实际换值且曲线有效、值可插值时才分配 frame signal、Tween 和 driver。Instant 不分配帧资源；之后改用动画会在原绑定 scope 创建并复用同一资源。规则输出、Theme、options、hover 与动态 transition 曲线都仍保留响应式 source。
- 增加回归覆盖真实动画前无 driver、首次动画的 frame 同次建立依赖、临时 caller scope 触发时资源仍归绑定 scope、animated/Instant 往返不重复分配、动画完成与 dispose 清理、NaN、打断动画、动态 Instant 切换、同步 batch 读取与 update metadata。
- 真实 Playground 十页仍 eager 构建并保留；仅跳过未动画属性的 frame signal。优化前后 signals 60,308 -> 44,840，effects 保持 44,913。Scrolling 页仍是最大静态页面资源来源，通用 Pages 契约未更改。
- 实际通过：显式 manifest 的完整 cargo test --workspace（含跨 crate 和 doctests）、cargo check --workspace --all-targets、release deferred_tween_compare 三轮同 binary/CPU 0 交替 A/B。代表主题 leaf 构建/布局/销毁 p50 约快 14%，每节点少 33 signals、66 allocations 和 4,468 allocated bytes；无活动动画的每帧 driver 扫描 p50 约 25–27 ns，eager 约 362–364 ns。release cpu_performance_baseline 八场景、theme_projection_demand 与 instant_transition_baseline 均通过，动态新增/撤销与 fallback/相等抑制及资源回收保持。真实 Playground 主题/缩放/选页单次交互也运行通过；临时采样钩子已移除。精确范围和测量限制见性能记录。
- 本轮未改 Pages 生命周期或全局 scheduler；旧 eager transition reference 仅编译进 test binary。

## 2026-11-03：空 resolver 结果通知抑制

- 总 resolver 仍每轮追踪 Theme/class、匹配 selector、执行 rule body、更新依赖和替换 generation owner；只有新旧 Style 都完全没有视觉声明时才跳过下游通知。任何非空转换均通知，不深比较动态 Prop。删除规则仍恢复 fallback，空主题以后新增规则仍生效。
- core 新增 doc(hidden) computed_with_notify：始终保存新输出，只由回调选择通知，现有 computed 语义保持不变。回归覆盖不通知时依赖分支切换与新缓存值，空 rule 生成资源按代替换、后续命中与撤销、无 prepare/mount 重跑及销毁后资源回基线。
- Playground 背景、面板和前景三类颜色共享三个线程级派生 signal；dark_theme 直接交出既有只读句柄。测试验证三份投影仅初始化一次、重复调用不新增资源和暗/亮主题颜色正确。
- 完整 workspace tests、workspace all-targets check、Playground 专项、release cpu_performance_baseline 与三轮 empty_resolver_compare 通过。16节点、512次无关主题更新的投影次数 385,024 -> 0，resolver 仍各 8,192次，p50 约快35%；相关规则控制组基本持平。空结果路径分配次数增加，具体数据与依赖检查原因见性能记录。
- 真实 Playground release 12次换肤去除前2次预热，同步更新 p50 约7.79ms、范围6.98–9.63ms，随后的 App::frame 为0.46–0.65ms。不是与历史单次13.4ms的严格A/B；该采样早于最后去除 dark_theme identity mapping。临时探针已移除，十页构建与保留语义不变。

## 2026-11-03：Check 依赖遍历取消临时快照

- core Check 路径不再依赖 dependencies.to_vec()：短借用取单个 SignalId，释放所有 arena 借用后递归检查 parent；递归返回重新检查 effect 存活和状态。新增 dependency_revision 覆盖动态 tracker 的取出/写回及 signal dispose 时实际依赖移除，版本变化从零重扫，避免依赖列表压缩后漏检。
- 新增3项确定性回归：递归 parent 删除多个已遍历依赖后仍检查末尾 parent；递归销毁当前检查目标；嵌套强制重跑替换依赖并尊重 Clean 状态。测试在 batch 内主动拉取结果，避免后续 pending flush 掩盖漏检。
- 最终完整 workspace tests（含跨 crate/doctests）、all-targets check、core release check_traversal_compare 三轮同 binary 交替 A/B、release empty_resolver_compare 与八场景 pipeline baseline 通过。正式代码无旧快照分支；对照仅 cfg(test)。
- core 47依赖的相等抑制场景，每2048次更新分配110,592 -> 12,288，p50 2.108–2.116us -> 1.629–1.653us（约快22%）；真正值变化的控制组基本持平。空主题16节点/512次更新基准的两种通知策略如今分配次数均54,784，历史空结果路径767,488次分配的问题已消除。每effect额外一个u64依赖版本，signal/effect数量不变。
- 最终耗时数据来自独立运行；曾并行启动两个固定CPU 0基准的结果不用于耗时结论。本轮无新增真实图形交互测量。复现与完整边界见性能记录。

## 图形 smoke 与平台边界

- cargo run -p atlas -- --frames 4 --glyphs 100 --max-size 28 --verify：通过，内嵌字体实际绘制 4 帧；oversized/exhausted/raster rejected 均为 0。
- style-example --frames=3：宿主未提供默认字体，cosmic-text 报 no default font found；该项不计为通过。使用 atlas 内嵌字体 smoke 覆盖基础节点迁移后的真实绘制路径。
- Linux X11 enter/leave 使用已有平台事件来源，Macroquad 队列/重放桥接已修复；Wayland 新增 pointer enter/leave 转发，host 编译通过，但未做 Wayland 会话人工交互。
- 浏览器新增 enter/leave exports 和鼠标 PointerEvent 转发，触摸兼容鼠标过滤；JavaScript 语法检查通过，未做 wasm target 编译或真实浏览器交互。
- Windows 新增 PT_MOUSE client tracking、WM_MOUSELEAVE/pointer enter/leave；macOS content view tracking area 支持拖动中 entered/exited。两个平台已有焦点转发保留，源文件经格式/语法解析；宿主未安装对应 Rust targets，未声称通过其目标编译或窗口实测。

本轮未引入实例 ID、结构组合器、局部 provider、多指针、非 Clone option 或借用 accessor。外部 signal 仍由其原 scope 管理，节点销毁不销毁外部源。
