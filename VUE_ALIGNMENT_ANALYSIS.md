# Mue 与 Vue 3 的逻辑、数据模型差异及对齐方案

> 分析日期：2026-10-18  
> Mue 基线：`font_atlas_refactor` 分支当前工作区，包含已有未提交修改；HEAD 为 `2afbef8`。  
> Vue 基线：Vue Core **v3.5.22**（commit `5a8aa0b2ba575e098cbb63b396e9bcb751eb3a0f`）；概念说明参考 Vue 官方文档。此版本用于固定比较口径，不表示它是分析日期的最新版本。  
> 范围：响应式数据、组件、更新调度、生命周期、父子通信、动态子树和应用运行模型。本文是分析与设计建议，不是已实现的改造说明。  
> **实施状态（2026-10-18）：** 下文保留改造前基线；组件构建 Scope、effect owner、Reaction 生命周期、cleanup/异常恢复和循环诊断已在第一阶段实现，详见 [第一阶段落地记录](VUE_ALIGNMENT_PROGRESS.md)。其余建议不代表已经实现。

## 1. 结论先行

**Mue 的声明式 API 受到 Vue 启发，但当前运行机制更接近“细粒度 Signal + 一次性组件构建 + 保留式节点树”，并不是 Rust 版 Vue 运行时。**

它已经具备 Vue Composition API 的部分基础：可写状态、只读状态、派生值、自动依赖追踪、watch、作用域、依赖注入、组件参数和类似双向绑定的能力。但以下语义差异会直接影响业务正确性：

1. **组件没有独立且统一的实例生命周期。** `#[component]` 生成 builder，不会自动建立组件自己的 Scope；局部状态可能归属外层作用域。
2. **更新时机不同。** Mue 默认同步传播，只有显式 `batch` 才合并；Vue 组件更新和默认 watcher 通常通过微任务队列合并。
3. **同 key 不代表同样的更新行为。** Mue `map_keyed` 复用节点但不自动刷新 item；Vue 同类型、同 key 时保留实例，同时更新 props。
4. **条件显示语义不同。** Mue `show_if` 保留节点、将其移出活动 children；不是 Vue `v-if` 的卸载重建，也不完全等同 `v-show`。
5. **双向绑定的数据边界不同。** `#[model]` 直接共享可写 Signal；Vue `v-model` 是 prop 与 `update:*` 事件构成的通信协议。
6. **状态粒度不同。** Mue 追踪整个 Signal，Vue `reactive` 能追踪对象属性、集合访问等更细粒度操作。

**推荐对齐 Vue 的开发者语义，而不是首先复制它的 VDOM、JavaScript Proxy 或 SFC 编译链。** 保留 Rust 强类型、Taffy 布局和 Macroquad 渲染，在其上建立明确的组件所有权、更新阶段、可更新列表项、受控 Model 与生命周期协议，收益更高、风险更小。

---

## 2. 当前项目的实际运行模型

### 2.1 分层

| 层 | 当前职责 |
| --- | --- |
| `mue-core` | Signal、ReadSignal、Prop、computed、watch/effect、Scope、batch、更新上下文 |
| `mue-macros` | `#[component]`、`#[node]`、`theme!` 等过程宏 |
| `mue-macroquad` | 节点树、样式、布局、事件、主题、导航、动画、文字与 GPU 绘制 |
| 示例 | 展示页面、导航、事件、列表、主题、图集等使用方式 |

当前数据与画面的连接大致为：

```text
Rust 组件函数 / builder
  → 创建并持有 Node、Scope、Signal、布局/绘制 hook

Signal 改变
  → 依赖图标记失效
  → 同步 flush，或在显式 batch 结束时 flush
  → computed / watch_effect 更新布局、文字缓冲或其他节点状态

App::frame()
  → viewport 与动画更新
  → timeout
  → 字号解析与 Taffy 布局稳定化
  → 遍历并绘制现有节点树
  → pointer / wheel 输入处理
```

Vue 3 的典型组件流程则是：

```text
创建组件实例
  → setup 执行一次，建立状态和副作用
  → render effect 首次运行
  → 创建 VNode 并挂载宿主节点

响应式状态 / props 改变
  → 调度 watcher / 组件更新任务
  → render effect 重新执行
  → 新旧 VNode patch
  → 宿主节点更新
  → post 回调 / nextTick
```

**重要澄清：Vue 更新时通常重新执行的是 render，不是 setup。** Mue 组件函数只执行一次本身不是错误；真正不同的是，Mue 缺少 Vue 那样明确的“组件实例 + 可更新 props + 渲染更新契约”，主要由各个细粒度订阅完成更新。

