
# Vue 3 与 mue：逻辑与数据差异对照

> 对照基准
> - **Vue**：`vuejs/core` tag **v3.5.13**（commit `6eb29d3`），主要看 `packages/reactivity/src` 与 `packages/runtime-core/src`。
> - **mue**：当前工作区 `font_atlas_refactor` 分支，以工作树源码为准（包含未提交修改）；涉及 `mue-core`（响应式内核）、`mue-macros`（组件宏）、`mue-macroquad`（节点/布局/渲染/事件）。
>
> 目的：把两者在 **响应式内核 / 组件模型 / 子节点 / 属性 / 调度与渲染** 上的**逻辑差异**与**数据结构差异**一次说清，便于移植、API 对齐或查找概念对应关系。
> 引用形如 `mue-core/src/signal.rs:15`、`packages/reactivity/src/effect.ts:86`。  
> **实施状态（2026-10-18）：** 下文保留第一阶段改造前的结构对照；组件构建 Scope、effect owner、Reaction 生命周期、cleanup/异常恢复和循环诊断已更新，当前行为见 [第一阶段落地记录](VUE_ALIGNMENT_PROGRESS.md)，原行号也可能随实现移动。

---

## 0. 一句话结论

| 维度 | Vue 3 | mue |
| --- | --- | --- |
| 范式 | 运行时 + 编译器 + **虚拟 DOM** + 补丁（patch）渲染器 | 无编译器、无 VDOM 的**响应式画布 UI 库** |
| 更新粒度 | 组件级重渲染 + 运行时 diff | **信号级**：属性/子节点是独立响应式值，谁变谁重算 |
| 组件实例 | `ComponentInternalInstance`（props/slots/setupState/ctx/subTree/effect/job/scope…） | **没有组件实例**；`Factory` 是定义句柄，`Scope` 是生命周期边界，`NodeInner` 是运行时节点 |
| 状态存放 | `ref/reactive` 对象，运行时按属性建 Dep | 整值 `Signal<T>`（`Box<dyn Any>`），无属性级依赖 |
| 调度 | 微任务队列（`queueJob` / `Promise.then`） | **同步刷新**（`batch` 深度归零即 flush），另有逐帧 `App::frame` |
| 子节点 | VNode children + keyed diff（LIS） | `Children = ReadSignal<Rc<[NodeRef]>>`，`map_keyed` 按 key 缓存节点，**从不对节点做 diff** |
| 生命周期 | `onMounted/onUpdated/onUnmounted…` + `EffectScope` | 无生命周期钩子；`Scope`/build/dispose + `on_cleanup` 替代 |
| 事件 | DOM 事件 + 原生冒泡 + emits/修饰符 | 命中测试 + 指针事件钩子 + 手势独占竞争 |

核心心智差异：

- Vue 的 `setup()` 通常在组件实例创建时执行一次；响应式变化驱动的是 render effect，render 在更新时产出新的 VNode 树再 patch。mue 的 `#[component]` 函数通常在一次 `IntoNode` 物化过程中执行一次（重新调用 builder、创建新 key 或重建分支时仍可能再次执行）。
- mue 的 `#[component]` 通常返回一棵已经挂好响应式属性的节点树；之后变化的是属性信号本身，节点树不会因为普通属性变化而整体重建（除非某个 computed 子列表信号变化）。

---

## 1. 响应式内核

### 1.1 存储容器对照

| 数据 | Vue | mue |
| --- | --- | --- |
| 全局入口 | 模块级变量 + `targetMap`（`dep.ts:234`） | `thread_local! RUNTIME`（`mue-core/src/runtime.rs:15`） |
| 源/依赖节点 | `Dep`（`dep.ts:67`）+ `Link`（`dep.ts:32`） | `SignalInner`（`signal.rs:15`） |
| 订阅者 | `ReactiveEffect` / `ComputedRefImpl`（实现 `Subscriber`） | `EffectInner`（`effect.rs:54`） |
| 反应式对象 | `Proxy` + 四个 `WeakMap` 缓存（`reactive.ts:26-35`） | **不存在**，只有 `Copy` 句柄 `Signal<T>` |
| 值容器 | JS 引用（对象/数组/Map/Set 原生结构） | `value: Option<Box<dyn Any>>`（`signal.rs:16`） |
| 身份 | 对象引用 / `uid` 计数器 | `slotmap` 的 `SignalId / EffectId / ScopeId / NodeId` 键 |
| 作用域 | `EffectScope` 实例树（`effectScope.ts:6`） | `ScopeInner` 存于 `SlotMap`，带 `parent/subscopes`（`scope.rs:21`） |
| 依赖列表 | `Link` 双向链表 `sub.deps/depsTail`、`dep.subs` | `EffectInner.dependencies: SmallVec<[SignalId;2]>` + `SignalInner.subscribers: HashSet<EffectId>` |
| 版本 | `dep.version`、全局 `globalVersion`（`dep.ts:19,68`） | `EffectState { Clean, Check, Dirty }`（`effect.rs:23`） |

### 1.2 依赖图的形状（最重要的数据差异）

**Vue 3.5 是「Link 双向链表 + 版本号」的拉取式系统：**

- 每个 `Dep` 有一条订阅者双向链表 `subs`（尾）/`subsHead`（DEV 头）；每个 `Subscriber` 有一条依赖双向链表 `deps/depsTail`。
- 二者之间的每一对关系由一个 `Link` 实例表示，`Link` 同时是上述两条链表的节点，还带 `version` 与 `prevActiveLink`（`dep.ts:32-62`）。
- 运行前把该 effect 所有 `Link.version = -1`（`prepareDeps`，`effect.ts:300`）；运行中每次命中 `track` 就把 `link.version = dep.version`（`dep.ts:121`）；运行后仍为 `-1` 的 Link 说明本次没用到，被双向摘除（`cleanupDeps`，`effect.ts:312`）。依赖集合增删都是 O(1) 链表操作，且顺序保持为访问顺序。
- 失效判定是**按需**的：`isDirty(sub)` 逐个比较 `link.dep.version !== link.version`（`effect.ts:341`），computed 的出口 dep 再叠加 `refreshComputed` 与 `globalVersion` 快速路径。

