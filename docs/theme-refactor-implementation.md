# Theme 重构实施规格

> **状态：首版实现规格（实现已落地）。定稿日期：2026-10-31。**
> 实际实现范围与验证证据见 [实施记录](theme-refactor-validation.md)；性能实测见 [性能基线](theme-refactor-performance.md)。本规格描述目标行为，未逐项声明宿主平台均已实测。
> 本文是本轮实施的规格入口。[设计摘要与 Q 索引](theme-refactor.md) 解释决策，[讨论历史](theme-refactor-history.md) 保存旧提案。历史中的未决项与候选示例不覆盖本文。
> 本文同步首版 API 与内部协议；实现细节与设计约束分别注明。行为、所有权边界和验收要求保持不变。文档同步日期：2026-11-02。

## 1. 首版范围

首版实现范围如下；实际验证状态与宿主边界见本页顶部链接的实施记录：

- 删除 Factory、FactoryInstance、FactoryUse 及 factory stack/sibling context，无兼容层；迁移工作区示例、测试与使用文档。
- node/component 使用共同的 prepare → seal → mount 协议；node 声明显式分离准备与消费，children 延后物化。
- 显式主题 opt-in、声明级类型 identity、具体泛型实例化匹配、allowlist options、ClassName/class!、compound 与逗号 selector list。
- 顶层 Prop&lt;Theme&gt;、跨共根 target 的逐属性级联、动态换肤及 class/options/hover 更新。
- 显式 hover、NodeInner 统一状态、最上层目标及祖先命中路径、拖动/触摸/窗口语义和只读观察。

明确后置：实例 ID（setter、selector、registry、claim/release 和 ID 专属替换逻辑）、结构组合器、局部/嵌套 theme provider、多窗口/多 root Runtime 设计、非 Clone option、借用 accessor、class 声明注册/引用检查、多悬停指针、!important/CSS origin/layer。内部 generational NodeId 继续使用，不能将它误删或当作本轮实例 ID API。

## 2. 声明、identity 与泛型

### 2.1 主题是显式 opt-in

```rust
#[component]
#[theme(primary, progress)]
fn button(caption: Rc<str>, #[default] primary: bool, #[default] progress: i32) -> impl Component {
    let _ = (primary, progress);
    text(caption)
}
```

- 只有 #[theme] / #[theme(...)] 才生成 ThemeTarget 与主题 metadata；未标注声明不产生 identity/options/resolver。普通节点仍使用统一生命周期，并可通过 with_hovered 使用 hover。
- 裸 #[theme] 使用 Options = ()，不生成空 options struct；#[theme()] 同样视为无参数 opt-in。
- 函数生成 ButtonBuilder 与同可见性的 CamelCase alias Button；alias 是类型别名，无独立 marker 或注册步骤。重名直接编译诊断，不提供兼容命名机制。
- ThemeTarget: 'static，具有 Options: 'static 和 key() -&gt; ThemeKey；ThemeKey 由 TypeId::of::&lt;Self&gt;() 得到，仅当前进程有效，无类型名字符串匹配和类型名调试 API。
- wrapper 直接返回现有 builder 不创建新 identity；独立的 #[component] + #[theme] 声明才形成新的共根 target。

### 2.2 泛型边界（Q03）

selector 接受完整 Rust 类型路径，例如 crate::widgets::Badge&lt;u32&gt;、重导出 alias 和带具体 const 实参的类型。具体实例化 identity 不同；别名不会制造不同的 TypeId。

宏必须正确区分泛型声明、impl 泛型和类型实参：struct 声明保留 type/lifetime/const 参数及 bounds，impl 使用对应参数，alias 正确转发，生成 helper 调用显式转发必要泛型。不能继续把 split_for_impl 的 ty_generics 用作 struct 泛型声明，也不能依赖仅出现在返回类型的参数自动推断。

- opt-in builder 的 ThemeTarget、PrepareNode 与 IntoNode impl 共用 Builder: 'static 边界；非 'static 借用实例不能通过这些主题构建入口。若 type/lifetime 参数参与 builder identity 但没有出现在任何值字段中，用私有 PhantomData 避免 E0392；构造时直接初始化，不引入 T: Clone/Default 等界。Const 参数按 Rust 原生声明/转发保留，无需 marker。
- 所有 builder/alias 泛型与 const 实参都正确保留，未使用参数也必须可编译。首版 option 字段不依赖开放泛型，不同具体 builder 可共享非泛型 Options；ComponentRef::options::&lt;T&gt;() 先比较 T::key() 再 downcast，不能只按 Options 类型判断 identity。
- 约束不无差别施加到未列入 options 的所有参数；未标注主题的 builder 不因主题功能获得 Clone 约束。mount 捕获和已有响应式 API 自身的生命周期约束仍成立。
- 泛型 builder 的 identity 支持不等于泛型 option 支持：声明中依赖开放 T、T::Assoc、Vec&lt;T&gt;、开放 const 参数或未确定 lifetime 的主题参数首版拒绝。与这些泛型无关的具体字段仍可进入 options。
- 具体别名/具体关联类型由 Rust 类型系统检查，宏不声称能解析任意别名底层。准入规则见下一节。