主要代码依据：

- `mue-macros/src/component.rs:192`：`#[node]` 与 `#[component]` 的展开分支。
- `mue-macroquad/src/node.rs:38`：节点内部数据。
- `mue-macroquad/src/node.rs:253`：节点构建与节点 Scope。
- `mue-macroquad/src/app.rs:45`：帧循环。
- `mue-macroquad/src/layout.rs:80`：响应式属性同步到布局系统。

---

## 3. 数据模型差异

### 3.1 状态、追踪粒度与相等性

| 维度 | Mue 当前行为 | Vue 3 行为 | 对齐建议 |
| --- | --- | --- | --- |
| 基础状态 | `Signal<T>` / `ReadSignal<T>` 句柄 | `ref`、`reactive` 等 | 保留 Rust API，先对齐契约 |
| 依赖粒度 | 一次 tracked read 订阅整个 Signal | ref 的 value、Proxy 的属性/集合操作 | 增加字段投影和 selector，而非先做完整 Proxy |
| 嵌套对象 | 不自动深度响应式转换 | 普通 ref 对对象做深层转换，reactive 深度代理 | 明确集合/结构体需要显式写入 |
| 写入相等判断 | 普通 `set` 用 `PartialEq` | ref 等替换判断基于 `Object.is` 语义 | 允许显式 comparator，避免假装完全一致 |
| 原地更新 | `update` 无条件通知 | 通过 Proxy 拦截具体字段或集合操作 | 增加可选的条件通知策略 |
| 浅层状态 | `Signal::shallow` 每次 set 都通知 | `shallowRef` 不深转内部值，但替换仍比较 | 改善命名与文档，不能一一映射 |
| 跳过追踪 | 有 untracked 读写 API | 有原始对象/浅层/手动触发等逃生接口 | 解释“不订阅”和“不通知”的区别 |

代码依据：`mue-core/src/signal.rs:168`、`:212`、`:231`、`:245`、`:259`、`:284`。

#### 业务上的具体影响

假设把整个业务状态放到一个 Signal 中：

```rust
// 示意：User 需要满足对应 API 的类型约束。
let user = signal(User { name: "Alice".into(), age: 18 });
```

订阅 `user` 的消费者在 `age` 修改时也可能重新运行，即使只关心 `name`。Vue 则可以仅追踪被读取的属性。

此外：

- `signal.get()` / `get_clone()` 在依赖收集环境内建立订阅；普通 Rust 引用或内部可变性并不会自动变成响应式。
- `update(|_| {})` 即便不改变数据也通知；`set` 相等值不通知。
- 集合和 `Rc` 的 `PartialEq` 可能进行内容比较，而不是对象身份比较。
- 浮点 `NaN`、正负零的 Rust 相等性与 `Object.is` 不一致。

**建议契约：** 默认采用 Rust 值语义，但支持 `signal_with_eq` / identity comparator 等显式策略；若增加 `select` 或 `lens`，要说明它们通常只是抑制下游传播，不自动获得 Proxy 的字段级失效能力。

### 3.2 computed：已有缓存，但计算时机不一样

Mue 的 `computed` 并不是每次读取都重新计算：它已经缓存结果，并在结果相等时阻止后续有效传播。真正的差异是：

- 创建 computed 时立即执行。
- 上游变化后，computed 会进入更新流程；即使没有下游读取者，也可能主动重算。
- batch 中读取失效 computed 时，会按需刷新依赖链。
- Vue computed 通常在首次读取时计算，之后按需刷新缓存；依赖改变不等于立刻无条件执行 getter。

代码依据：`mue-core/src/effect.rs:236`、`:266`；`mue-core/src/runtime.rs:204`、`:257`、`:281`。

**不能直接把现有 computed 改成惰性。** 当前项目也用 computed 持有节点、维护列表和组织 children；这些闭包不全是纯数据计算。改变求值时机可能连带改变建树与资源释放时机。

建议先拆分职责：

1. `computed`：纯派生值，目标为惰性、缓存、输出相等性控制。
2. `watch_effect`：有副作用的订阅。
3. 独立的结构更新任务：创建/销毁节点、维护 keyed children。
4. 若仍需要主动求值，保留明确命名的 eager memo，而不是把所有职责混在 computed 中。

### 3.3 watch / watchEffect

Mue 已支持动态依赖收集：每次 effect 运行重新收集读取项，并解除旧依赖中的失效订阅。这是与 Vue 相近的基础能力。

