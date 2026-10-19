# Vue 语义对齐：第一阶段落地记录

> 日期：2026-10-18。本文记录实际实现；`VUE_ALIGNMENT_ANALYSIS.md` 和 `vue-mue-diff.md` 保留改造前的分析基线。  
> 范围：响应式安全、owner Scope、组件构建边界、Reaction 生命周期及 children 契约。没有引入 VDOM 或改变默认同步调度。

## 1. 已实现

### 1.1 响应式运行时

- effect 记录创建时的 owner Scope；重跑以及 `Reaction::track` 在该 owner 下执行。
- `watch_effect`、computed 在首次执行前完成 Scope 注册；Reaction 也登记到 Scope，并实现 `Disposable`，支持 `.owned()`。
- 普通写入仍同步更新；callback 内的写入不重入当前 flush，待 callback 返回后统一消费。
- 正在运行的 effect 再次失效时，安排后续运行，不静默吞掉自写更新。
- 每个 effect 在同一 flush 中最多执行 100 次，超出后明确 panic，debug 构建附带创建位置。
- 检测正在求值的 computed 和依赖检查环，避免读取暂时移出的缓存或无界循环检查。
- 读取脏 computed 时区分“本次读取消费的刷新”和真正的后续失效，避免首次 watchEffect 重复运行、Reaction.track 意外调用 invalidation callback。

### 1.2 清理与 panic 边界

- cleanup 在释放 effects arena 借用后执行，不收集依赖。
- cleanup 可以读取 computed、写其他 Signal、创建并停止 effect；不会因原先持有的 arena 借用而直接冲突。
- cleanup 逐项执行：某项 panic 后继续尝试剩余清理，最后重新抛出首个 panic。
- `Scope::run` 和 `batch` 用 RAII 恢复上下文/深度；effect 用户回调的 panic 会恢复 callback、缓存容器、tracker、current effect 和执行状态，再继续抛出。
- 失败 computed 保持失效状态，后续读取可以重试，而不是把旧缓存误当最新值。
- effect 抛出前所做的业务写入、闭包内部修改和外部副作用**不会回滚**；这不是事务回滚机制。
- Scope 拆除以 batch 包裹，effect 逆序释放；任意清理失败不阻止继续拆除其他登记资源和子 Scope。
- 禁止进入已经销毁的非 null Scope，并拒绝 Scope 自环/祖先环。

### 1.3 组件构建期作用域

`#[component]` 现在通过 `Node::build_component` 执行：

```text
创建组件 Scope
→ 在 Scope 内求值缺省参数（含默认 model）
→ 执行组件函数
→ 保持原有 style 默认值合并
→ 将返回 builder 物化为原根 Node
→ 根 Node retain 组件 Scope
```

这解决了组件函数在最终根节点创建之前产生的 Signal/computed/effect 归属外层的问题。

- 没有增加布局包装节点，没有替换根 Node identity。
- builder 构造与显式 setter 仍在调用方执行。
- 外部传入的 Signal/model 不转移所有权，不随组件销毁。
- Factory、Theme、嵌套组件和原有样式优先级保持兼容。
- 这是逻辑资源边界，不是完整的 Vue ComponentInternalInstance；尚未增加 mounted/updated/unmounted 协议。

### 1.4 节点与测量 Reaction

- Node 拆除以 batch 包裹，避免子 cleanup 的写入驱动尚未拆除的父节点 watcher。
- 拆除时即使某项 cleanup panic，仍继续尝试释放其他 scope、布局 context 和手势，最后重抛首个 panic。
- 测量闭包通过 `Owned<Reaction>` 持有订阅；替换闭包会停止旧订阅。
- 删除 Taffy 节点前显式清空 node context。当前 Taffy 的 `remove()` 不会自动清理该测量 context，单靠删除节点会留下闭包与订阅。

### 1.5 Children 契约与诊断

- `map_keyed` 先计算并验证全部 key，再修改节点 cache；重复 key 明确报告冲突位置，避免生成包含悬空 NodeRef 的列表。
- 保留原语义：同 key 复用 Node，但不会自动 patch 新 item 或重新执行 factory。
- `show_if` 仍接收预构造 Node；隐藏只移出活动 children，保留状态和 effect，不是惰性卸载。
- `join_children` 仍只做有序 flatten。
- 为上述行为和 Taffy children 重接增加 headless 回归。