## 3. 参数与 options（Q06/Q09）

### 3.1 准入与诊断

#[theme(...)] 是函数级 allowlist。名字必须存在，不可重复，不可指向保留的 style 或框架上下文参数；未列出的文本、children、回调和资源不进入 options。显式无效标注必须编译期报错，不能静默省略字段。

首版主题值统一要求 **PropValue + Clone + static 生命周期**，通过 Access 执行 tracked owned read；Copy 是 Clone 特例。自定义 nominal 的 PropValue 实现是作者的数据值承诺，不代表宏透视私有字段。非 Clone 与借用 accessor 后置。

宏对语法可见的开放泛型、impl Trait/闭包签名、函数指针、dyn Trait 与暂不支持的 Box&lt;dyn Trait&gt; 报 syn::Error。对于别名等宏不可透视的具体类型，使用正向值类型准入和 Rust trait 检查，不靠字符串猜测：以 PropValue + Clone + 'static 为首版值语义边界，不为 fn 指针、闭包或 dyn 提供 blanket 准入。自定义 nominal 类型显式实现 PropValue；这表示作者选择将它作为值传递，不承诺检查 struct 内部每个字段。具体 alias 指向不合格类型时仍由 trait bound 在编译期拒绝。

### 3.2 存储与读取

- builder 普通参数继续接收 Into&lt;Prop&lt;T&gt;&gt;，#[model] 在函数侧使用 Signal&lt;T&gt;。
- options 私有字段统一为 Prop&lt;T&gt;，有主题参数时生成公开具体 options 类型与公开 inherent accessor，无 accessor trait。
- accessor 返回 T，内部 tracked get_clone() 可统一覆盖 Copy 与 Clone；不要求宏通过语法判断任意别名是否 Copy。
- #[model] 只有列入 allowlist 才以 Prop::Dynamic 进入 options；模型机制本身不创建主题身份。
- 未列出参数没有 accessor；selector 引用它由 Rust 方法检查报错。
- mue-core 已为 HashSet&lt;T: PropValue&gt;、Rc&lt;str&gt; 和 Arc&lt;str&gt; 实现 PropValue，静态 class 集合可直接转为 Prop&lt;HashSet&lt;ClassName&gt;&gt;。ClassName 在 UI crate 实现 PropValue；未给 fn/dyn 增加 blanket 准入。
- 默认参数在 prepare 中只求值一次；selector 匹配生效值，不区分调用方是否显式传参。#[default(...)] 不允许引用其它参数；宏检查直接引用，诊断不能要求执行任意用户宏来猜语义。
- Prop::Static 不提供内部可变性的通知；修改 Cell/RefCell 不会自动触发主题。反应性更新使用外部 Signal，本轮不增加 mutation/notification API。

### 3.3 所有权方案 B

```text
求参数生效值一次
    → 有顶层主题且本声明已 opt-in：clone 所列 Prop，Rc 持有 options
    → 原始参数 move 给 component/node prepare
    → RootBuild 保留 owned options，resolver 期间短暂借用 &dyn Any
```

无顶层主题时跳过 options 实例、clone 和主题 resolver；编译期准入不因此放宽。动态 Prop 共享只读 signal 句柄，外部源仍由创建它的 scope 管理；复制句柄不保活。函数修改 Static 副本不回写 options。

## 4. ClassName 与 classes（Q19）

```rust
fn classes(self, value: impl Into<Prop<HashSet<ClassName>>>) -> Self;

const COMPACT: ClassName = class!("compact");
let names = signal(HashSet::from([COMPACT]));
let view = button("Save").classes(names);
names.update(|set| { set.insert(class!("busy")); });
names.update(|set| { set.remove("compact"); });
```

- ClassName 包装单个名称，字段私有；格式严格为 [A-Za-z][A-Za-z0-9_-]*，安全 API 不允许修改成非法名称。
- class! 首版接受字符串字面量，在普通表达式与 const 中都必须编译期校验；非法值使用有 span 的编译诊断。可采用过程宏或强制 const 校验，不能只是调用一个可能在运行时执行的 const fn。
- 运行时 String 经 TryFrom&lt;String&gt; 等 fallible 构造返回 Result；类型不提供未经验证的 From&lt;String&gt; 直通。
- 同名静态/动态名称内容 Eq/Hash 一致，大小写敏感，不 trim/normalize；支持 Borrow&lt;str&gt;/as_str 无分配查询。内部初始可用 Cow&lt;'static, str&gt;，不公开承诺其布局；不引入 intern registry。
- 跨 crate 的 class! 展开不得直接访问私有字段。若使用 doc(hidden) 公开 const helper，它仍须校验，宏强制常量求值；不能暴露安全 unchecked 构造破坏不变量。运行时借用 str 必须转为 owned 或明确仅接受 static，不能保留悬空借用。
- 集合默认空；同一 builder 后调用完整替换先调用及其绑定，不追加/并集。没有 .class(name, active) 或 class_set。
- 不同共根 target 各自保留自己的 class 集合，禁止把外层/内层集合取并集后匹配。compound 的全部条件必须在同一个 target 上成立。
- 读取动态集合订阅整个集合，新增此前不存在名称也能重算。多来源和布尔条件由调用方显式派生集合。
- 安全集合只存已验证名称，setter/resolve 不再重复校验、panic、忽略非法项或保留旧快照。暂不检查名称预先声明或是否有规则使用它。
- 静态 HashSet 转换使用 mue-core 的 PropValue 体系；ClassName 与 Theme 在 UI 层实现 PropValue，不增加 UI 层 HashSet→Prop 转换。