**mue 是「HashSet + 索引前缀复用」的推送式系统：**

- `SignalInner.subscribers: HashSet<EffectId>`：源 → 订阅者集合。
- `EffectInner.dependencies: Dependencies::Dynamic(SmallVec<[SignalId;2]>) | Static(SignalId)`：订阅者 → 源列表。`watch` 用 `Static`（回调内的读取不追踪，`effect.rs:142-157`），`watch_effect/computed` 用 `Dynamic`。
- 重新运行 effect 时用 `DependencyTracker { dependencies, index, new_dependencies }`（`runtime.rs:19`）：旧依赖按**前缀**对比，本次访问序列若与旧前缀一致就复用，一旦出现差异就整体转入 `new_dependencies`（`runtime.rs:34-41`），运行结束后删除尾部旧依赖、把新依赖插入订阅者集合（`runtime.rs:176-194`）。
- 失效判定是**推**的：`on_update(signal)` 直接给每个直连订阅者标 `Dirty`，并沿 computed 自己的信号向传递订阅者标 `Check`（`mark_stale`，`runtime.rs:257`）；读 computed 时 `update_signal_if_necessary` 再做惰性重算。

一句话：**Vue 主要用版本号在读取时判断「要不要重算」；mue 在写入时传播失效状态，读取或 flush 时再处理 `Check → Dirty`。mue 的 computed 具有缓存和按需刷新路径，但当前不是 Vue 那种完全惰性的 computed：创建时会立即计算，普通非 batch 写入也可能在 flush 中主动重算。**

### 1.3 追踪的入口

| 场景 | Vue | mue |
| --- | --- | --- |
| 读 ref | `ref.value` → `dep.track()`（`ref.ts:123`） | `ReadSignal::get()` → `track()`（`signal.rs:168-192`） |
| 读 reactive 属性 | Proxy `get` → `track(target, GET, key)`（`baseHandlers.ts:112-114`） | 不存在属性级追踪；`Signal::get` 就是唯一入口 |
| 暂停追踪 | `pauseTracking/enableTracking/resetTracking` + `shouldTrack`（`effect.ts:511-536`） | 无全局开关；用 `get_untracked/set_untracked` 与 `Access::track` 显式区分 |
| 手写 tracker | — | `create_reaction().track(|| …)`（`effect.rs:309`），taffy measure 用它（`layout.rs:38-57`） |
| 当前订阅者 | `activeSub` 全局变量，`run()` 内保存/恢复（`effect.ts:150-180`） | `Runtime.current_effect: RefCell<Option<EffectId>>`，`update_inner` 内保存/恢复（`runtime.rs:148,169`） |

mue 的「追踪开关」是局部的：一次 effect 运行期间，`Runtime.tracker` 被换成新的 `DependencyTracker`，`ReadSignal::track` 把它记进列表；effect 之外 `track()` 无效果。Vue 则用全局 `activeSub` + `shouldTrack` 的栈。

### 1.4 触发、失效与批处理

| 环节 | Vue | mue |
| --- | --- | --- |
| 写值 | Proxy `set` / ref setter；`hasChanged` 才触发（`ref.ts:143`） | `Signal::set/set_with/update`；比较器决定是否触发（`signal.rs:251-274`） |
| 触发 | `Dep.trigger`：`dep.version++`、`globalVersion++`、`notify()`（`dep.ts:161`） | `Runtime::on_update`：取出订阅者、逐个 `mark_stale`（`runtime.rs:105`） |
| 延迟 | `startBatch/endBatch` + `batchedSub/batchedComputed` 单链表，`batchDepth` 归零时统一 `trigger()`（`effect.ts:235-298`） | `batch_depth: Cell<usize>` + `pending_updates: Vec<EffectId>`，深度归零时 `flush_updates` 循环执行（`runtime.rs:281-303`） |
| 去重 | `EffectFlags.NOTIFIED` 位 + 链表头插（`effect.ts:239-248`） | `EffectState` 单调升级：`if effect.state >= state return`（`runtime.rs:260`），只在 `Clean → 非 Clean` 时入队 |
| computed 传播 | computed 在 `Dep.notify` 里继续 `dep.notify()` 通知下游（`dep.ts:187-193`） | `mark_stale` 对 computed 自身 signal 的订阅者标 `Check`，由读取触发惰性重算 |
| 自触发保护 | `RUNNING && !ALLOW_RECURSE` 时 `notify()` 直接返回（`effect.ts:139-144`） | 无等价的运行中保护；`flush_updates` 采用循环消费 pending 队列，但依赖传播和 computed 链检查仍有递归路径，也没有统一循环预算（`runtime.rs:204-255,257-289`） |

