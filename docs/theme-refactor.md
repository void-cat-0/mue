# Theme 重构设计定稿：从 Factory 到声明式主题目标

> **状态：首版重构已实施。设计定稿：2026-10-31；文档同步：2026-11-02。**
> 实际执行的检查与平台边界见 [实施记录](theme-refactor-validation.md)，测量条件与结果见 [性能基线](theme-refactor-performance.md)。
> **实施入口：[Theme 重构实施规格](theme-refactor-implementation.md)。** 本文提供决策摘要与 Q 索引，类型、阶段、迁移和验收以实施规格为准。
> [讨论历史](theme-refactor-history.md) 保留原讨论、未采用方案和 ID 设想；归档不构成本轮要求。

## 1. 目标

删除 Factory / FactoryInstance / FactoryUse，让 node/component 使用共同构建协议，并在显式 opt-in 时直接成为主题目标。参数声明是组件 API 与主题 options 的共同来源，不再维护值级 factory identity 或第二套 defaults/options。允许大幅修改声明和内部结构，无兼容层。

首版主题 API 已在当前代码中实现。已实施内容与自动验证、图形 smoke 及平台覆盖边界见 [实施与验证记录](theme-refactor-validation.md)；此处保留设计决策，不代替验证报告。

## 2. 首版决策摘要

### 2.1 声明、identity 与 options

- 仅函数级 #[theme] / #[theme(...)] 启用主题身份；普通 node/component 没有 ThemeTarget/options 实例/resolver。
- builder 直接实现 ThemeTarget，CamelCase alias 是同一类型，不生成独立 marker；identity 是具体 builder 的 TypeId，仅当前进程内有效。
- 泛型按具体实例化区分，例如 Badge&lt;u32&gt; 与 Badge&lt;u64&gt;；selector 是 Rust 类型路径。开放泛型 option 首版拒绝，不与泛型 identity 混淆。
- options 使用显式 allowlist；裸 #[theme] 的 Options=()，不生成空 struct。主题值满足 PropValue + Clone + 'static，所有无效标注必须编译期诊断。
- 所有权方案 B：默认值只求值一次，clone 生效 Prop 给 owned options，原值 move 给函数；无顶层主题跳过 options 构造和 clone。
- 字段私有，公开 inherent accessor 返回 tracked owned 值；可统一 get_clone()，不要求宏猜任意类型是否 Copy。非 Clone 与借用 accessor 后置。
- model 决定函数侧动态写模型，列入 allowlist 才以只读 Prop::Dynamic 暴露；未列出的内容/回调/资源不主题化。

### 2.2 ClassName 与 selector

- ClassName 私有字段保证格式 [A-Za-z][A-Za-z0-9_-]*；class! 编译期校验字面量，运行时 String 经 fallible 构造返回 Result。不检查名称是否声明或注册。
- 入口只保留 .classes(impl Into&lt;Prop&lt;HashSet&lt;ClassName&gt;&gt;&gt;)：默认空集合，后调用完整替换先输入及其绑定，无隐式追加/并集，无单 class 布尔绑定。
- 不同共根 target 保留各自 class/options；compound 的条件不能跨 target 拼接。动态增删只改外部 Signal。
- 首版仅 compound 和逗号 list：identity → class* → option? → hover?，支持 .class/.!class/$option/:hover；普通 Rust 标识符不改写。
- 每个 $ 语法出现增加一份 CLASS 权重；specificity 按 (ID,CLASS,TYPE) 字典序比较，首版 ID=0；逗号分支独立计权。

### 2.3 统一构建、children 与级联

- 顶层 AppBuildContext 携带 Prop&lt;Theme&gt;，每个最终真实 Node 独立拥有 RootBuild，二者分离。
- component 函数体属于 prepare，返回 builder，RootBuild 只沿返回链传递；node 函数体显式准备默认值/资源并返回 NodePlan(defaults,mount)，宏不猜阶段、不重跑函数。
- 同根链 prepare → seal → mount；seal 固定贡献和绑定结构而非动态值。transition/font/transform/layout/paint/hooks 与注册只在 seal 后建立一次。
- .children(...) 保存计划；父 seal 并建立 mount 环境后，各 child 独立构建，再接父 layout。动态 child 恢复同一主题输入，prebuilt Node 直接接入。
- 每属性优先级：显式样式 &gt; theme &gt; prepare fallback &gt; 框架默认/继承。同 builder 后写覆盖先写；component 外层显式值覆盖返回 builder 的值。
- Theme 用有序 Rule/Selector 保存稀疏声明和 specificity，规则数组位置决定 source order；统一级联所有共根 target，同一规则只形成一次候选，多分支用实际命中最高权重，不乘命中数或加 target 顺序权重。
- 动态换肤可新增首次未命中属性、删除规则；稳定绑定必须支持新增命中和 fallback 恢复，不按首次结果裁剪字段。

### 2.4 顶层主题与 hover