当前 API 的对应关系：

- `watch(signal, callback)`：固定订阅一个 Signal，注册时不调用 callback。
- `watch_immediate`：固定订阅并立即调用一次。
- `watch_effect`：立即执行，并动态追踪本次执行的读取。

但尚不等价于 Vue watch：

- 不提供统一的 getter source、多 source、old/new value 参数与 flush 选项。
- 没有显式 deep / once / pause-resume 等较完整的观察协议。
- watch 自身不保存旧值做最终比较。普通 Signal 在 batch 中 `A → B → A`，watch 仍可能执行一次；Vue 的普通标量 ref watcher 通常比较最终值后跳过。deep/force-trigger 等情形需另论。
- Vue 的 watch callback 也不会因为回调内额外读取而增加 source，这一点 Mue 已经相近。

代码依据：`mue-core/src/effect.rs:133`、`:159`、`:187`；`mue-core/src/runtime.rs:138`。

建议引入 source 抽象与 `WatchOptions`，先支持：getter、old/new、immediate、flush、cleanup、stop；deep watch 对当前非 Proxy 数据模型并不是简单加一个布尔开关，应后置。

---

## 4. 组件、props 与父子通信

### 4.1 组件函数和实例不是同一回事

当前 `Component` 主要是 `IntoNode + Styleable + StyleableExt` 的 trait 组合。`#[component]` 生成 builder，构建时调用函数，把 style 合并到返回根节点；没有独立的组件实例记录。

`#[node]` 会进入 `Node::build` 或 `build_with_style`，它与 `#[component]` 的 Scope 语义不同。

这会产生一个重要问题：**组件函数在根 Node 建立之前创建的 Signal、computed、effect、甚至提前创建的子节点，可能被登记到外层 Scope，而非组件自身。** 如果当前没有有效 Scope，则没有自动的组件级资源回收归属。

代码依据：

- `mue-macros/src/component.rs:192`–`:209`。
- `mue-core/src/signal.rs:93`、`mue-core/src/effect.rs:104`：资源登记到当前 Scope。
- `examples/tab/src/main.rs:77`：组件函数中创建局部交互状态的实际用法。

#### 推荐最小组件模型

新增轻量 `ComponentInstance`，至少持有：

```text
id / type 标识
owner scope / parent component
mount 状态与生命周期回调
根节点或逻辑子树句柄
组件级诊断信息
```

完整的 props patch 容器和 render effect，可以根据是否采用 VDOM 逐步增加，不必在第一版全部实现。

宏应让**组件函数整个执行过程，以及返回 builder 的物化过程**都发生在组件 Scope 内。不能只在最后给根 Node 包一个 Scope：函数体中提前创建的资源已经发生归属。

组件 Scope 必须由组件句柄/根树真实持有，直到卸载；不能函数返回就释放。它应是逻辑边界，不强制生成影响布局的包装节点，也不应为了实现它破坏主题与 style 继承。

### 4.2 Props：已有单向只读入口，但缺少统一更新协议

Mue 的 `Prop<T>` 是：

```text
Static(T)
Dynamic(ReadSignal<T>)
```

因此：

- 传 Signal 或 ReadSignal 可以保持动态连接。
- 传 `signal.get()` 是当前值快照，不会自动变成动态 prop。
- 是否持续更新，取决于组件是否在 computed、effect 或逐帧 hook 中读取该 Prop。
- builder setter 是构建期赋值，不是挂载后对同一组件实例 patch props。

代码依据：`mue-core/src/prop.rs:29`、`:78`；`mue-macros/src/component.rs:118`。

Vue 的组件 props 会随着父级渲染刷新。顶层绑定遵循单向数据流，但这不等于所有嵌套对象都不可变：Vue 的保护是浅层的，共享对象内部仍可能被修改；开发环境还提供 readonly 包装和告警。

**保留细粒度架构时，不必强制复制 props 字典。** 可以继续使用强类型 Prop，但必须约定：动态参数在完整挂载周期内是持续有效的输入，组件实现不得意外把它降为快照。动态列表项也必须能更新同实例输入。

### 4.3 `#[model]` 与 Vue `v-model`

当前 `#[model]` 把参数类型转换成 `Signal<T>`；子组件直接调用 `set` 修改共享状态。它更接近“父级授予可写状态句柄”。

Vue 的默认协议是：

```text
父级下传 modelValue
子级提出 update:modelValue(newValue)
父级监听器决定如何更新源状态
```