关键逻辑差异：**Vue 的批处理在同步栈结束时立即触发 effect，但组件的重渲染被调度器推迟到微任务**（见 `3.2）；**mue 的批处理结束即同步执行所有失效 effect，没有独立调度队列，也没有 `nextTick`。**

### 1.5 计算值（computed）

| 方面 | Vue | mue |
| --- | --- | --- |
| 类型 | `ComputedRefImpl implements Subscriber`（`computed.ts:47`） | computed 是一个带关联 effect 的 `Signal`（`signal.rs:17`），由 `create_computed` 关联（`effect.rs:237-261`）；创建时立即计算，后续既有失效传播也有按需刷新路径 |
| 缓存字段 | `_value`、`flags(DIRTY)`、`globalVersion`、`dep.version`（`computed.ts:51-82`） | `SignalInner.value` + `EffectInner.state`（`effect.rs:237-260`） |
| 重算判定 | `refreshComputed`：`globalVersion` 快速路径 → `isDirty` → 执行 `fn`（`effect.ts:364-417`） | 读时 `update_signal_if_necessary` → `Check` 时递归更新上游 computed → `Dirty` 则 `update`（`runtime.rs:204-255`） |
| 结果比较 | `hasChanged(value, _value)`，变了才 `dep.version++`（`effect.ts:404`） | `computed` 用 `PartialEq` 比较，不同才更新并通知；`computed_always` 每次通知（`effect.rs:266-291`） |
| 链式更新 | 依赖版本逐层比较 | `Check → Dirty` 沿 computed 链递归传播（回归测试 `effect::test::test_computed_chain_in_batch`） |
| 可写 | `computed(get, set)` 支持 setter（`computed.ts:147`） | 只读；要可写就返回 `Signal` 或自己组合 |
| 全局快速路径 | `globalVersion` 未变直接返回 | 无等价物；靠 `EffectState` 判断 |

> 校核备注：Vue 3.5.13 中 `ComputedRefImpl` **没有** `_dirty`、`_cacheable` 字段，也没有 `DepIterator`；脏状态完全由 `EffectFlags.DIRTY` 与 `dep.version` 表达（`effect.ts:341-357`）。早期教程里这些名字会误导。

### 1.6 作用域、清理与生命周期

| 方面 | Vue `EffectScope`（`effectScope.ts:6`） | mue `Scope`（`scope.rs:75`） |
| --- | --- | --- |
| 结构 | `_active`、`effects: ReactiveEffect[]`、`cleanups`、`parent`、`scopes`、`index`、`_isPaused` | `ScopeInner { effects, signals, subscopes, parent, context: TypeMap }` |
| 子作用域 | 未 detach 的子 scope 记在父 `scopes`，带下标做 O(1) 移除（`effectScope.ts:39-47,140-147`） | `Scope::new()` 自动挂到 `CURRENT_SCOPE`；`push_subscope`（`scope.rs:133`） |
| 运行 | `run(fn)` 临时切换 `activeEffectScope`（`effectScope.ts:88`） | `run(fn)` 临时切换 `CURRENT_SCOPE`（`scope.rs:150`） |
| 停止 | `stop()`：停 effects → 跑 cleanups → 递归停子 scope → 从父摘除（`effectScope.ts:118-150`） | `Scope::dispose`：逐个 `dispose_effect`、`dispose_signal`、递归子 scope（`scope.rs:51-72`） |
| 暂停/恢复 | `pause()/resume()`，暂停期间触发进 `pausedQueueEffects`，恢复时补发（`effect.ts:84,121-133`） | **没有暂停/恢复**；要么不创建，要么整体 dispose |
| 注销回调 | `onScopeDispose`（`effectScope.ts:182`） | 无 scope 级钩子；effect 内用 `on_cleanup`（`effect.rs:226`） |
| effect 资源注册 | `new ReactiveEffect` 时自动进 `activeEffectScope.effects`（`effect.ts:116-118`） | `Signal/Effect::new` 时 `CURRENT_SCOPE.push_signal/push_effect`（`signal.rs:94-105`） |
| 清理时机 | effect 重跑前执行上一次 `cleanup`；`stop` 时也执行（`effect.ts:150-192`） | `update_inner` 开头 `effect.cleanup()`；`dispose_effect` 也先 `cleanup`（`runtime.rs:139,334-350`） |
| 组件卸载 | 组件 `scope.stop()` 停掉组件内 watcher（`renderer.ts:2276`） | 节点 `dispose` → `node.scope.dispose()` + `retained_scopes`（`node.rs:216-237`） |

两者都靠「资源注册到当前作用域」实现自动清理，但 mue 的作用域还额外承担 provide/inject 上下文（见 1.7）。

### 1.7 注入 / 上下文

| 方面 | Vue | mue |
| --- | --- | --- |
| 存放 | `instance.provides`，子实例用 `Object.create(parentProvides)` 形成原型链（`apiInject.ts:11-34`） | `ScopeInner.context: TypeMap`，`inject_with` 沿 `parent` 链查找（`scope.rs:166-193`） |
| 键 | 字符串 / `InjectionKey` symbol | Rust 具体类型 `T`（`provide::<T>` / `inject::<T>`） |
| 就近覆盖 | 原型链天然覆盖 | 子 scope 有自己的 TypeMap，先查子再向上 |
| 合并语义 | `provide` 直接赋值 | `provide_with(|outer| …)` 可读取外层值再产出新值（`scope.rs:177-186`） |
| 根级提供 | `app.provide` / `appContext.provides` | 无 app 级；`provide_theme` 用外层 scope（`theme.rs:102-126`） |

mue 的 provide/inject 是**作用域级**的（与节点/组件无关）；Vue 是**组件实例级**的。mue 里 `inject` 的类型就是键，天然强类型；Vue 需要 `InjectionKey<T>` 约定。

### 1.8 更新元数据：mue 特有

mue 在 core 里维护了一个 Vue 没有的通用机制 `UpdateContext`（`update_context.rs`）：

- `with_update_context(value, f)` 把任意 `'static` 值压进当前上下文，`mark_stale` 时快照到 effect 上（`runtime.rs:259`），重跑 effect 时用 `update_context.run(…)` 恢复（`runtime.rs:163,198`）。
- 动画系统正是靠它实现「这次状态修改用哪条曲线」：`with_animation` = `with_update_context(Option<Animation>, f)`（`animation.rs:188-196`），`animated_with` 在 `watch` 回调里 `transaction()` 取出曲线（`animation.rs:279-299`）。
- 上下文会穿过 computed 与批处理（测试 `metadata_survives_deferred_computed_updates`）。