## 5. 构建协议（Q15）

### 5.1 共享上下文与同根上下文分离

| 对象 | 内容和所有权 | 传递范围 |
| --- | --- | --- |
| AppBuildContext | 可捕获/恢复的顶层 Option&lt;Prop&lt;Theme&gt;&gt;；资源所有权由独立 scope 管理，Prop 句柄本身不保活外部源 | root 与动态 children 共享 |
| RootBuild | 一个真实 Node 的 target/options/class、显式样式、提前 hover owner、OwnedScope 和共享环境；终端接收 NodePlan defaults | 只沿 component 返回 builder 链 move |
| PreparedNode | RootBuild::finish 完成 seal 后的 Style、擦除 mount 闭包、OwnedScope、共享环境与 hover owners | 一次 mount |
| ChildrenPlan | 尚未执行的 child 输入/列表创建函数和资源 | 父 mount 中执行；每个 child 新建 RootBuild |

App::with_theme(theme: impl Into&lt;Prop&lt;Theme&gt;&gt;, build: impl FnOnce() -&gt; T) 在执行 build 前安装共享环境，T: IntoNode。初次 root 物化才建立它自己的 RootBuild；不能把“应用上下文”实现成全树共用的同根收集器。保留 App::new 的无主题入口。顶层安装函数不暴露为可嵌套 provider。

当前 AppBuildContext::capture 捕获顶层主题输入，run 通过带 RAII 恢复的 TLS 临时安装环境；动态 children 恢复同一输入。App::with_theme 的构建回调资源由独立 OwnedScope 持有，并由 root Node 保留。RootBuild 则显式沿返回链 move，不从 TLS 获取。

### 5.2 node 声明定稿

node 函数体属于 prepare，返回默认样式与一次性的 mount 闭包，采用 NodePlan 形态：

```rust
#[node]
#[theme(level)]
fn heading(text: Rc<str>, #[default] level: HeadingLevel)
    -> NodePlan<impl FnOnce(&mut Style)>
{
    let defaults = Style::new()
        .font_size(level.map(|level| level.default_size()));
    NodePlan::new(defaults, move |style| {
        render_text(style, text);
    })
}
```

示例遵守现有参数宏语义：源码写 T，宏生成的 prepare 函数实际接收 Prop&lt;T&gt;；#[model] 实际接收 Signal&lt;T&gt;。NodePlan&lt;F&gt; 的类型声明、构造与 PrepareNode/IntoNode 实现均要求 F: FnOnce(&mut Style) + 'static；prepare 创建资源，mount 通过 move 捕获 owned 数据，延后到 seal 完成后执行。返回类型可使用 impl FnOnce；不让宏分析函数体语句来拆分。

移除旧的特殊 style: &mut Style 函数参数；默认样式由 NodePlan 返回，调用方显式样式存 builder。不允许在 mount 后追加用于同根级联的 fallback；消费者只能读取 seal 后的结果。NodePlan::new 及其通用性由宏展开编译测试落实，不要求匿名闭包类型出现在生成 builder 的字段类型中。

component 函数体同属 prepare，返回 impl Component 或具体 builder；它不创建新 Node，不调用返回值的最终 into_node。宏求默认值/生成 options 后调用函数一次，将 RootBuild 继续传给返回 builder 的 prepare。Component 的公共能力必须包含框架可调用的 prepare 协议，不能只约束 IntoNode。

### 5.3 内部接口与阶段不变量

内部使用 PrepareNode::prepare(self, RootBuild) -&gt; PreparedNode 一类消耗式接口。为跨 crate 宏展开提供稳定的 doc(hidden) 路径；应用仍以 IntoNode 作为最终入口。终端准备完成后可擦除 mount 闭包，保证返回链可跨不同 builder 类型展开。