`defineModel()` 为这个协议提供 ref-like 使用体验，并不是简单把父级 ref 本体交给子级。命名 model、修饰符与无父级绑定时的本地值，也有明确行为。

直接共享 Signal 很方便，但验证、拒绝写入、转换值、日志和权限检查容易散落在组件内部。

建议增加受控 Model 抽象，例如以下**目标 API 草图，不是现有 API**：

```rust
struct Model<T> {
    value: ReadSignal<T>,
    request_update: Rc<dyn Fn(T)>,
}
```

`Model::set` 的语义是提出更新请求，只有父级接受后，`value` 才变化。实际 API 可进一步接受 `Prop<T>`，支持静态输入与本地 fallback，但要明确受控/非受控模式，避免悄悄产生两份不同步状态。

现有 `Signal<T>` 可以适配成 Model 以平滑迁移；保留共享状态模式时，应明确命名，而不是宣称它已经等同于 Vue `v-model`。

### 4.4 Slots、组件事件和原生输入

- Mue children 是节点或节点列表的组合，已经能实现简单内容插槽。
- 但它不是 Vue 那种可在特定上下文调用的 slot 函数协议，尚缺明确的命名 slot、作用域 slot 参数与调用生命周期。
- 可先用强类型闭包定义 `Slot<Props>`，并为动态 slot 建树约定 owner Scope；不必马上做模板语法。
- Mue pointer/wheel/gesture hook 是画布输入系统，不能等同组件自定义事件。
- Vue 自定义 `emit` 不向祖先自动冒泡；它与 DOM 原生事件传播是两套机制。

建议使用类型化 `Callback<T>` / `Event<T>` 作为组件输出，同时保留现有 pointer/gesture 系统。要区分组件事件、命中测试、手势竞争和输入消费，不应为了“像 Vue”把它们混成一个冒泡模型。

---

## 5. 动态节点：最容易出现可见差异的地方

### 5.1 `map_keyed`：保留实例之外，还需要更新输入

当前行为见 `mue-macroquad/src/node/layout/children.rs:66`–`:111`：

1. 按 key 查找旧节点。
2. key 存在则直接复用，不再次调用 `node_fn`。
3. 新 key 才创建新节点。
4. 消失的 key 对应节点被释放。

源码也明确说明：保留节点保持原始内容和局部状态。

例子：

```text
第一轮：{ id: 1, title: "旧标题" }
第二轮：{ id: 1, title: "新标题" }
```

如果节点只在首次构建时复制 `title`，Mue 中第二轮可能仍显示“旧标题”。如果 item 本身包含共享 Signal 且节点订阅了它，则可以更新，但那是用户提供的响应式连接，不是 `map_keyed` 自动 patch 了 item。

Vue 对同类型、同 key 的子组件，会保留实例，同时更新 props、运行必要的 render 更新；更换 key 或类型才替换实例。

#### 推荐方案：每个 key 维护 item Signal

引入新的列表 API，而不是悄悄改变旧函数：

```text
key → { item_signal, node, item_scope }
```

流程：

- 新 key：在 item Scope 内创建 Signal 和节点，将只读 item 信号传给构建回调。
- 已有 key：更新 item Signal，不丢失组件局部状态。
- 重排序：只更新 children 顺序，不重建。
- 删除：卸载节点并释放 item Scope。
- 重复 key：debug 阶段明确报错。
- 合并更新：在一次结构更新事务中完成，避免消费者看到新旧列表混合状态。

需要为 `T` 的复制/所有权和相等比较设计约束，例如 `Rc<T>` 或明确的 `Clone + PartialEq` 版本；不能为了省事隐式要求所有业务类型都廉价复制。

现有 `map_keyed` 可保留为显式“构建后内容不自动更新”的低级接口，并标明它的适用条件。

### 5.2 条件分支要分清“销毁”和“保留”

当前 `show_if(condition, node)` 接收已经构建的 Node。隐藏时返回空 children，但通过闭包保留原 Node，所以状态和副作用可能继续存在。

| 语义 | 初始 false 是否创建 | 隐藏时是否卸载 | 状态 |
| --- | --- | --- | --- |
| Vue `v-if` | 不创建该分支 | 是 | 重新显示通常重新创建 |
| Vue `v-show` | 创建 | 否，只切换 display | 保留 |
| Mue 当前 `show_if` | 已创建，因为传入 Node | 不释放，但移出活动 children | 保留 |

代码依据：`mue-macroquad/src/node/layout/children.rs:114`。

建议拆成两类 API：