Vue 没有跨计算/批次传播的「更新元数据」概念；动画由 `<Transition>` 与 CSS 在渲染层完成，与响应式内核解耦。

---

## 2. 组件与节点模型

### 2.1 实例数据对照

| Vue `ComponentInternalInstance`（`component.ts:316-598`） | mue 对应物 | 说明 |
| --- | --- | --- |
| `uid/type/parent/root/appContext` | 无 | mue 没有组件树；`Factory.id` 只是主题匹配用的身份 |
| `vnode/next/subTree` | `NodeRef` + `Children` 信号 | 没有「上一棵/下一棵」，节点原地存活 |
| `effect/update/job` | effect + 逐帧 `App::frame` | 没有 render effect + scheduler job |
| `scope: EffectScope`（`component.ts:626`，`new EffectScope(true)` detached） | `NodeInner.scope` / `FactoryInstance` 的 `Scope` | 概念最接近的一块 |
| `props/attrs` | `Prop<T>` 字段（`Static` / `Dynamic(ReadSignal)`） | 静态/动态在类型上区分，无 attrs 透传 |
| `slots: InternalSlots` | `Owned<Children>` / 自定义 `children` 参数 | 子是值而非函数，没有 slot 作用域 |
| `setupState/ctx/renderCache/accessCache` | 无 | mue 的「setup 状态」就是闭包里的 `Signal`，由 `Scope` 统一释放 |
| `provides` | `Scope.context` | 见 1.7 |
| `refs/exposed/exposeProxy` | 无 | 没有模板 ref；节点句柄就是 `NodeRef` |
| 生命周期钩子数组（`bm/m/bu/u/bum/um…`） | `on_cleanup` + `Disposable` | 见 2.5 |
| `isMounted/isUnmounted/isDeactivated` | 无状态位 | 没有对应的 NodeRef 状态位；节点存活主要看对应 `NodeId` 是否仍存在于节点 SlotMap。`Signal` 句柄另有 `is_disposed`（`signal.rs:119`），不能直接套用于 `NodeRef` |

### 2.2 VNode 与 NodeInner

**Vue `VNode`（`vnode.ts:160-256`）** 核心字段：`type / props / key / ref / children / component / dirs / transition / el / anchor`；再加优化用的 `shapeFlag / patchFlag / dynamicProps / dynamicChildren`。

- `shapeFlag` 由类型推导（string→ELEMENT、object→STATEFUL_COMPONENT、function→FUNCTIONAL_COMPONENT、Suspense/Teleport 等，`vnode.ts:609-619`）。
- 编译器把模板切成 **block**：`openBlock()/createBlock()` 把带 patchFlag 或组件节点收进 `dynamicChildren`（`vnode.ts:263-334,514-530`）。
- 更新时 `patchBlockChildren` 按下标一一配对只 patch 动态子（`renderer.ts:940-981`），静态子树完全跳过；`patchFlag` 决定局部更新范围（TEXT/CLASS/STYLE/PROPS/FULL_PROPS，`renderer.ts:880-929`）。
- `patchKeyedChildren` 用 type+key 复用（`isSameVNodeType`，`vnode.ts:390-403`），未知序列走 `keyToNewIndexMap` + `newIndexToOldIndexMap` + LIS（`getSequence`，`renderer.ts:2491-2530`）最小化移动。

**mue `NodeInner`（`node.rs:38-57`）**：

- 生命周期：`scope`、`retained_scopes`、`hooks: NodeHooks`；
- 布局：`layout_id: Option<taffy::NodeId>`、`layout: OwnedLayout { rect: Signal<Rect> }`、`layout_sync: Option<Rc<dyn Fn(Vector) -> bool>>`；
- 交互：`hit_test: Option<HitTestFn>`、`gestures: Vec<GestureId>`；
- 绘制：`clip / child_offset / center_on_layout_origin / transform: Prop<Matrix>`；
- 文本：`font_size: Option<Prop<FontSize>>`、`computed_font_size: Signal<f32>`；
- 子节点：`children: Owned<Children>`，其中 `Children = ReadSignal<Rc<[NodeRef]>>`（`children.rs:14`）。

对应关系：Vue 的 `shapeFlag/patchFlag` 是「下次怎么 diff」的提示；mue 没有 VNode/组件 props diff，`Children` 信号的值就是**当前活动子节点的有序列表**。节点本身是长期存在的 Node；children 变化时仍会重算列表顺序并同步 Taffy 的直接子节点（`layout.rs:102-116`）。布局靠 `watch_effect` 同步。

### 2.3 构建：渲染函数 vs 一次性构建

| 方面 | Vue | mue |
| --- | --- | --- |
| 组件定义 | `setup()` 返回渲染函数/模板；`render()` 每次更新执行 | `#[component] fn` 函数；构建期执行一次 |
| 宏产物 | — | `XxxBuilder` 结构体 + `new` + fluent setter + `IntoNode`（`mue-macros/src/component.rs:225-254`） |
| 参数 | `defineProps` 运行时声明 | 参数被包成 `Prop<T>`；`#[model]` 包成 `Signal<T>`；setter 接 `impl Into<Prop<T>>`（`component.rs:79-94`） |
| 实例化 | `createVNode` → `mountComponent` → `setupComponent`（`renderer.ts:1165-1235`） | `Factory::create(content)` → `FactoryInstance::into_node`（`factory.rs:81-141`） |
| 复用 | 组件类型 + key | `Factory` 句柄（`Rc<base>` + `id` + `style` 回调） |
| 默认值合并 | props default 在组件选项里；`resolvePropValue` 处理 Boolean casting（`componentProps.ts:449-503`） | `Factory.defaults` → `style(defaults)`；主题再逐字段补（`factory.rs:149-173`） |
| 样式优先级 | class/style 合并 + scoped CSS | 实例 style > theme > factory defaults > base style（`provide_defaults` 链，`style.rs:257-274`） |