## 2. 有意发生的行为变化

1. 组件内 `provide` 归属组件自己的 Scope，不再意外污染调用方或后续兄弟；已在别处构建的 children 不自动迁移上下文。
2. effect 重跑使用创建时的 owner，`inject` 不再随触发者上下文漂移。
3. callback 内嵌套写入不递归重入当前 callback；自失效在其返回后补跑，并受预算约束。
4. 清理过程可能继续执行其他 cleanup 后再抛出首个 panic；Scope 中的 effects 按创建逆序释放。
5. 重复 key、循环更新和失效 Scope 进入会产生明确诊断，而不是依赖后续失效句柄或 RefCell 错误。

## 3. 测试结果

执行命令：

```bash
cargo test --workspace --offline
cargo check --workspace --offline
cargo test -p mue-core --release --offline
cargo clippy -p mue-core -p mue-macroquad -p mue-macros \
  --all-targets --offline --no-deps -- -D warnings
```

全部通过：

| 模块 | 单元测试 | 文档测试 |
| --- | ---: | ---: |
| mue-core | 57 | 2 |
| mue-macroquad | 33 | 5 |
| mue-macros | 0 | 0 |
| 合计 | **90** | **7** |

相对于本阶段开始时的 44 个单元测试，新增 **46 个**，覆盖：

- 条件依赖切换、computed 相等性过滤和脏读取。
- owner 上下文、Reaction 显式停止及随 owner 销毁。
- cleanup 响应式操作、追踪隔离、panic 后其余 cleanup 执行。
- batch/effect/computed 异常恢复、计算预算及自写不重入。
- Scope 活子树级联、环检测和拆除中不 flush。
- 组件局部资源、外部 model、缺省 model、context 隔离、Factory/Theme/样式兼容。
- key 复用/重排/删除、重复 key、隐藏副作用、children 顺序和 Taffy 重接。
- 测量闭包替换、节点删除及异常拆除后的释放。

本次未运行 GUI、GPU 图像验收或 Android 设备验证。全工作区编译通过不等于这些交互/平台路径已经实测。

## 4. 仍然保留的边界

- 默认仍是同步更新与显式 batch；没有微任务 scheduler、pre/post watcher 或 nextTick。
- computed 仍保持创建时计算、结果缓存和现有刷新方式，未改为纯 lazy。
- `Signal::update/set_with` 等值访问闭包仍持有 signals arena 借用；不能把 cleanup 借用修复理解为任意 Signal 嵌套访问都已支持。
- effect 重跑时不会自动清理上一轮创建的子 effect/Scope；需要显式管理，避免持续创建资源。
- owner 已销毁时，cleanup 在 null Scope 下执行；其中新建资源需显式停止，不会自动转移给失效 owner。
- timeout 仍在独立队列，不随组件 Scope 自动取消。
- 已销毁资源的 ID 仍可能留在长寿命 Scope 的登记列表中；本阶段没有重新设计反向登记与移除。
- `Node::build` 的构建上下文栈及任意建树 panic 的完整回滚未在本阶段统一改造。组件 helper 的失败释放不代表所有节点构建都已完全 unwind-safe。
- Drop 清理本身如果在已有 unwind 过程中再次 panic，仍受 Rust 双重 panic 规则约束；不保证任意用户析构异常都可恢复。
- 没有取消 `Signal/Scope/Effect` 的 Copy 句柄语义；裸句柄离开作用域不会自动 dispose，`Owned` 才提供 Drop 释放。

## 5. 第二阶段接口方向（尚未实现）

保持旧 API 兼容，下一步可独立新增：

### `map_keyed_reactive`

```text
key → item Signal + Node + item Scope
```

同 key 更新 item Signal，Node/局部状态保留；删除 key 时释放 item Scope。factory 接收只读 item 信号，而不是一次性的 `&T` 快照。需要明确 `Clone/PartialEq` 或 `Rc<T>` 的约束，并把输入更新与结构变更放在同一 batch。

### `if_branch`

接收惰性 factory，初始 false 不建树；true 创建分支，false 卸载并释放分支 Scope，再次 true 创建新状态。现有 `show_if` 保持“隐藏保留”语义，不悄悄改成卸载。

它们应沿用本阶段已经建立的 owner 与释放契约，再各自增加同 key 输入更新、分支惰性创建/卸载及资源回收测试。