- `if_branch(condition, factory)`：接收惰性 factory，显示时创建，隐藏时卸载和释放 Scope。
- `show(condition, node)` / `visibility`：保留实例，明确不参与布局、绘制、命中测试时的规则。

“保留实例”不等于“暂停副作用”。隐藏期间 effect、timeout、动画是否继续，必须单独规定。若需要暂停/恢复，应设计类似缓存激活状态的协议，而不是让 `show` 隐式承担这一职责。

---

## 6. 更新调度、生命周期与资源安全

### 6.1 默认同步传播与 Vue 默认异步批处理

Mue 在非 batch 写入后立即 flush；batch 则到最外层结束再处理。订阅者集合是 HashSet，不能据此承诺同一 Signal 的 watcher 按注册顺序执行。

Vue 的默认组件更新和 `flush: 'pre'` watcher 进入调度队列；`flush: 'post'` 用于更新之后，`flush: 'sync'` 则在变更时同步执行、不做通常的调度合并。

代码依据：`mue-core/src/runtime.rs:105`、`:281`、`:293`。

直接照搬 JavaScript 微任务并不适合 Macroquad 的帧驱动宿主。推荐分层：

- `mue-core` 提供 job 去重、停止、失效、阶段和可插拔调度策略。
- `mue-macroquad` 将 deferred UI job 对接到帧内确定的提交阶段。
- 保留显式同步模式，避免破坏低级使用者。
- 保留 `UpdateContext` 等更新元数据，不能因延迟执行丢失动画上下文。

一个可选的目标帧模型是：

```text
采集输入/时间/动画变化
  → pre 更新任务
  → 同步节点结构与布局属性
  → 布局稳定化
  → layout-ready 回调
  → 绘制命令提交
  → post-commit 回调
```

这不是可以直接搬动现有调用顺序的一行修改。当前输入在绘制后处理，命中测试依赖节点位置；若改为输入优先，必须规定使用上一份已提交布局，或者在必要时刷新命中测试几何，避免首帧、窗口缩放、transform 变化时出现错误。

### 6.2 不要把 `nextTick`、布局完成、屏幕显示混为一谈

Vue `nextTick` 等待当前 DOM 更新 flush，并不保证浏览器已经把像素显示到屏幕。

Mue 若增加类似 API，也应区分：

- `next_update`：本轮响应式/节点更新提交完成。
- `after_layout`：布局稳定，可读取几何。
- `after_frame`：本帧绘制提交完成；仍不等于 GPU/显示器已经呈现。

这些名称是建议，不是现有 API。若使用 `next_tick` 名字，必须写清对应哪个阶段，以及“当前没有待处理更新”时如何完成。

### 6.3 Scope、context 与生命周期

当前 Scope 具备父子关系、按类型 provide/inject 和显式 dispose。`Owned<T>` 提供 Drop 释放，但裸 `Scope`、`Effect`、`Signal` 是可复制句柄，不能把“丢弃句柄”当作资源停止。

Vue 组件自身有 effect scope；setup 中同步创建的相关副作用在卸载时停止。异步回调中晚创建的 watcher 也并非无条件自动归属组件，仍需要手动管理。

Mue 还有两个需区分的问题：

1. **资源 owner**：effect 属于哪个 Scope，应由创建和显式 owner 决定。
2. **执行上下文**：effect 重跑时 `inject` 和新资源创建使用哪个 Scope。

当前 Effect 没有保存 owner Scope，重跑没有自动恢复创建时的 Scope。`map_keyed` 等地方已经通过捕获 owner 手动补偿。这说明动态建树需要稳定的上下文，但不能因此声称 Vue 所有异步执行都会自动恢复 setup 环境。

建议：Effect 保存 owner，并在运行时使用明确的 owner guard；同时定义重跑中创建资源的归属和清理规则，避免每次运行积累子 effect。需要“每次重跑自动清理的运行期子 Scope”时，应独立设计，不能未经说明就改变所有 Scope 行为。

补充组件生命周期：`before_mount / mounted / before_update / updated / before_unmount / unmounted`。原生宿主里 mounted 应定义为逻辑挂载，几何读取另用 layout-ready hook，不应假装它与 DOM 插入完全相同。

Scope 级别还需要独立的 `on_scope_dispose`；现有 `on_cleanup` 是 effect 重跑/销毁回调，不能代替整个组件的卸载回调。

代码依据：`mue-core/src/scope.rs:50`、`:150`、`:157`、`:208`；`mue-core/src/effect.rs:54`、`:226`；`mue-core/src/disposable.rs:44`。

### 6.4 当前可见的健壮性风险