mue 的宏只做「闭包 + 构造器」的机械展开，没有任何编译期优化或依赖分析；`#[component]` 的结构体 `Style` 字段是最终落到 `NodeInner` 的样式入口。

另外，Vue 的 render effect 在 `setupRenderEffect` 中创建：`new ReactiveEffect(componentUpdateFn)`，`update = effect.run.bind(effect)`，`job.id = instance.uid`，`effect.scheduler = () => queueJob(job)`，并 `toggleRecurse(instance, true)`（`renderer.ts:1555-1579`）。mue 没有等价的 per-component effect，只有节点属性上的信号。

### 2.4 props、attrs 与样式

| 方面 | Vue | mue |
| --- | --- | --- |
| 声明 | `props: { type, required, default, validator }`（`componentProps.ts:55`） | Rust 参数类型 + `#[default(…)]` / `#[default]`（`component.rs:57-94`） |
| 运行时存储 | 有状态组件 `instance.props = shallowReactive(props)`（`componentProps.ts:218`） | `Prop<T> = Static(T) | Dynamic(ReadSignal<T>)`（`prop.rs:31`） |
| 更新 | `updateProps`：`patchFlag & PROPS` 时只写 `dynamicProps` 列出的键（`componentProps.ts:238-300`） | 无需更新：`Prop::Dynamic` 本身就是信号；静态值永不改 |
| 键归一化 | `camelize` 后归入 props；未声明键进 `attrs`（`componentProps.ts:372-447`） | 无 attrs；类型系统直接约束 |
| attrs 透传 | 默认落到根元素（`inheritAttrs`） | 无 |
| 校验 | `validator` + DEV 类型校验（`validateProps`） | 编译期类型检查，无运行时校验 |
| 默认值 | 运行时 default；Boolean 隐式 false；函数工厂结果缓存到 `propsDefaults` | `Prop::Static(default)`，在 setter 缺省时使用 |
| 事件 | `onXxx` 作为 prop；`emit` 查找 `toHandlerKey` / camelize / hyphenate，`once` 用 `instance.emitted` 去重（`componentEmits.ts:111-229`） | 样式上的 `HookFn<T>`（`hook.rs:5`）；`append` 串接（先注册先执行） |
| class/style | 字符串/对象/数组归一 | `Style` 字段逐项 `Option`，`merge/provide_defaults` 决定覆盖 |

Vue 的动态等价物在 mue 里是**数据本身**（信号），不需要 re-render 或 diff；反过来，mue 的 `Prop<T>` 不能承载「对象属性级」的响应性——要变化就得整体换值或换信号。

### 2.5 slots 与生命周期

**slots**：Vue 把子内容归一成函数（`componentSlots.ts:26-199`），`withCtx` 绑定渲染实例并管理 block 追踪，`renderSlot` 生成带 `STABLE_FRAGMENT/BAIL` 的 Fragment（`helpers/renderSlot.ts:25-100`）；动态槽带去重更新（`updateSlots`，`componentSlots.ts:201-245`）。mue 没有插槽协议，`Owned<Children>` 或泛型内容参数就是全部。

**生命周期**：Vue 的钩子通过 `injectHook` 挂到实例的 `LifecycleHooks` 槽位上（`apiLifecycle.ts:20-64`），顺序为 `beforeCreate → created → beforeMount → mounted → beforeUpdate → updated → beforeUnmount → unmounted`（`bm`/`m` 在 `renderer.ts:1293-1392`，`bu`/`u` 在 `1474-1527`，`bum`/`um` 在 `2264-2288`）；`onActivated/onDeactivated` 由 KeepAlive 再导出。

mue 当前没有与 Vue 组件实例一一对应的 `mounted/updated/unmounted` 生命周期协议，但已有节点级和 effect 级 hook。替代机制是：

- **创建**：`Node::build_with_style` 内部 `scope.run(|| …)`，用户的构建闭包在这里执行一次（`node.rs:253-281`）；
- **更新**：通常不需要组件级更新钩子，changed 的是信号；`Animation/Driver` 的 `advance(dt)` 是显式逐帧接口（`animation.rs:198-327`）；
- **销毁**：`Owned<NodeRef>` 的 `Drop → NodeRef::dispose`，依次释放 `children`、`scope`、`retained_scopes`、taffy 节点、手势（`node.rs:216-237`）；
- **effect 级清理**：`on_cleanup`（`effect.rs:226`）；
- **局部资源保留**：`Node::retain_scope` 把 scope 挂在节点上（`node.rs:244-246`），`FactoryInstance` 与 `ThemeProvider` 用它保证实例期资源与节点同寿；
- **节点/交互 hook**：`NodeHooks`、pointer/hover/gesture 等 hook 已存在，但它们不是统一的组件生命周期协议。

---

## 3. 子节点、调度与渲染

### 3.1 子节点与 keyed 复用