```text
builder 配置：普通 setter / with_hovered 回调立即执行
    → 每层 prepare：求默认参数 → 收集本层 target 与显式贡献 → 调用函数一次
    → component 返回 builder：继续同一个 RootBuild
    → node 返回 NodePlan：完成当前返回链
    → seal：统一逐属性决议并建立稳定绑定
    → 建立 Node mount 上下文与所有权 guard
    → 框架建立父 transition/paint（含 opacity）/font/transform 绑定
    → 物化父的 children：各 child 独立 prepare → seal → mount
    → 终端 mount 消费 style/layout/paint，随后完成 hooks
    → 注册 Node，提交资源所有权
```

每个真实 Node 的布局、绘制/事件消费者和注册仅执行一次。seal 固定的是贡献集合和绑定结构，不冻结属性值；theme/options/class/hover 变化只更新既有响应式图，不重跑 prepare/mount/配置回调。

prebuilt Node 仍实现最终 IntoNode identity 转换，但不能作为 component 返回链继续贡献的终点：首版 component 必须返回可 prepare 的 builder/plan。直接作为 child 的 prebuilt Node 原样接入，不借此承诺重建样式或改变其主题输入。

### 5.4 scope 与失败清理

实现采用 owned scope 与构建 guard 覆盖 RootBuild/提前状态的 builder、prepare、seal、mount 和提交阶段；配置回调内派生计算在所属 scope 中创建。context lookup 与资源 owning 关系分离。具体验证范围见实施记录。

当前 OwnedScope 拥有 detached scope，Drop 时 dispose；new_inherited 只继承 context lookup，不把资源所有权交给当前配置 scope。RootBuild 的 owner 和 Style 中提前创建的 hover owners 在 mount 时移交 Node。Scope::run、AppBuildContext::run 使用 RAII 恢复当前 scope/theme；Node mount 的 MountGuard 在失败时清理未提交节点并恢复构建栈。外部 Signal 仍由其原 scope 管理，不随节点销毁。

core scheduler 另有 effect 临时状态与 batch depth 的 unwind 恢复，不能由 scope guard 代替；相关代码见 [core runtime](../mue-core/src/runtime.rs) 与 [更新上下文](../mue-core/src/update_context.rs)。本轮保证限定为构建所拥有资源回收、构建 scope/context 与 Node 栈恢复，不承诺任意用户响应式 callback panic 后整个 Runtime 可继续工作。

没有实例 ID，首版最终 IntoNode 仍返回 Node；构建 panic 的 unwind 清理回收构建所拥有的资源并恢复构建上下文，不在本轮引入全库 Result&lt;Node&gt; API 或任意响应式回调 panic 的全局恢复承诺。外部资产等既有 Result 保持其本来错误通道；panic=abort 不承诺运行析构。

## 6. children（Q16）

- .children(...) 调用 IntoChildren::into_children_plan() 保存计划，不执行 child prepare/mount 或响应式列表计算；setter 实参表达式本身按普通 Rust 时机求值。IntoChildren: 'static；显式调用 into_children() 仍会立即转换，不能据此承诺所有 children 入口均延迟。ChildrenPlan::new 接收 `FnOnce() -> Owned<Children> + 'static`，mount()/into_children() 消费并执行计划。
- 父 prepare/seal 完成并建立 mount 上下文后物化 children；每个 child 新建 RootBuild，继承共享环境，不继承父的 target、options、class 或显式样式。
- Style.children 保存 `Option<ChildrenPlan>`，与视觉声明分离；ChildrenPlan 内部擦除一次性构造闭包，调用方不必手写 lazy closure。
- tuple 可混合 builder/prebuilt Node；Vec/数组按序支持可消费 builder 或 Node；Rc&lt;[Node]&gt; 保留已有 owning 容器语义。Rc&lt;[Builder]&gt; 无法一般地 move 消费共享元素，不承诺自动支持，可显式传 owned 容器或构造函数。
- map_keyed 返回动态 ChildrenPlan，父环境就绪后启动；输入项 T: 'static、key K: Hash + Eq + 'static、生成结果 N: IntoNode + 'static，key/view 回调为 'static FnMut。后续新增项恢复环境和每 key 的 owning scope；已有 key 复用原 Node 及其内容，删除项清理，重复 key 在重算时断言失败并报告索引。无 ID 时不为 claim 增加树事务。
- show_if/join_children 返回 ChildrenPlan。show_if 在父 mount 时构建并持有节点，即使初始条件为 false；条件为 false 时只返回空的可见列表，不销毁该节点。显式传入已物化节点/列表则原样持有，不追溯此前构建。
- Node/layout 必须能在 child mount 时提供所需父环境，父 layout 的 children 绑定在 child 可用后建立；不要求整棵树先 prepare，也不提前运行父布局消费者。

## 7. Theme、规则与级联（Q12/Q14）

### 7.1 顶层输入与组合

Theme 使用有序规则集合，顶层输入统一 Prop&lt;Theme&gt;。通过 Theme::compose(iter) 按输入顺序组合模块，后组合模块在 source order 中靠后，不增加 scope 权重。静态 Theme、Signal&lt;Theme&gt;、ReadSignal&lt;Theme&gt; 都可输入。

替换旧 Fn(ComponentRef, Style) -&gt; Style、ThemeSource 和 ThemeProvider/provide_theme；旧的 first-match theme! match 形式不保留为另一套级联入口。首版 theme! 统一生成规则描述，Rust 条件放 option 谓词，样式值仍可为 typed Rust 表达式/Prop。手写代码如需建规则必须经同一 metadata/rule 构造路径，不得绕过 specificity 和共根去重。

Theme 只能声明视觉/布局/字体/transition 等可主题化属性，不能创建 children 或事件 hooks。style! 的具体语义、单位、transform/shorthand 展开遵循现有系统；新规则表示必须保留展开后的属性未声明状态。

### 7.2 候选与排序

当前 Theme 是 `Rc<[Rule]>`，Rule 持有 `Vec<Selector>` 与声明闭包，Selector 持有 Specificity 与谓词。组合后的规则数组位置就是 source order；resolve 按规则位置遍历，用实际命中分支的最高 specificity 为该规则生成一次候选，不另设 RuleId/分支 id 类型。候选按 (Specificity, source order) 字典序排序，声明仍以稀疏 Style 参与逐属性合并。

每属性优先级：

```text
显式层：外层 component/caller > 返回链更内层
    > theme：specificity 大者优先，同权重源码较后者优先
    > 终端 NodePlan 提供的 prepare fallback
    > 框架默认值/既有继承规则