以下是源码中可见的保护缺口，**不代表本次已经通过运行复现每一种故障**，也不能因为现有测试通过就忽略：

| 风险 | 静态依据 | 建议 |
| --- | --- | --- |
| cleanup 中响应式操作可能 RefCell 重借用 | `runtime.rs:138` 持有 effect arena 可变借用执行 cleanup | 先移出 callback，释放 arena 借用后执行用户代码 |
| effect 执行中写自身依赖，重入/再排队语义不清晰 | callback 被取出，只有 Clean/Check/Dirty 状态 | 增加 Running、非重入 flush、明确自触发规则 |
| 循环更新缺少统一诊断 | flush 持续消费队列，无核心循环预算 | 按 flush 统计执行次数，输出依赖/创建位置 |
| panic 后运行时上下文可能未恢复 | `Scope::run`、batch、effect 通过正常返回路径恢复 | 使用 RAII guard；明确 unwind 后恢复或终止策略 |
| Reaction 所有权不完整 | `create_reaction` 没走 Effect 的 Scope 注册，也没有 Disposable/Drop | 统一 owner/stop，验证测量 Reaction 随节点释放 |
| update / set_with 用户闭包内嵌套读写可能重借用 | 回调在 Signal arena 可变借用期间执行 | 缩小借用范围，或明确禁止并提供诊断 |

准确落点为 `mue-core/src/runtime.rs:129`、`:159`、`:281`、`:293`，`mue-core/src/effect.rs:293`，`mue-core/src/signal.rs:259`、`:290`，以及 `mue-macroquad/src/layout.rs:35`。

这些问题应先于更大规模的 Vue API 仿制解决，因为它们影响任何响应式业务架构。

---

## 7. 应保留的差异，以及不应误认为已支持的能力

### 7.1 值得保留

- **强类型 builder 和 Rust 过程宏**：不必为了接近 Vue 就先引入 SFC/HTML parser。
- **细粒度响应式更新**：可以提供 Vue-like props、model、条件分支语义，不强制经过 VDOM。
- **Taffy + 原生绘制**：不是浏览器，不应该承诺完整 CSS/DOM 行为。
- **类型化 context**：比字符串 key 更安全；同类型多用途可以用 newtype 或 typed key。
- **画布手势竞争和滚动消费**：这是原生交互优势，不应被组件 emit 协议替换。

### 7.2 不能直接视为 Vue 等价物

- Mue `Factory` 更像可复用构造器与样式身份，不是完整组件实例。
- 主题层叠不是浏览器 CSS cascade，style 透传也不等于 Vue 完整的 attrs fallthrough。
- Mue `navigation` 是 typed path 与保留页面栈，不是 Vue Router 的 URL、history、路由守卫与懒加载协议。Vue Router 本身也是生态库，不是 Vue Core 必备组成。
- 全局 Signal / context 可以承载应用状态，但不等于 Pinia 的 action、订阅、开发工具与持久化生态。
- 文本/链接绘制不提供 DOM 输入焦点、表单、可访问性树；这些属于宿主能力，不是只增加 Vue 风格语法就会获得。
- SSR、hydration、SFC、HMR、Teleport、Suspense、KeepAlive、Devtools 不应纳入第一阶段，除非产品目标明确需要。

---

## 8. 推荐的对齐架构与实施顺序

### 8.1 推荐方向：语义对齐，保留细粒度后端

目标可概括为：

> Mue 是采用 Rust 与保留式渲染的 UI 框架，提供与 Vue Composition API 接近的组件所有权、单向输入、受控输出和更新阶段，但不承诺 Vue 模板或运行时兼容。

这一方向不要求所有状态变化都调用组件 render。细粒度订阅可以直接更新已保留节点，只要对外保证：输入持续有效、实例身份稳定、销毁可靠、阶段可预测。

### 8.2 分阶段实施

| 阶段 | 内容 | 主要代码位置 | 依赖与成本 |
| --- | --- | --- | --- |
| P0：契约和稳定性 | 记录同步/相等性语义；完善 guard、cleanup、Reaction owner、重入诊断 | `mue-core` runtime/effect/scope/signal | 改动集中，但需大量边界测试 |
| P1：组件所有权 | ComponentInstance、完整构建期 Scope、scope dispose、基础生命周期 | 宏、node、App、Factory/Theme 接入 | 中到高；先审计现有建树/销毁路径 |
| P2：结构与数据更新 | 新 keyed item API、惰性条件分支、明确保留隐藏语义 | children、node、组件测试 | 中；依赖 P0/P1 的资源规则 |
| P3：调度阶段 | 可插拔 scheduler、job 去重、pre/post、布局完成通知、停止失效任务 | core runtime、App、animation 上下文 | 高；需处理输入和布局时序 |
| P4：组件通信 | Model、Callback/Event、watch options、typed slots/context key | core、宏、组件示例 | 中；异步 flush 选项依赖 P3 |
| P5：效率与体验 | 纯 computed 惰性化、selector/lens、诊断视图与调试信息 | core、children、开发工具 | 先把结构副作用从 computed 拆出 |