| 方面 | Vue | mue |
| --- | --- | --- |
| 子节点表示 | `VNode[]` / 文本 / 插槽函数 | `ReadSignal<Rc<[NodeRef]>>`（`children.rs:14`） |
| 普通列表 | `patchChildren` + `patchKeyedChildren` LIS diff（`renderer.ts:1601-2001`） | `computed` 直接算出新数组；节点对象不变 |
| 无 key | `patchUnkeyedChildren` 按索引逐位对齐，不识别移动（`renderer.ts:1703-1760`） | 通常不需要；`join_children` 直接拼接 |
| 带 key | `keyToNewIndexMap` + LIS 识别复用与最小移动 | `map_keyed(list, key_fn, node_fn)`：`HashMap<K, Node>` 缓存，key 命中就复用原节点，并重建当前顺序；不对同 key item 自动做 props diff（`children.rs:68-112`） |
| 条件显示 | `v-if` 销毁/重建；`v-show` 保留 DOM 改 display | `show_if(cond, node)`：条件为假时列表为空但**节点与局部状态保留**（`children.rs:115-131`） |
| 顺序变化 | diff 后 `move` 真实 DOM | 顺序就是数组顺序；没有「移动」概念 |
| 卸载时机 | 从 children 消失即 unmount（KeepAlive 除外） | 从列表移除且最后一个持有者被 drop 才 dispose |
| 保留页面 | `<KeepAlive>`（组件级缓存） | `pages(selected, [..])` 全部构建并保留（`pages.rs:168-255`）；`navigation_stack` 保留路径前缀页 |
| 稳定化循环 | — | 字体/布局依赖可能连锁变化，`resolve_font_sizes` 与布局各自最多 64 轮直到 `changed == false`（`node.rs:205-212`、`app.rs:78-113`） |

这是 mue 与 Vue 在「数据」上最直观的差异：**Vue 的 children 是每次 render 重新生成的 VNode 数组，节点身份靠 key 与 diff 维持；mue 的 children 是一份长期存在的 `Rc<[NodeRef]>`，节点身份就是 `NodeId`。**

### 3.2 更新调度

| 方面 | Vue | mue |
| --- | --- | --- |
| 触发源 | 信号/属性变化 → 组件 render effect 失效 | 信号变化 → effect 立即/批处理标脏 |
| 队列 | `queueJob`：按 `id` 升序插入（`job.id = instance.uid`），父先于子；`PRE` 排在组件更新前（`scheduler.ts:73-112`） | 无组件队列；`pending_updates` 按标脏顺序，`flush_updates` 循环消费 |
| 时机 | 微任务：`currentFlushPromise = Promise.resolve().then(flushJobs)`（`scheduler.ts:114-118`） | 同步：`batch` 深度归零或非批处理下写入即刷（`runtime.rs:117-119,298-300`） |
| post 回调 | `queuePostFlushCb` + `flushPostFlushCbs`，承载 `mounted/unmounted` 与模板 ref（`scheduler.ts:120-204`） | 动画每家一个 `Driver`，由 `animation::advance` 在帧首批量推进（`animation.rs:308-327`） |
| 递归上限 | 同一 job 超过 100 次报错（`scheduler.ts:53,265-284`） | 没有统一显式上限；pending 队列虽用循环消费，但依赖传播和 computed 链检查仍有递归路径，环依赖缺少统一诊断（`runtime.rs:204-255,257-289`） |
| `nextTick` | 有，复用 `currentFlushPromise`（`scheduler.ts:56-62`） | 没有；`set_timeout` 是帧驱动定时器（`mue-macroquad/src/runtime.rs:157`） |
| 卸载跳过 | `DISPOSED` 位跳过已卸载组件的 job（`scheduler.ts:226`） | slotmap 中不存在即跳过（`runtime.rs:205`） |
| 强制更新 | `$forceUpdate` = `queueJob(instance.update)`（`componentPublicInstance.ts:381-385`） | 调 `Signal::force_trigger`（`signal.rs:245`）或 effect `force_trigger`（`effect.rs:122`） |

### 3.3 布局与绘制

Vue 把渲染委托给宿主：DOM 由浏览器布局，`@vue/runtime-dom` 只做 `hostInsert/hostPatchProp` 等操作。

mue 自己就是一个宿主，`App::frame`（`app.rs:45-120`）的顺序是：

1. `batch` 推进时钟动画（`animation::advance`），再 `batch` 跑到期 `set_timeout`；
2. `resolve_font_sizes` 自根向下解析 `FontSize`（`Percent/Em` 相对父字号、`Rem` 相对根字号），循环至稳定；
3. `compute_layout_with_measure` 跑 taffy，随后 `sync_layout` 递归让节点修正尺寸相关输入（如 `pages` 的视口），最多 64 轮；
4. `begin_text_render_pass`：cosmic-text 图集开始新一轮；
5. `batch` 内 `render`（递归、transform、clip）、`pointer_manager.process`（命中测试 + 事件 + 手势）、滚轮检查。

### 3.4 事件

| 方面 | Vue | mue |
| --- | --- | --- |
| 事件源 | 宿主 DOM 事件 | macroquad/miniquad 指针事件（鼠标、触摸） |
| 命中 | 浏览器 | `hit_test` 递归：先逆序遍历子节点，命中时按**子→父**顺序加入 `IndexSet`（`hit_test.rs:38-65`） |
| 传播 | 捕获/冒泡、`stopPropagation`、修饰符 | 没有冒泡控制；沿命中集合依次调用 `pointer_event/hover_event` |
| 组件事件 | `emit('change')` → `onChange` prop | 无 emit；直接 `HookFn`（回调）或更新外部 `Signal` |
| 手势 | 由浏览器指针事件或第三方库实现 | 内置 `Gesture` 识别器与**独占认领**：先认领的胜出，其余 `on_rejected`（`pointer.rs:261-321`） |
| 变换 | CSS transform 影响命中 | `transform` 同时参与绘制和命中测试；命中测试先应用逆矩阵，再以布局矩形判断（`hit_test.rs:25-35,38-65`） |
| 键盘/无障碍 | DOM 焦点与语义 | 无（README 明确画布节点不提供键盘焦点/无障碍语义） |

---

## 4. 逐条差异清单

### 4.1 逻辑差异