- 主题唯一入口 App::with_theme(impl Into&lt;Prop&lt;Theme&gt;&gt;,build)，早于 root 构建；删除 ThemeSource 与局部/嵌套 provider。规则模块由顶层有序组合。
- Themeable: ThemeTarget+Styleable 提供 classes/enable_hovered；StyleableExt:Styleable 提供 with_hovered，不因调用创建主题身份。
- with_hovered 当场创建/复用初值 false 的状态并执行一次配置回调，无额外 'static 约束、不延迟 prepare；主题 :hover 不自动使能。
- NodeInner.hovered:Option&lt;Signal&lt;bool&gt;&gt; 是真实 Node 的统一状态；指针系统写、主题/配置只读；提前状态与 scope 移交，已经交出的多个共根句柄必须继续有效。
- 命中为最上层目标及真实 Node 祖先路径，被遮挡兄弟不同时 hover；使能不影响遮挡，透明度不隐含穿透。
- 鼠标按下/拖动仍按位置更新，和捕获接收方独立；触摸与模拟鼠标无 hover；离窗/失焦清除，恢复后确认位置。
- 布局/transform 后重命中，静止光标响应界面变化；hover 改布局交下一轮检查，不同步递归求稳定点。首版单鼠标路径。
- NodeRef::hovered() 返回 Option&lt;ReadSignal&lt;bool&gt;&gt;，不分配/启用/保活；未启用或已销毁返回 None。

## 3. 设计基线与首版范围

以下内容作为首版设计与实现基线；实施状态及宿主验证边界以实施记录为准：

- **Q03**：正确生成泛型声明/实参，限定主题能力的 'static 边界；具体 alias/关联类型由 Rust 正向 trait 证明。宏不能承诺透视任意 alias 和自定义字段。
- **Q06**：PropValue + Clone + 'static，方案 B 不变；非 Clone 不进入首版，不重做所有权模型。
- **Q15**：NodePlan 与消耗式 prepare 协议；scope/context 使用 owned 关系和 RAII guard，不依赖正常路径 push/pop。内部名字可调整，不能退回宏自动拆旧函数体。
- **Q23**：共享总 resolver + 稳定字段绑定，可比较最终属性用 computed 抑制重复通知；无主题 fast path 保留。规则索引、完全静态路径、per-class、interning 等测量后再做。
- **旧入口**：factory、旧 first-match theme callback/match 管线与嵌套 provider 已从首版 API 移除；主题只从应用顶层接入。

## 4. Q 编号结案索引

保留旧锚点用于追溯。设计已定稿；具体实现与宿主覆盖范围见实施记录。

<a id='questions-identity'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q03 | 泛型 identity/路径/alias 定稿，编译测试验证边界 | §2 |

<a id='questions-options'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q06 | 方案 B、PropValue+Clone+'static；非 Clone 后置 | §3 |
| Q09 | owned accessor，借用后置 | §3 |

<a id='questions-cascade'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q12 | 逐属性优先级、外层覆盖、同层顺序与共根去重定稿 | §7 |
| Q14 | selector list 独立权重/source order，无 provider 权重 | §7–8 |
| Q15 | 显式 prepare/seal/mount、NodePlan/RootBuild 定稿 | §5 |

<a id='questions-children'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q16 | children 计划与独立构建并入 Q15 | §6 |
| Q17/Q18 | 结构 selector 的目标树/祖先/兄弟语义后置；动态 child 基础保留 | §1、§6 |

<a id='questions-class-hover'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q19 | ClassName/class!、运行时 fallible 构造定稿 | §4 |
| Q20 | API/状态/命中/输入/观察语义定稿；canonical/alias 同批更新与清理已接入 | §9 |

<a id='questions-id'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q21/Q22 | **实例 ID 暂不实现**：无 setter、selector、registry 或 claim/release | §1 |

<a id='questions-performance'></a>

| Q | 结论 | 实施章节 |
| --- | --- | --- |
| Q23 | 稳定绑定、比较抑制、基准场景定稿；优化后置 | §10 |
| Q24 | compound/list 支持 identity/class/option/hover，无 ID/结构组合器 | §8 |

<a id='open-questions'></a>

## 5. 后续范围

首版按上述设计边界实施。实例 ID、结构组合器、非 Clone option、借用 accessor、class 注册与多指针仍后置；历史 ID 唯一性和 claim/retire 设想不要求改写 keyed 生命周期。逐项验证结果、未覆盖的平台交互和运行限制以 [实施与验证记录](theme-refactor-validation.md) 为准。

## 6. 文档分工

- [实施规格](theme-refactor-implementation.md)：首版目标行为、内部协议、代码落点与实施核对清单。
- 本文：定稿摘要与旧 Q 映射，不重复维护完整接口代码。
- [讨论历史](theme-refactor-history.md)：冻结追溯，不再追加规范。
- [主题教程](themes.md)、[可复用组件声明教程](factories.md)、[README](../README.md)：已迁移到新 API；原 factory 教程保留 URL，内容改为声明式组件用法。
- [实施与验证记录](theme-refactor-validation.md)：记录执行过的检查、结果和平台覆盖边界。
- [性能基线](theme-refactor-performance.md)：记录独立测量条件、数据与复现方式。