不建议给出未经原型验证的日级工期。P1 和 P3 都涉及运行模型，应该按验收用例完成度推进，而不是按 API 数量计算进度。

### 8.3 模块落地建议

- `mue-core`：所有权与响应式调度协议，不依赖 Macroquad；保证无图形环境可测试。
- `mue-macroquad`：组件实例如何持有节点、挂载提交、布局与帧阶段、输入系统。
- `mue-macros`：自动进入组件上下文，生成受控参数和输出接口；不要让宏本身承担调度器逻辑。
- 迁移兼容：旧同步 watch 和 `map_keyed` 暂时保留；新增明确命名 API，写清语义差异后再考虑默认值变更。
- 示例迁移：优先选择 `basics` 的列表/条件、`tab` 的 model、`navigation` 的延迟建树和 `theme` 的上下文继承，覆盖最容易退化的实际路径。

### 8.4 只有在目标变成“支持 Vue 风格 render/template”时，才引入 VDOM

完整 Vue-like 渲染路线至少需要：

1. `VNode` 或等价描述树，含类型、key、props、children。
2. Node 宿主适配层，实现创建、移除、重排、属性更新与文本更新。
3. 组件实例与 props 更新容器。
4. render effect 和父子更新排序。
5. 新旧节点 patch、keyed children diff、卸载协议。
6. 最后再做模板编译和静态优化。

此时已有 builder 是否可充当可重复执行的 VNode 描述，需要重新设计；当前 builder 消费自身并创建真实节点，不能直接当作可反复 patch 的描述对象。

这条路线成本显著更高，也可能重复现有细粒度绑定的职责。**除非目标是模板兼容、动态 render 组合或移植 Vue 组件开发方式，否则不推荐作为近期主线。**

---

## 9. 验收标准：如何判断是真的对齐了

建议新增语义一致性测试，而不是只把 API 改名为 ref/watch/model。

| 场景 | 目标断言 |
| --- | --- |
| 条件依赖切换 | effect 从 A 分支切到 B 后，A 改变不再触发 |
| 相等写入 | 各 comparator 的通知规则有明确测试；update 的强制通知行为可解释 |
| computed 缓存 | 连续读取不重复计算；采用 lazy 后，未读取不执行；输出相等抑制下游 |
| watch A→B→A | 明确最终值相同是否跳过；区分 watch 与 watchEffect |
| 同轮多次写 | deferred 模式只提交一次 UI 更新；sync 模式保留及时性 |
| 更新中再次写入 | 不重入正在运行的 callback；不会丢更新；循环能诊断 |
| cleanup / panic | cleanup 可安全停止资源；panic 策略不会留下错误的 scope/batch/tracker |
| 组件创建与销毁 | 创建 N 次、销毁 N 次后，核心资源计数回到基线 |
| 组件 setup | 同实例输入变化不重复初始化局部状态；卸载重建才初始化 |
| 同 key 新 item | 标题更新，同时局部输入状态保留 |
| key 重排与删除 | 重排不重建；删除后 effect、Reaction、context 可释放 |
| 条件销毁 | false 不建树，true 建一次，false 卸载，再 true 创建新状态 |
| 保留隐藏 | 状态保留，不参与指定的布局/绘制/命中；副作用策略一致 |
| Model 拒绝更新 | 父级拒绝后，子组件读到的值不自行漂移 |
| context | 最近祖先生效，typed key 不冲突，延迟建树仍使用正确 owner |
| 阶段 | pre、布局完成、post、下一帧通知的顺序固定并可断言 |

尽量将这些测试放在无 GPU 的 headless 节点/调度测试层。截图和交互示例用于补充布局、输入与绘制验证，不能代替生命周期和依赖图测试。

---

## 10. 本次验证与证据边界

### 10.1 已执行

```bash
cargo test -p mue-core --offline
cargo test -p mue-macroquad -p mue-macros --offline
```

结果：