1. **Vue 的属性级追踪**（`targetMap: target → key → Dep`）**在 mue 中不存在**；mue 只有「整个信号值」的追踪，嵌套对象要改就整体替换或经 `Signal::update` 显式通知。
2. **Vue 的依赖清理靠版本号 + 双向链表**（`prepareDeps/cleanupDeps`）；mue 靠**访问序列前缀复用 + HashSet 增删**（`DependencyTracker`）。
3. **Vue 主要用版本号在读取时判断是否需要刷新**（读时 `isDirty/refreshComputed`，含 `globalVersion` 短路）；mue 在写入时传播失效状态，读取或 flush 时处理 `Check→Dirty`。mue 的 computed 不是完全惰性的：创建时立即计算，普通非 batch 写入也可能在 flush 中主动重算。
4. **Vue 有全局追踪开关**（`pauseTracking/enableTracking/resetTracking`）；mue 用 `get_untracked/set_untracked` 做点状逃逸，无全局态。
5. **Vue 的组件会重复执行 render effect 并 patch VNode**（但 `setup()` 通常只执行一次）；mue 的组件构建函数通常只在一次物化过程中执行，变化发生在信号里。
6. **Vue keyed diff 会移动/复用真实节点**（LIS，`renderer.ts:1966-1999`）；mue 的 `map_keyed` 做基于 key 的 Node 缓存、删除和顺序重建，但不做 VNode/组件 props diff；同 key item 的新数据不会自动重新调用 `node_fn`。
7. **Vue `v-if` 默认销毁**；mue `show_if` 保留 Node 实例但把它从活动 children 中移除，因此不参与当前布局/绘制/命中；它不是严格等价的 `v-show` 或 `KeepAlive`。
8. **Vue 有异步调度器与 `nextTick`**；mue 同步刷新，没有微任务队列。
9. **Vue 有组件实例与生命周期**；mue 只有 `Scope` 与 `Disposable`（`Owned` 的 `Drop`）。
10. **Vue 的 `emit`/slots/attrs 是运行时协议**；mue 用泛型参数、`Prop<T>`、`Owned<Children>` 和 `HookFn` 表达，全部在编译期定型。
11. **Vue 的 provide/inject 挂在实例**；mue 挂在 `Scope` 的 `TypeMap` 上，以 Rust 类型为键。
12. **Vue 有 `globalVersion` 的 O(1) computed 快速路径**；mue 没有全局版本，靠状态机。
13. **mue 有 `UpdateContext`**（跨 computed/批次的更新元数据），支持 `with_animation` 这类事务语义；Vue 无对应概念。
14. **mue 强制 `'static` 值 + `Box<dyn Any>` 下转型**；Vue 用 JS 动态类型，无此约束。
15. **mue 有指纹式稳定化循环**（字体/布局最多 64 轮，否则 panic）；Vue 的布局交给浏览器，无此阶段。
16. **mue 的手势是独占认领模型**；浏览器是多监听器可同时响应 + 传播控制。
17. **mue 的 transform 参与命中测试**（先对输入点应用逆矩阵，再检查布局矩形），但整体语义不等同于 DOM/CSS transform；DOM 的 transform 也会改变命中。
18. **mue 没有模板/编译器/指令**（`v-if/v-for/v-model` 等都没有），只有 Rust 组合子；因此也没有 block 树、patchFlag、静态提升等编译优化。
19. **mue 的批处理不会默认延迟到微任务**：batch 结束时同步 flush；失效 computed 在读取或 flush 中可能重算。Vue 的组件提交通常由 scheduler 延后，因此会出现响应式值已变而 DOM 尚未更新的阶段。
20. **mue 的信号是 `Copy` 句柄**（`ReadSignal/Signal`，`signal.rs:47-58,194-210`），可自由复制进闭包；Vue 的 ref 是对象引用。
21. **Vue 的异步 setup / Suspense**（`component.ts:895`、`components/Suspense.ts:683-730`）在 mue 中没有对应；mue 的异步只能是资源加载后手动写信号。

### 4.2 数据差异

| 概念 | Vue 数据结构 | mue 数据结构 |
| --- | --- | --- |
| 源 | `Dep { version, activeLink, subs, subsHead, map, key, sc, computed }` | `SignalInner { value, effect, subscribers, comparator }` |
| 订阅者 | `ReactiveEffect { deps, depsTail, flags, next, cleanup, scheduler, onStop, onTrack, onTrigger }` | `EffectInner { update_context, callback, cleanups, signal, dependencies, state }` |
| 关系 | `Link { sub, dep, version, nextDep, prevDep, nextSub, prevSub, prevActiveLink }` | `SignalInner.subscribers: HashSet<EffectId>` + `EffectInner.dependencies: SmallVec<[SignalId;2]>` |
| 标志位 | `EffectFlags`：ACTIVE/RUNNING/TRACKING/NOTIFIED/DIRTY/ALLOW_RECURSE/PAUSED | `EffectState`：Clean/Check/Dirty（单调升级） |
| 计算 | `ComputedRefImpl { _value, dep, deps, depsTail, flags, globalVersion, isSSR, fn }` | 普通 `Signal` + 关联的 `EffectInner.signal/effect` |
| ref | `RefImpl { _value, _rawValue, dep, __v_isRef, __v_isShallow }`（`ref.ts:108-121`） | `Signal<T>` 单一整值；无 raw/reactive 双层 |
| 反应式代理 | 四张 `WeakMap` + base/collection handlers + `ReactiveFlags` | 无 |
| 全局 | `activeSub`、`shouldTrack`、`trackStack`、`globalVersion`、`targetMap`、`batchDepth/batchedSub/batchedComputed` | `Runtime { signals, effects, scopes, current_effect, tracker, batch_depth, pending_updates }` |
| 作用域 | `EffectScope { _active, effects, cleanups, _isPaused, parent, scopes, index }` | `ScopeInner { effects, signals, subscopes, parent, context: TypeMap }` |
| 注入 | `provides` 对象 + 原型链 | `TypeMap` + `parent` 链 |
| 组件 | `ComponentInternalInstance`（约 30 个字段，`component.ts:316-598`） | 无；`Factory/NodeInner/Scope` 三者分担 |
| 节点 | `VNode { type, props, children, key, shapeFlag, patchFlag, dynamicChildren, el, component, … }` | `NodeInner { scope, hooks, layout_id, layout_sync, hit_test, gestures, clip, transform, children, … }` |
| 子节点 | `VNodeNormalizedChildren`（动态数组/文本/插槽） | `Children = ReadSignal<Rc<[NodeRef]>>` |
| 调度任务 | `SchedulerJob { id, flags, i }`，`queue: SchedulerJob[]` | `pending_updates: Vec<EffectId>`；无 job/优先级 |
| 生命周期 | 实例上按 `LifecycleHooks` 枚举分槽的钩子数组 | 无；`Disposable/on_cleanup/Driver` |
| 版本号 | `dep.version` / `globalVersion` / `computed.globalVersion` | 无；`EffectState` 取代 |