```

同一 builder 的同属性后写覆盖先写。component 函数体对返回 builder 的 .style/setter 是显式写入，与外层相同大层级，内部实现不应把其误标为 fallback；普通默认值应通过 prepare 的默认样式通道提供。hooks/children 属构建附属数据，保持既有组合与拥有行为，不参加主题属性竞争。

- 同一 compound 的 identity/class/options 条件在同一个 target 上判断；不能跨 target 拼出一次匹配。
- 对同根 targets 统一匹配：同一个 selector 分支匹配多个 target 只形成一个候选，不乘命中次数、不加 target 顺序权重。
- 逗号列表每个 selector 独立 specificity；同一声明块多分支匹配时按实际命中分支中的最高 specificity 参与，不让未命中的高权重分支抬权。每个规则位置只产出一次声明集，body 在本轮 resolve 中最多执行一次。Theme::compose 按输入顺序追加规则；重复组合同一模块会产生多个规则位置，不按闭包身份做全局去重。
- 不先对每个 target resolve 为 Style 再合并。类集合、options 不因共享 Node 而合并；hover 对同一 Node 共享。

### 7.3 动态绑定与回退

安装顶层主题且同根链包含 target 时，Node 建立一个 computed_always 总 resolver；否则不创建主题响应式图。最终显式 Style 已提供的属性保留原 Prop 绑定；对没有显式值的可主题化属性建立稳定 computed 投影，按当前主题 → fallback → 框架默认/继承取值。动态 Prop&lt;Theme&gt; 不按当前主题声明并集或首次命中裁剪，允许以后新增任何属性或删除规则。静态 Theme 也可能在谓词/style Rust 表达式中读取 Signal，不能据此推断无依赖；完全静态专用优化后置。

resolver 订阅整体动态 Theme、动态 class、实际读取的 option/hover；每次重算保留稀疏未声明状态。规则声明闭包中创建的派生计算归本次 resolver generation 的 OwnedScope 所有，结果 Style 持有 owner，旧结果释放时清理旧派生图。属性消费者计算显式 &gt; 当前 theme &gt; fallback；规则退出时恢复低层值，包括原有字体/前景继承，不能用过早填充的引擎默认遮蔽继承。Style.take_* 等消费一次后，绑定仍必须持续读取当前候选，而不是保留失效的快照。显式值自身也是 Prop，不能把它读成常量再丢掉依赖。

总 resolver 使用 computed_always；可比较的最终属性用 computed 阻止相同值向 layout/paint/transition 无效传播。不新增不可靠 PartialEq；不可比较字段保持现有传播并量化成本。无顶层主题/无 target 不分配主题响应式资源；空但动态 Theme 不等于永久无主题，仍需响应后来新增规则。

## 8. 新 selector（Q24）

```text
selector_list := compound (',' compound)*
compound      := identity? class* option? hover?
identity      := RustTypePath | '*'
class         := '.' ClassName | '.!' ClassName
option        := '[' RustBoolExprWithDollarRefs ']'
hover         := ':hover'
```

compound 至少含一项。顺序固定 identity → class* → option? → hover?，最多一个 [] 与一个 :hover。首版 ID、空格后代、&gt;、+、~ 和 :not(...) 都编译错误。class-only、*.class、裸 :hover 可用于所有已 opt-in target；* 不使普通未标注节点变成 target。option 块必须依附具名类型 identity，[$x]、*[$x]、.a[$x] 都拒绝；非布尔谓词由 Rust 类型检查拒绝。不支持 = 相等语法糖。

```rust
theme! {
    Button.compact.!disabled[$progress < 2]:hover { opacity: 80%; }
    Button[$primary], Button.selected { background: blue; }
    crate::widgets::Badge<u32> { padding: 4px; }
    .selected { opacity: 90%; }
    * { transition: 150ms ease-in-out; }
}
```

代码生成：

- Rust 类型路径由 syn 类型路径解析，支持泛型中的逗号，不把它当作 selector list 分隔。
- class 名是自定义 token lexer，支持 my-class 与 ASCII 约束；不能只读 syn::Ident，也不把任意相邻 token 字符串拼为合法名。Rust token stream 不保存一般空白，应按语法边界与 token/span 诊断，不承诺按字符空白扫描。
- [] 内先把 $ident token 映射为 hygienic 占位，再解析 syn::Expr 并展开 options.ident()；普通局部变量、枚举变体 H2、常量、字符串和方法不做 PrefixOptionPaths 改写。必须递归处理 token group（包括 Expr::Macro 的 opaque token），使 matches!($level, H2) 等实际生成 accessor 调用；仅对普通 Expr 跑 visitor 不够。不通过字符串替换。
- 删除 SelectorValue/is_set 和旧 [primary] 的 option-or-class 回退。没有 $ 的具名 target 谓词仍可使用普通 Rust bool 表达式（例如 Button[local_enabled]）；local_enabled 仅按词法作用域解析，不会隐式读 options/class。若没有对应局部值则 Rust 编译错误，不承诺无条件拒绝所有外观类似旧语法的表达式。
- specificity 为 (ID, CLASS, TYPE)：首版 ID=0；每个 .class/.!class/:hover 加一个 CLASS，每个语法出现的 $option 加一个 CLASS，具名 identity 加一个 TYPE，* 加零。&&/|| 的短路不改变语法计数。

例如 Button.primary[$active]:hover 为 (0,3,1)，Button[$a || $b] 为 (0,2,1)。逗号分支独立，不按整列表取未命中分支的权重。

## 9. Hover（Q20）

### 9.1 API 与生命周期

```rust
// Themeable: ThemeTarget + Styleable
fn enable_hovered(self) -> Self;
// StyleableExt: Styleable
fn with_hovered(self, f: impl FnOnce(Self, ReadSignal<bool>) -> Self) -> Self;
// 物化后的只读查询（省略方法体）：
// impl NodeRef { pub fn hovered(&self) -> Option<ReadSignal<bool>> { ... } }
```

- 调用时按需创建/复用初值 false 的上下文，with_hovered 立即执行一次回调，无额外 'static 限制，不保存 setter 操作队列。回调自身创建的长期计算遵守原有生命周期。
- enable_hovered 只提供主题消费入口；with_hovered 同时使能，并把只读信号交配置代码。均幂等；主题 :hover 和动态新增规则不反向安装跟踪。
- 运行期 NodeInner.hovered: Option&lt;Signal&lt;bool&gt;&gt;，None 不启用/不分配，Some 是统一真实根状态。指针系统唯一写入；同根 target 共享、旁支 child 独立；普通未 opt-in 节点也可 with_hovered。
- builder 提前状态由 RootBuild 移交，不能 mount 时换成无关 signal；NodeRef 查询不分配、不使能、不保活；未启用/已销毁返回 None。先前取得的 signal 句柄遵循现有失效规则，不承诺 Node 销毁后还能读取。
- 多个共根 builder 各自提前交出信号时，必须保留所有已发出句柄并持续更新。首版指针更新对 canonical 与所有已发出 alias 句柄在一个 mue_core::batch 中完成全部写入，再让下游 observer flush；不得用普通 watch/effect 级联桥接代替，因为 subscriber 顺序不保证 canonical 与 alias 的原子可见性。canonical 存于 NodeInner.hovered；alias handles 随同一 Node 的 hover owner 由 RootBuild 移交并按所属 scope 清理。测试验证依赖有效、同批次一致、无重复 tracker、无环及清理。不能将“增加一个字段”视为问题已自动解决。
- 为 StyleableExt 的 blanket 能力提供可取得提前 hover/context 状态的内部存储通道；覆盖生成 builder、手写 builder 以及 Style 自身，不能仅在有 ThemeTarget 的类型里存 hover。

### 9.2 命中与输入

1. 使用与普通 pointer 相同的有效命中/绘制顺序/遮挡；仅选择最上层目标，hover 路径为该目标及真实 Node 祖先。不能继续广播给所有几何相交兄弟；祖先命中与 theme 结构组合器无关。
2. transform、clip、滚动偏移和自定义 hit test 都进入同一路径计算；未裁剪 child 溢出命中时其祖先仍在路径，被裁剪部分不能命中。没有布局的容器不隐式成为无限大的遮挡层。自定义 HitTestFn 接收 (NodeRef, Vector, &mut HitTestResult)，按绘制逆序通过 do_hit_test 遍历直属 child，再按需要 hit_self；空结果表示穿透，第一个有效命中确定路径。pages 使用同一协议。
3. hover 使能只控制路径节点是否维护状态，不改变遮挡。透明/opacity=0 不代表穿透；装饰穿透通过显式命中策略，不能按“无 hover hook”推断。
4. 鼠标按下/拖动时持续按当前位置更新，和按下路径/手势捕获分离；触摸不更新 HoverState。PointerManager 创建时关闭 macroquad 的触摸模拟鼠标流，平台兼容鼠标过滤边界见实施记录。
5. 离窗/失焦清空；恢复后先确认有效光标位置再重命中，不复用失效命中。mouse_leave 同时取消按下的鼠标 track/capture；窗口失焦/最小化处理会取消全部 tracks。macroquad/miniquad 的窗口事件桥接已迁移；各平台目标编译及真实交互覆盖范围见 [实施记录](theme-refactor-validation.md#图形-smoke-与平台边界)。
6. App::frame 在布局、transform 与绘制边界发布后处理指针，每帧都会重新命中，光标不动也响应滚动、动画、节点隐藏/出现与遮挡。只更新发生变化的状态；hidden 但仍存活变 false，disposed 从跟踪记录移除。
7. hover 改变布局交给下一轮正常布局检查，不在单次输入处理中递归求稳定点；不能声称这消除用户样式造成的跨帧振荡。
8. 首版一个鼠标位置/一条路径，多个鼠标按钮共享；多悬停指针后置。

更新 signal 前复制必要状态、释放 UI runtime/Node 的 RefCell borrow；响应式更新可能同步触发布局与销毁，不持有节点表 borrow 调用户计算。单次 hover path 更新在一个 core batch 中写完 canonical 与所有已发出句柄，然后再触发 observer flush。旧路径中的 generational NodeId 先验证存活，重复 leave/dispose 不重复写失效信号。

## 10. 首版性能基线（Q23）

正确性基线采用共享 resolver + 稳定字段绑定，无主题 fast path 必须保留；静态主题仍可包含动态声明输入，不能仅看到 Prop::Static(Theme) 就当完全静态。

本轮量化无主题、空静态 Theme、有主题无匹配、静态命中、class/hover 高频、option 改变但结果相同、动态主题新增/撤销字段。记录构建次数、resolver/投影运行与通知、signals/effects、layout/paint/transition、分配与清理后存量及 p50/p95，不编造耗时门槛。

后置按 identity 索引、完全静态专用路径、只绑定可证明封闭的属性集合、per-class 跟踪、interning 与 Rc&lt;HashSet&gt; 快照优化。优化不能改变首次未命中后来命中、规则退出回退和动态主题覆盖语义。

## 11. 代码落点与迁移索引

| 模块 | 当前实现职责 |
| --- | --- |
| [mue-core scope](../mue-core/src/scope.rs) / [effect](../mue-core/src/effect.rs) | owned/detached scope 接入、unwind guard、资源移交与存活检查 |
| [mue-core Prop](../mue-core/src/prop.rs) | 保持 Access 契约，补静态集合值转换，不顺带引入借用读取 |
| [组件宏](../mue-macros/src/component.rs) | 统一参数 IR、theme allowlist、泛型正确生成、PrepareNode 与 NodePlan |
| [selector 宏](../mue-macros/src/theme.rs) / [宏入口](../mue-macros/src/lib.rs) | 新 AST/类型路径/$ 引用/class!，删除 factory clone 与旧 match 路径 |
| [构建协议](../mue-macroquad/src/node/build.rs) / [节点运行时](../mue-macroquad/src/node.rs) | RootBuild/PreparedNode/NodePlan、阶段 mount、hover 字段与只读观察 |
| [children](../mue-macroquad/src/node/children.rs) | ChildrenPlan 与动态上下文、去 factory sibling 依赖 |
| [Style](../mue-macroquad/src/style.rs) / [扩展](../mue-macroquad/src/style/ext.rs) | 稀疏声明、稳定绑定、默认与显式分流、提前 hover 状态通道 |
| [Theme](../mue-macroquad/src/theme.rs) / [App](../mue-macroquad/src/app.rs) | 规则集合、统一级联、顶层 Prop&lt;Theme&gt; 安装 |
| [命中](../mue-macroquad/src/event/hit_test.rs) / [指针](../mue-macroquad/src/event/pointer.rs) / [hover](../mue-macroquad/src/event/hover.rs) | 最上层路径、捕获分离、窗口/几何变化及 canonical/alias 同批更新 |
| [heading](../mue-components/src/typography/heading.rs) / [pages](../mue-components/src/layout/pages.rs) 等 | 所有 node 与手写 builder 迁移；默认值和消费者分离 |
| crate 导出与调用点 | 移除旧 factory API；核对旧入口无残留 |

以下顺序是实施过程的历史索引，可用于代码变更与回归定位；不是待执行计划：

1. 建立 owned scope、共享环境及 RootBuild/PreparedNode/NodePlan 构建协议。
2. 接入宏生成、显式 opt-in、泛型 identity、options accessor 与 ClassName/class!。
3. 接入延迟 children、主题规则级联、顶层 App::with_theme 与动态绑定。
4. 接入 hover 状态移交、路径命中和窗口/布局变化处理。
5. 迁移组件、示例、测试与文档，并移除 factory 和旧主题入口。

该顺序只记录实施组织过程。执行证据及未覆盖环境详见实施与验证记录。

本轮 ID 不增加 registry 测试或 ID retire/claim 流程。旧 ID 设计只在历史文档保留。

## 12. 设计不变量核对清单

### 12.1 编译期

- opt-in / 非 opt-in、空 options、公开 accessor/私有字段跨 crate 正确；泛型/const 泛型/alias/re-export、具体 TypeId 区分且 bounds 不外溢。
- 默认值一次执行，不允许直接引用其它参数；model 不隐式主题化；Clone/PropValue/'static 约束、非法 theme 名字及回调/开放泛型失败诊断。
- class! 在 const 与普通表达式编译检查，非法/空/非 ASCII 名失败；动态构造返回 Result，static/owned 同名值 Eq/Hash/Borrow 一致。
- compound 多 class/否定/option/hover 顺序、泛型路径逗号、$ 与普通 Rust 标识符、独立 specificity、非法语法均有用例。ID/组合器/旧 match 拒绝；旧 [] 的隐式 option/class 回退不存在，同时验证零 $ 普通局部 bool 谓词合法且不访问 options。
- node prepare 返回 NodePlan，component 返回可 prepare builder，泛型 helper 不误捕获外层参数；不能回退到两次执行函数体。

### 12.2 级联与响应式结果

- 外层显式 &gt; 内层显式 &gt; theme &gt; prepare fallback；同 builder 后写覆盖，属性独立，shorthand/相关字段保持正确。
- 共根多 target 不按匹配次数加权；同规则去重、逗号只用命中分支权重、相同 specificity 按模块/source order。
- class/options 条件必须在同一 target 成立，不拼 target、不合并集合；.!missing 初始命中，后来新增名称能更新。
- 动态 Theme 新增首次未命中的 width/font/transform，删除规则恢复 fallback/继承；没有重建 Node、layout consumer 或 transition binding。
- class 输入后调用替换旧绑定；旧源不再决定结果，外部源在节点销毁后仍可使用。

### 12.3 children 与生命周期

- children setter 不物化；父 seal 在前，各 child 独立 RootBuild；旁支 child 不消费父返回链。
- tuple/Vec/数组/prebuilt/Rc&lt;[Node]&gt; 和 map_keyed 后续创建恢复同一主题输入，同 key 复用，删除与隐藏的资源生命周期正确。
- 配置回调、prepare、seal、child mount、parent mount 各失败点的构建-owned 资源回收；scope/context/Node 栈恢复；若 unwind 穿过 batch/effect，再验证 batch_depth/current_effect/tracker 与临时 callback/value 恢复，否则不声明全 Runtime 可恢复；丢弃未物化 builder/plan 无泄漏。
- 所有节点与手写 builder 的 layout/paint/transform/font/transition 均消费 seal 后样式且只绑定一次。

### 12.4 hover/输入

- 重复 API 复用、共根多个提前句柄和主题状态一致、无主题普通节点可用；回调即时且不重跑；NodeRef 查询无副作用。
- 覆盖层挡住底部按钮，hover 路径只含目标及祖先，未启用覆盖层也遮挡；透明度不改变命中，clip/transform/scroll/pages 自定义路径一致。
- 按下拖出/拖回持续更新；捕获保留事件接收方，触摸无 hover；离窗/失焦清空并在位置确认后恢复。
- 静止光标遇到滚动/动画/显隐更新；销毁清除跟踪，signal borrow 不跨用户计算；hover 导致布局变化无同步递归。
- 句柄观察不保活；未启用/已销毁查询 None，提前状态与派生计算随所属生命周期清理。

### 12.5 执行与完成标准

以下是设计不变量的核对清单，不单独声明每项均已验证。已执行的自动化测试、图形 smoke 与平台限制以 [实施记录](theme-refactor-validation.md) 为准；运行测试应断言获胜声明、响应式变化、实际布局/绘制输入与生命周期，而不只编译样例。

验证记录单独维护，不能从本清单推断测试执行结果。涉及窗口或平台输入的验证须注明实际宿主条件；构建环境依赖失败不得记为测试通过。

设计完成条件是上述首版行为可复现、旧 factory 公开入口与依赖清除、示例/使用文档与实现一致，且外部动态源不被误清理。实施与验证记录描述本工作树实际执行的检查；后置功能不构成首版失败。