- `mue-core`：29 个单元测试通过，2 个文档测试通过。
- `mue-macroquad`：15 个单元测试通过，5 个文档测试通过。
- `mue-macros`：0 个单元测试，3 个文档测试通过。
- 合计：**44 个单元测试、10 个文档测试通过，无失败。**

这证明当前已有测试通过，**不证明本文指出的组件作用域、循环更新、异常恢复、同 key 输入刷新等缺口已有覆盖**。图集和几何测试也不能替代组件语义测试。

本次没有新增行为复现测试，没有运行 GUI/Android 验证，也未执行全工作区构建。未修改实现，仅新增本文档。

### 10.2 本地关键源码索引

以下行号对应分析时工作区，后续修改可能使其移动：

| 主题 | 位置 |
| --- | --- |
| Signal 读取、比较、写入 | `mue-core/src/signal.rs:168`、`:212`、`:231`、`:259`、`:284` |
| Prop 静态/动态连接 | `mue-core/src/prop.rs:29`、`:78` |
| watch / effect / computed / Reaction | `mue-core/src/effect.rs:133`、`:187`、`:236`、`:293` |
| 依赖重收集、flush、batch | `mue-core/src/runtime.rs:129`、`:204`、`:281`、`:293` |
| Scope 与 provide/inject | `mue-core/src/scope.rs:50`、`:150`、`:157`、`:188` |
| Owned 释放 | `mue-core/src/disposable.rs:44` |
| 组件参数与宏展开 | `mue-macros/src/component.rs:55`、`:118`、`:192` |
| 节点生命周期 | `mue-macroquad/src/node.rs:216`、`:253` |
| children / keyed / 条件保留 | `mue-macroquad/src/node/layout/children.rs:13`、`:66`、`:114` |
| 帧驱动 | `mue-macroquad/src/app.rs:45` |
| 测量与布局订阅 | `mue-macroquad/src/layout.rs:35`、`:80` |
| Factory 与主题 Scope | `mue-macroquad/src/factory.rs:135`、`mue-macroquad/src/theme.rs:117` |

### 10.3 Vue 官方资料

官方文档来自文档站/`vuejs/docs` 的 main 内容，未固定为 v3.5.22 的文档快照；涉及具体实现的判断使用下列固定版本源码核对。

- [Reactivity in Depth](https://vuejs.org/guide/extras/reactivity-in-depth.html)
- [Reactivity Fundamentals](https://vuejs.org/guide/essentials/reactivity-fundamentals.html)
- [Computed Properties](https://vuejs.org/guide/essentials/computed.html)
- [Watchers](https://vuejs.org/guide/essentials/watchers.html)
- [Props](https://vuejs.org/guide/components/props.html)
- [Component v-model](https://vuejs.org/guide/components/v-model.html)
- [Conditional Rendering](https://vuejs.org/guide/essentials/conditional.html)
- [Provide / Inject](https://vuejs.org/guide/components/provide-inject.html)
- [Component Events](https://vuejs.org/guide/components/events.html)
- [Lifecycle Hooks](https://vuejs.org/api/composition-api-lifecycle.html)
- [effectScope / shallowRef](https://vuejs.org/api/reactivity-advanced.html)
- [nextTick](https://vuejs.org/api/general.html#nexttick)
- [Vue v3.5.22：ref](https://github.com/vuejs/core/blob/v3.5.22/packages/reactivity/src/ref.ts)
- [Vue v3.5.22：computed](https://github.com/vuejs/core/blob/v3.5.22/packages/reactivity/src/computed.ts)
- [Vue v3.5.22：effectScope](https://github.com/vuejs/core/blob/v3.5.22/packages/reactivity/src/effectScope.ts)
- [Vue v3.5.22：component](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/component.ts)
- [Vue v3.5.22：componentProps](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/componentProps.ts)
- [Vue v3.5.22：renderer](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/renderer.ts)
- [Vue v3.5.22：apiWatch](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/apiWatch.ts)
- [Vue v3.5.22：scheduler](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/scheduler.ts)
- [Vue v3.5.22：useModel](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/helpers/useModel.ts)
- [Vue v3.5.22：VNode identity](https://github.com/vuejs/core/blob/v3.5.22/packages/runtime-core/src/vnode.ts)

## 最终建议

近期主线应是：**先补齐资源所有权与组件边界，再明确调度阶段，修正列表和条件分支的数据更新契约，最后完善 Model、watch 与 slots。**

对齐的验收标准应是“相同业务意图能得到可预测、可测试的行为”，而不是“函数名看起来像 Vue”。对于当前项目，语义对齐比照搬 Vue 的实现架构更合适。