---

## 5. 迁移与互操作提示

- **从 Vue 心智迁到 mue**：把「组件」当作一次性构建函数，把「响应式状态」当作 `Signal`，把「模板里读状态」当作把 `Prop/ReadSignal` 直接塞进属性/setter。不要再指望 re-render 或 diff。
- **需要属性级粒度时**：拆成多个 `Signal`（而不是一个大对象 + `reactive`），或用 `map/ReadSignal::map` 派生。
- **列表**：用 `map_keyed` 明确给 key，才能保留子节点局部状态；不需要保留就用普通 `IntoChildren`。
- **条件**：要销毁重建语义时，需自己根据条件构造/丢弃节点（例如把节点放进 `Signal<Option<Node>>` 并在条件变化时替换），mue 没有等价的 `v-if`。
- **动画**：状态改动用 `with_animation` 包起来，再通过 `animated(*signal)` 或 `.animation(...)` 取呈现值；这替代了 Vue 的 `<Transition>` + CSS。
- **副作用清理**：用 `Scope` + `Owned` + `on_cleanup`；不要依赖「组件卸载钩子」。
- **注入**：用类型作键；跨组件传值建议优先用 `Prop` 显式传递，`provide/inject` 用于主题这类跨层配置（`ThemeChain` 就是范例）。

---

## 6. 参考索引

**Vue（`vuejs/core@v3.5.13`）**
- `packages/reactivity/src/dep.ts`：Dep、Link、targetMap、track/trigger、version/globalVersion
- `packages/reactivity/src/effect.ts`：EffectFlags、ReactiveEffect、batch、isDirty、refreshComputed、pauseTracking
- `packages/reactivity/src/computed.ts`：ComputedRefImpl、notify、value
- `packages/reactivity/src/ref.ts`：RefImpl、toRef/toValue/proxyRefs
- `packages/reactivity/src/effectScope.ts`：EffectScope（pause/resume/stop）
- `packages/reactivity/src/baseHandlers.ts` / `collectionHandlers.ts`：reactive 代理追踪
- `packages/reactivity/src/watch.ts`：watch/watchEffect 调度
- `packages/runtime-core/src/component.ts`：ComponentInternalInstance、setupComponent、handleSetupResult、finishComponentSetup
- `packages/runtime-core/src/vnode.ts`：VNode、shapeFlag、block 树、normalizeChildren
- `packages/runtime-core/src/renderer.ts`：patch、processComponent、patchElement、patchKeyedChildren（LIS）、unmount、setupRenderEffect
- `packages/runtime-core/src/componentProps.ts`：initProps/updateProps/setFullProps/resolvePropValue
- `packages/runtime-core/src/componentSlots.ts` / `componentEmits.ts`：slots、emits
- `packages/runtime-core/src/apiInject.ts`：provide/inject 原型链
- `packages/runtime-core/src/apiLifecycle.ts`：injectHook 与钩子顺序
- `packages/runtime-core/src/scheduler.ts`：queueJob、微任务 flush、nextTick、递归上限
- `packages/runtime-core/src/helpers/renderSlot.ts`：renderSlot / Fragment

**mue（本仓库）**
- `mue-core/src/runtime.rs`：Runtime、依赖追踪、mark_stale、flush_updates、batch、dispose
- `mue-core/src/signal.rs`：SignalInner、ReadSignal/Signal、Access
- `mue-core/src/effect.rs`：EffectInner、EffectState、watch/watch_effect、computed、on_cleanup
- `mue-core/src/scope.rs`：Scope、provide/inject、dispose 级联
- `mue-core/src/prop.rs`：Prop、PropValue、default_props
- `mue-core/src/update_context.rs`：UpdateContext
- `mue-macros/src/component.rs`：`#[component]/#[node]` 展开、`#[default]/#[model]`
- `mue-macroquad/src/node.rs`：NodeInner/NodeRef/Node、build_with_style、render/dispose
- `mue-macroquad/src/node/layout/children.rs`：Children、map_keyed、show_if、join_children
- `mue-macroquad/src/style.rs`：Style 字段、merge/provide_defaults、build_taffy
- `mue-macroquad/src/factory.rs` / `theme.rs`：Factory、FactoryInstance、ThemeChain、provide_theme
- `mue-macroquad/src/app.rs`：App::frame 的帧循环与稳定化
- `mue-macroquad/src/animation.rs`：with_animation、animated_with、Driver
- `mue-macroquad/src/layout.rs`：use_layout、MeasureFn、set_measure_fn
- `mue-macroquad/src/event/hit_test.rs` / `event/pointer.rs`：命中测试、指针与手势
- `mue-macroquad/src/node/layout/pages.rs` / `navigation.rs`：保留式页面切换
