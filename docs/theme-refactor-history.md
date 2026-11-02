# Theme 重构讨论历史（归档）

> 归档日期：2026-10-31。本文保存定稿前讨论；未决描述、候选 API、ID 方案和旧示例不再作为首版规范。文中“尚未实现”等状态描述均是归档时点的历史记录，不代表当前状态。
> 首版已实施。归档导航同步：2026-11-02；执行检查和平台边界见 [实施记录](theme-refactor-validation.md)。正文中的源码链接及行号对应讨论时的目录结构，已删除的 factory 文件仅用于历史追溯。
> 首版以 [实施规格](theme-refactor-implementation.md) 为准；决策索引见 [设计定稿](theme-refactor.md)。

# Theme 重构讨论稿：从 Factory 到声明式主题目标（归档正文）

> 归档时点（2026-10-31）：首版设计已定稿，代码尚未实现。实例 ID（Q21/Q22）明确后置；不实现 ID setter、ID selector 或 registry。以下正文只记录当时的讨论方向、约束和待决问题，不描述当前代码状态，也不代表现行 API。
>
> **阅读导航：** 已确认方向见第 2 节；现状与实现约束见第 3～4 节；所有尚待讨论的问题集中在[第 7 节](#open-questions)。
>
> **示例约定：** `Button` / `ButtonOptions` 是声明级身份与 options 的示意名称；主题参数通过函数级 `#[theme(...)]` 显式列出，未列出的参数不进入 options。候选方案和实施建议不等于已确认决策。

## 1. 背景

当前只有通过 `factory(...)` 创建的对象才能成为主题目标。`FactoryInstance::build_node` 是唯一解析主题的地方，`ComponentRef` 也完全围绕 factory 的 id、options、ancestors 和 siblings 组织。直接使用 `#[node]` / `#[component]` 生成的 builder 没有主题身份，也不能携带 class。

这带来了几个问题：

- 为了使用主题，必须给每个组件再包一层 factory，重复声明 `ButtonOptions`、`defaults`，并记住把同一个 handle 同时传给 `theme!` 和 `.create(...)`。
- `Node` / `Component` 本身是更自然的组件边界，但主题却要求一个额外的、值级的抽象。
- 组件作者用函数参数表达组件 API，主题作者却要面对另一套 factory options，认知是分裂的。

讨论方向是：**主题身份属于组件声明本身，而不是创建实例时传入的 factory 值**；factory 可以退回为可选的 defaults/参数化工具，最终可以直接删除。

## 2. 已确认方向与范围

1. **factory 可以直接删除**，作为重构目标，不保留兼容层。主题身份、options、class 和主题解析需要由 `#[node]` / `#[component]` 以及新的主题目标抽象接管。
2. **`#[node]` / `#[component]` 应能直接成为主题目标**，调用方不再需要先创建 factory。普通非泛型组件由宏生成的 builder 类型直接实现 `ThemeTarget`；不额外生成独立 marker struct。
3. **主题 options 采用显式 allowlist**。只有函数级 `#[theme(...)]` 列出的参数进入 options；未列出的参数（包括文本、children、回调和绘制资源）不进入 options。`#[theme]` 不列参数时仍建立主题 identity，但 options 使用 `()`。被标注参数必须通过严格的编译期合法性检查。
4. **class 的匹配只表示存在性，只提供完整集合入口 `.classes(...)`，选择器采用 CSS 式复合**。`.classes(impl Into<Prop<HashSet<ClassName>>>)` 统一接收静态 `HashSet<ClassName>` 或响应式集合；默认是空集合，同一 builder 后一次调用替换前一次输入（包括响应式绑定），不做隐式追加或并集。不保留 `.class(name, active)` 或 `class_set(...)`。传入集合完整决定当前 target 的 class，多个来源由调用方显式合成；运行期修改源 `Signal` 即可增删 class 名，无需持有 Node 或 Component。`has_class` 只检查当前集合中的存在性，不保存独立的 active:false 状态；集合变化触发主题重算。选择器写成 `element.class1.!class2[$progress < 2]:hover` 这样的 compound：`.class` 表示存在、`.!class` 表示不存在；顺序固定为 identity → class 组 → `[...]` option 谓词 → `:hover`，每个 compound 最多一个 `[...]` 块，`[]` 不再用于选 class。class 名限制为 `[A-Za-z][A-Za-z0-9_-]*`。option 用 `$` 前缀的引用；`$` 只标记 option，其它标识符（`H2`、局部变量、常量）保持普通 Rust 语义。同一 compound 内是 AND，OR 用逗号分隔选择器（见 4.5.1）。
5. **保留编译期类型检查**。目标 identity 与 `$option` 访问由 Rust 类型系统检查；class 名使用保持格式合法性的 `ClassName` 包装类型，`class!("...")` 在编译期校验字面量，运行时字符串在构造时返回 Result；不要求声明同名 option，也暂不检查 class 是否预先声明。
6. **options 所有权采用方案 B（clone 进 options）**：从生效值 clone 出 options，`Rc` 捕获给主题闭包；组件函数接收原值（move）。被 `#[theme(...)]` 标注的参数必须满足当前 options 读取和共同持有的所有权要求；`Copy` 是免费特例。
7. **Q15 的样式应用协议已确认**：`#[component]` 沿返回 builder 链传递同根构建贡献；终端 node 按 `prepare/defaults → resolve/seal → mount` 执行，完成同根 target 的统一级联后只建立一次样式绑定并物化一次 Node。旁支 child 建立独立 `RootBuild`，只继承顶层 `Prop<Theme>` 和后续明确的祖先上下文。node 声明显式拆分 prepare 与 mount，接受修改声明 API 和迁移现有函数体，不把旧的混合函数体保持兼容作为约束；不依赖宏猜测任意函数体的阶段，不通过重跑或第一次读取隐式冻结样式。具体协议见 4.3 和 Q15。
8. **被标注参数保留编译期检查**：`#[theme(...)]` 中的每个名字必须对应真实的非-style 参数；被标注类型必须可具体验证、可按 `Access` 读取，并满足泛型、回调、`Box<dyn T>` 和 options 所有权约束。检查失败必须编译期报错，不能静默降级。
9. **级联与 specificity 参照 CSS**：CSS 是级联与权重的设计依据；Rust `[]` 谓词按 `$` 引用出现次数计权；逗号列表与并列规则细节见[Q14](#questions-cascade)。
10. **ID 暂按当前 UI Runtime 约束设计**：ID 在存活的 `Node` 实例之间唯一，registry 放在 `mue-macroquad` Runtime；不按 theme scope 隔离，不建立进程级全局表。多窗口和多 root 的长期 Runtime 拆分暂不纳入本次重构。
11. **Theme 只允许在应用顶层提供**：应用建立唯一的顶层 theme 规则链；不允许嵌套或局部 `ThemeProvider` / `provide_theme`，组件和节点声明不创建新的 theme scope。顶层入口采用 `App::with_theme(theme: impl Into<Prop<Theme>>, build: impl FnOnce() -> T)`；不保留独立的 `ThemeSource` 类型，`Prop<Theme>` 同时承载静态主题和响应式换肤。
12. **Q12 的显式样式与同层顺序已确认**：调用方在 builder 上使用 `.style(...)` 或单属性 style setter 设置的值属于最高优先级；`#[component]` 函数体内部对其返回 builder 的同类写入也属于这一层。component 外层的显式 style 覆盖返回 builder 的显式 style；同一 builder 对同一属性允许后写覆盖先写。组件实现中仍不推荐手动设置样式，优先让顶层 theme 负责外观。
13. **hover 的显式使能与 trait 归属已确认**：`with_hovered(FnOnce(Self, ReadSignal<bool>) -> Self)` 属于 `StyleableExt: Styleable`，立即把状态交给回调使用；`classes(...)` 与 `enable_hovered()` 属于 `Themeable: ThemeTarget + Styleable`，后者只请求跟踪，供主题 `:hover` 消费状态。两种 hover 方法都建立或复用同一上下文、使能跟踪且幂等；主题规则不会因为出现 `:hover` 自动安装 tracker。普通未标注 `#[theme]` 的节点通过 `with_hovered` 使用 hover。详见 4.5 和 Q20。

> 以上均为讨论目标/结论；当前代码尚未这样实现。

## 3. 现状（重构前）

### 3.1 主题只在 factory 内解析

[`FactoryInstance::build_node`](../mue-macroquad/src/factory.rs) 会：

- 把 factory id 压入 thread-local 栈，收集 ancestors 和 sibling context；
- 为每个实例创建 hover signal，并挂上 `use_hovered`；
- 生成 active class 列表；
- 读取 `ThemeChain`，用 `Style::themed_defaults` 把主题样式合并进 root builder 的 style。

[`ComponentRef`](../mue-macroquad/src/theme.rs) 的字段也全是 factory 形状：id、`&dyn Any` options、classes、hovered、ancestors、siblings。没有 factory，就没有主题解析入口。

### 3.2 `#[node]` / `#[component]` 没有主题身份

- 两者生成的 builder 只有参数字段和 style，没有主题 identity、options 或 classes。
- `style` 参数目前按名字特殊处理，其余参数生成 `Prop<T>` / `Signal<T>` 字段和 setter。
- `#[node]` 直接走 `Node::build_with_style`，`#[component]` 调用函数后把调用方 style 作为默认值合并进结果（结果自身样式优先）。两者都没有“我是哪个主题目标”的信息。

### 3.3 class 当前是带 flag 的列表

以下描述的是重构前的 factory 语法；新的 compound 选择器见 4.5.1。

当前 `FactoryInstance.classes` 是 `Vec<(String, Prop<bool>)>`。主题解析时只保留 `active.get() == true` 的条目，然后交给 `ComponentRef`。

因此当前对 class 的匹配行为是：

| 写法 | 当前行为 |
| --- | --- |
| `.primary` | 只匹配 active 的 class |
| `[primary]` | `has_class("primary")` 或 `options.primary.is_set()`，编译期要求 options 有 `primary` 字段 |
| `[!primary]` | 只对 option 取反，不检查 class；factory options 是普通值，所以可以编译（若 options 字段是 `Prop<bool>` 则没有 `!` 实现） |
| `[primary == false]` | 只检查 option，不检查 class；能编译同样依赖 factory options 是普通值（`Prop<T>` 没有 `PartialEq`） |
| `:not(.primary)` | 不支持；目前只实现了 `:hover` |
| 手写 `theme` callback | 可以用 `!c.has_class(...)`，但仍无法区分“注册了但为 false”和“从未注册” |

注意：上表的 `!` 和 `==` 能成立，是因为 factory 的 options 是用户定义的具体类型（字段是普通值）；如果 options 改成 `Prop<T>`（见 4.2.2），这些表达式需要经过 accessor 或额外的 trait 才能编译。

结论：**当前没有选择 `active: false` class 的方法**；false 的 class 在进入 selector 之前就被过滤掉了，和不存在完全一样。`[primary]` 也不是纯 class selector，class-only 组件写 `[primary]` 会因为没有对应 option 字段而编译失败。

### 3.4 children 是 eager 的，factory content 是 lazy 的

`IntoChildren for T` 会立刻调用 `into_node()`。因此：

```rust
button().children(label())
```

里的 `label()` 在 `button` 的 theme 帧被 push 之前就已经构建完成。

factory 的 `.create(content)` 则把 content 保存下来，等 factory 帧建立后再构建，所以当前 `button > label` 这类 ancestor 匹配在 factory 下是可靠的。

如果直接把 `#[component]` / `#[node]` 变成主题目标，组合器会依赖调用方是传 builder 还是传已经建好的 `Node`，行为不一致。prebuilt `Node` 还会彻底丢失祖先和 sibling 信息。

### 3.5 layout 在构建时就消费了 style

`use_layout` 会调用 `style.build_taffy()`，把 width、padding、gap 等字段取走并建立 taffy 节点。因此主题**不能**在节点构建完成后再补样式，否则布局属性已经消费掉了。

这意味着：

- `#[component]` 可以在函数返回 builder 之后、`into_node()` 之前应用主题；
- `#[node]` 必须在函数体执行之前把主题注入 style，因为函数体会调用 `render_container` / `render_text` 并消费 style。

两者的 style 消费时机仍然不对称：`#[component]` 可在 builder 返回后应用根样式，`#[node]` 必须在函数体消费 style 前注入主题；统一的 target context 不改变这个布局约束。

### 3.6 factory 的 identity 会随配置变化

`Factory::defaults` 和 `Factory::style` 都会分配新的 id。也就是说同一个组件在配置前后可以是不同的主题目标，主题 handle 必须和创建实例时的 handle 完全一致。新的声明式 identity 不应该复制这个行为。

## 4. 设计说明与实现约束

### 4.1 选择器身份

主题身份归属于组件声明，不能随 builder 默认值、style 配置或某个实例变化。普通非泛型组件由宏生成的 builder 类型承担声明级 theme identity：`button(...) -> ButtonBuilder`，并生成 `impl ThemeTarget for ButtonBuilder`。builder 的字段值变化不会改变其 Rust 类型，因此不会改变主题身份；具体 options 类型仅在存在 `#[theme(...)]` 参数时生成。

主题 selector 的 identity 解析为 Rust `Path`，由 Rust 编译器负责路径、别名、重导出和类型检查；不使用函数名字符串匹配。为了让 selector 更简洁，宏默认生成 `pub type Button = ButtonBuilder`，但这是类型别名，不是新的 marker 类型，也不需要额外的 `Button` 与 `ButtonBuilder` 之间的运行时注册。

ThemeKey 使用 stable Rust 的 `TypeId` 绑定到具体 builder 类型，仅用于当前进程内匹配，不使用 `OnceLock<usize>` 分配身份，也不提供类型名调试接口。泛型组件按具体实例化类型区分 identity，selector 使用 `Badge<u32>` 这样的 Rust 类型路径语法。

别名默认由函数名转为 CamelCase，例如 `button` → `Button`；泛型 alias 应保留相应参数。泛型 parser/生成约束仍见[Q03](#questions-identity)。手写 wrapper 若返回同一个 builder 类型就共享 identity，不能仅凭 wrapper 函数名得到独立身份。

### 4.2 options 的边界

1. 主题 options 采用函数级 `#[theme(...)]` 显式 allowlist，未列出的参数不进入 options。`#[theme]` 不列参数时仍声明主题 identity，但 options 使用 `()`。
2. `#[theme(...)]` 中的每个名字必须对应真实的非-style 参数，不能重复或标注 `style`；不存在或不适用的名字编译期报错。
3. 被标注参数必须通过编译期类型检查：具体化类型可以进入；未具体化泛型参数、闭包、函数指针和暂不支持的 `Box<dyn T>` 必须报错。
4. 被标注参数必须能够通过 `Access` 的读取协议访问，并满足 options 与组件函数共同持有的所有权要求。
5. options 所有权采用方案 B：从参数生效值 clone 一份供主题持有，原值 move 给组件函数；无 ThemeChain 时跳过 options 构造和 clone。

options 所有权的剩余细节见[Q06](#questions-options)。accessor 当前返回 Copy / owned Clone 值，临时借用仅作为未来可能路线（见 Q09），不阻塞本次重构。以下代码展示已确定的 accessor 形态。

#### 4.2.1 Prop<T> 的现状与读取限制

`Prop<T>` 当前是：

```rust
pub enum Prop<T> {
    Static(T),
    Dynamic(ReadSignal<T>),
}
```

它是只读视图，不是 `Signal<T>`；builder 的 setter 通常接受 `impl Into<Prop<T>>`，Style 字段通常是 `Option<Prop<T>>`，消费时通过 `take_*()` 取走并套默认值。

当前读取方式：

| 方法 | 条件 | 是否跟踪 | 说明 |
|---|---|---|---|
| `get()` | `T: Copy + 'static` | 是 | 复制当前值 |
| `get_clone()` | `T: Clone + 'static` | 是 | 克隆当前值 |
| `get_untracked()` | `T: Copy + 'static` | 否 | 读最新值但不订阅 |
| `get_clone_untracked()` | `T: Clone + 'static` | 否 | 同上 |
| `as_static()` | `T: 'static` | 否 | 仅 `Static` 返回 `&T` |
| `get_mut()` | `T: 'static` | 否 | 仅 `Static`，且不会通知依赖 |
| `to_tracker()` | `T: 'static` | 仅跟踪 | 不读取值 |
| `map()` | `T: Clone`，`U: PartialEq + 'static` | 视情况 | 静态立即计算，动态返回 computed |

限制：

- **没有动态值的借用接口**：`as_static()` 只对 `Static` 有效；`Dynamic` 不能返回 `&T`，只能 clone。
- **`get()` 只适用于 `Copy`**；非 `Copy` 类型必须 `get_clone()`；如果连 `Clone` 都没有，就无法通过 `Access` 读取动态值。
- **`T: 'static`**：`Access for Prop<T>`、`as_static`、`get_mut` 都要求 `'static`。
- **只读**：`Prop` 持有 `ReadSignal<T>`，不是 `Signal<T>`；修改必须改底层 `Signal` 或替换 `Prop`。`get_mut()` 只能改 `Static`，且不触发依赖更新。
- **`PropValue` 门槛**：`From<T> for Prop<T>` 只对 `T: PropValue` 成立；自定义类型要显式实现 marker，闭包和 `Box<dyn Trait>` 通常不满足。
- **`Clone` 是条件性的**：`Prop<Box<dyn Trait>>` 通常不是 `Clone`，因此也无法被 options 快照。
- **`map()` 也要求 `T: Clone`**，闭包接收的是 `T` 本身而不是 `&T`。
- **没有 `PartialEq`/`Debug`**：selector 和条件判断需要先读取值；`Prop<bool>` 也没有 `Not`。

对 options 的含义：

- options 如果存 `Prop<T>`，主题解析通常需要 `T: Clone + 'static` 才能快照动态值；
- 非 Clone 的 `Box<dyn Trait>`、闭包、含泛型的值不适合作为 option；
- `as_static()` 可以避免静态值的 clone，但主题规则通常跑在 `computed_always` 里，不能假设值一定是静态；
- options accessor 需要统一的取值协议：`Copy` 类型用 `get()`，非 `Copy` 的 `Clone` 类型用 `get_clone()`；4.2.2 把这个协议收在 options view 里，主题 DSL 只看到 `$` 引用返回的普通值。

#### 4.2.2 options 的访问与存储

目标：主题宏在编译期知道具体 options 类型，运行期只持有类型擦除引用；主题 DSL 不直接接触 `Prop<T>`。

**访问链**

```rust
pub trait ThemeTarget: 'static {
    type Options: 'static;

    fn key() -> ThemeKey;

}

pub struct ComponentRef<'a> {
    key: ThemeKey,
    options: &'a dyn Any,
    classes: &'a HashSet<ClassName>,
    // ...
}

impl ComponentRef<'_> {
    pub fn options<T: ThemeTarget>(&self) -> Option<&T::Options> {
        if self.key != T::key() {
            return None;
        }
        Some(
            self.options
                .downcast_ref::<T::Options>()
                .expect("theme key/options mismatch"),
        )
    }

    pub fn has_class(&self, name: &str) -> bool {
        self.classes.contains(name)
    }
}
```

`classes` 是本次 resolve 从 `Prop<HashSet<ClassName>>` 读出的快照；`has_class` 只是 `contains`，响应式跟踪发生在 `ThemeChain::resolve` 读取动态集合时，即使查询的名字不存在，也已订阅整个集合。普通组件的 `ButtonBuilder` 是 `ThemeTarget`，`ButtonOptions` 是具体 options 类型；`Button` 若存在只是类型别名。`ThemeKey` 由具体 builder 类型的 `TypeId` 得到，与实例配置无关。没有 options 的 target 使用 `()`；有 options 的 `ComponentRef` 只在本次 resolve 中借用类型擦除的 options。

**生成的 options view**

不要把这些字段直接暴露为主题字段；生成一个类型，字段私有，方法公开：

```rust
#[doc(hidden)]
pub struct ButtonOptions {
    primary: Prop<bool>,
    progress: Prop<i32>,
}

impl ButtonOptions {
    pub fn primary(&self) -> bool {
        self.primary.get()
    }

    pub fn progress(&self) -> i32 {
        self.progress.get()
    }
}
```

非 `Copy` 的 `Clone` 类型（例如 `Rc<str>`）用 `get_clone()`；内容参数未列入 `#[theme(...)]`，因此不会出现在 options 里。

规则：

- `bool` 和 `Copy` 类型用 `get()`；非 `Copy` 的 `Clone` 类型用 `get_clone()`；
- 宏生成组件中的 `#[model]` 表示运行期动态模型；只有同时列在 `#[theme(...)]` 中时，才以 `Prop::Dynamic` 进入 options。
- 默认值先求值为生效值；selector 按生效值匹配，默认表达式只求值一次。
- 未列入 `#[theme(...)]` 的参数不生成 options 字段或 accessor；
- 没有主题参数时不生成 options 类型，使用 `()`；class-only 主题仍然可用。

方法返回普通值，主题 DSL 里就能直接写 `$progress < 32` 或 `$primary`，不需要知道 `Prop` 的读取协议。

**主题宏展开**

目标类型提供 options 类型后，主题宏可以生成：

```rust
// theme! { Button[$progress < 32] { ... } }
if let Some(options) = reference.options::<Button>() {
    if options.progress() < 32 {
        // apply style
    }
}

// theme! { Button.selected[$primary] { ... } }
if let Some(options) = reference.options::<Button>() {
    if options.primary() && reference.has_class("selected") {
        // apply style
    }
}
```

- `.class` / `.!class` 不需要 options；
- `[$option]` 没有具名 identity，应该报错（包括 `[$primary]`、`*[$primary]`）；
- option 与 class 的 OR 用逗号分隔选择器，不在同一个 `[]` 里混写；
- 旧的 match 形式可以在 arm 里绑定 `options`，类型是 `&ButtonOptions`。

**构造与所有权（方案 B）**

主题解析是响应式的，`computed_always` 可能稍后重跑，所以 options 必须是 owned 值，不能把局部借用传进去。采用方案 B（见 4.2.3）：先从生效值 clone 出 options，再把原值 move 给组件函数。

```rust
let primary = self.primary.unwrap_or_else(|| Prop::Static(default));
let progress = self.progress;

// 有 ThemeChain 时：先 clone 一份 options 给主题闭包
let options = Rc::new(ButtonOptions {
    primary: primary.clone(),
    progress: progress.clone(),
});

let themed = computed_always({
    let options = Rc::clone(&options);
    move |_| {
        themes.resolve(
            Button::key(),
            options.as_ref(),
            &classes,
            hovered,
            // ...
        )
    }
});

let result = button(
    caption,                    // 未列入 #[theme(...)]，直接 move
    primary,                    // 原值 move 给函数
    progress,
);
```

- options 是只读副本；组件函数接收原值（move）；
- 被标注参数必须满足当前读取和共同持有的所有权约束；Copy 是免费特例；
- `Prop::Static(T)` 的 clone 克隆 `T`，`Prop::Dynamic` 的 clone 只复制句柄；
- 组件函数拿到原值后，如果对 `Prop::Static` 调用 `get_mut()` 修改本地副本，options 不会同步；需要共享状态时用 `Signal`（`Prop::Dynamic` 只共享句柄，不能通过 `Prop` 修改）；
- 未标注的闭包、泛型和 `Box<dyn Trait>` 不进入 options；若显式标注但不满足检查，则必须编译期报错；
- `#[default]` 先求值一次，clone 进 options，原值 move 给函数；
- 没有 `ThemeChain` 时跳过 options 构造和 clone，直接走原来的 move 路径，保留 fast path；
- options 为空时使用 `()`，不生成 options 类型；
- target 持有 `.classes(...)` 传入的 `Prop<HashSet<ClassName>>`，主题闭包保留该输入，`ComponentRef` 只借用本次 resolve 读出的 `&HashSet` 快照。动态源信号由创建它的 scope 管理，复制句柄不会延长其生命周期；节点清理主题订阅不应销毁由外部 scope 管理的源信号。

上例只展示 options 的所有权关系，不展开 Q15 的执行细节。component 和 node 现在统一遵循 4.3 的 prepare/defaults → resolve/seal → mount 协议。

**为什么用 accessor method**

如果直接公开 `Prop` 字段：

```rust
pub struct ButtonOptions {
    pub primary: Prop<bool>,
    pub progress: Prop<i32>,
}
```

主题宏就必须知道哪些字段用 `is_set()`、哪些用 `get()` / `get_clone()`、字段是不是 `Signal<T>`，还要处理跨 crate 的 `Access` import。accessor method 把这些都收在组件声明侧：

- 主题 DSL 看到的是普通 `bool` / `i32` / enum；
- `Prop` 的读取限制不会扩散到每个主题；
- 字段保持私有，方法可以公开；
- 未列入 `#[theme(...)]` 的参数没有 accessor，主题 DSL 引用它时会得到清晰的编译错误；
- 类型检查仍然落在具体 options 类型上；
- 方法内部调用 `get()` / `get_clone()`，在 `computed_always` 里仍然会 track，保持 options 的响应式。

代价是多生成一层薄方法，通常可以内联。

**备选方案**

- **公开 `Prop` 字段**：代码生成简单，但把 `Prop` 的读取协议暴露给主题宏，且字段成为公开 API；不推荐作为长期方案。
- **options 存 plain snapshot**：`pub primary: bool`、`pub progress: i32`，主题表达式最简单，但 options 不再响应式；如果确定 options 只是构建期快照，这也可行，运行期状态参数 需要单独处理。
- **擦除成 name → value 的 map**：运行期灵活，但丢失编译期字段检查，不建议。
- **不生成 options 类型，改用 accessor trait**：不采用。存在主题参数时生成具体 options 类型、私有字段和公开 inherent accessor 方法；无主题参数时直接使用 `()`。

开放事项统一见[第 7 节 options 问题](#questions-options)。

#### 4.2.3 方案 B 的成本与替代方案

方案 B 已采用：默认值求值一次，clone 进 options，`Rc` 捕获 options，函数接收原值。静态 `Prop<T>` 的 clone 克隆 T；动态 Prop 复制信号句柄。组件函数修改静态本地副本不会同步到 options，共享更新应使用信号。

现有方案 B 的普通静态参数构造需要 `T: Clone + 'static`；这是构造时的共同持有约束，与 resolve 时如何读取不同。非 Clone 静态参数是否支持、如何共享仍见[Q06](#questions-options)。临时借用只能减少读取时复制，不能使一个值被 move 两次。

未采用的所有权替代：move 进 options 再给函数 clone（没有额外收益）；借用 builder 字段（不能满足后续响应式计算的生命周期）；把字段改成 `Rc<Prop<T>>`（改变 builder API，仍不解决非 Clone 动态值的读取问题）。

#### 4.2.4 已确认的参数、默认值与响应式契约

```rust
#[node]
#[theme(level)]
pub fn heading(
    style: &mut Style,
    text: Rc<str>,
    #[default] level: HeadingLevel,
) {
    // 只有 level 进入 options；text 继续作为普通内容参数。
}
```

- 只有函数级 `#[theme(...)]` 列出的参数生成 options 字段和 accessor。未列出的参数无需 `#[skip]`，也不参与主题参数检查。裸 `#[theme]` 表示有主题身份、无主题参数，`ThemeTarget::Options = ()`，不生成空 struct。没有 `#[theme]` 的 node/component 不启用主题目标，不生成 ThemeTarget、options 或主题 accessor。
- 保留编译期检查：参数名必须存在、不可重复、不可指向 `style`；被标注类型不可含未具体化泛型、闭包、函数指针或暂不支持的 `Box<dyn T>`，并须满足 `Access` 读取和 options 所有权约束。失败必须报错，不能静默忽略标注。类型别名/关联类型的检查方式按已确定的编译期检查约束实现。
- selector 按默认值求出的生效值匹配，不区分是否显式传参。`#[default(...)]` 仅求值一次，不允许引用其它组件参数。
- options 字段统一保存 `Prop<T>`，普通值为 `Static`；保留的 `Signal` / `ReadSignal` 句柄转换为 `Dynamic`。`#[model]` 决定函数侧的动态可写模型，只有列入 `#[theme(...)]` 时才以只读 `Prop::Dynamic` 暴露给主题。
- `Dynamic` 的 tracked read 在主题计算中建立依赖；提前 `signal.get()` 再放入 `Static` 只保留当时的值。`Static` 只表示外层分支不提供追踪，不保证内部 `T` 完全不可变；`Cell` / `RefCell` 等修改不会自动通知。相关显式 mutation/notification API 仍待后续设计。
- 有主题参数时生成公开的具体 options 类型、私有字段和公开 inherent accessor 方法；不生成 accessor trait。未声明 option 的访问由编译期字段/方法检查拒绝。参数重命名不提供兼容层，不将已知的编译错误改成运行期 panic。
- `[]` 保留普通 Rust 布尔表达式和 `==`；不增加 `=` 相等判断语法糖。

### 4.3 应用点与级联顺序

**Theme 的放置与作用域**：应用在顶层建立唯一的 theme 规则链，根视图及其子树使用这条规则链。组件和节点可以声明 `ThemeTarget`、options 和 class，但不能在子树内追加主题规则、建立局部 `ThemeProvider` 或创建新的 theme scope。规则可以分散定义在独立模块，再由顶层统一组合和提供；限制的是安装位置，不是源码定义位置。主题规则的适用范围由 selector 决定。

顶层主题入口必须在根视图构建前安装，并接收构建闭包：

```rust
impl App {
    pub fn with_theme<T>(
        theme: impl Into<Prop<Theme>>,
        build: impl FnOnce() -> T,
    ) -> Self
    where
        T: IntoNode,
    {
        // 在执行 build() 前建立顶层 RootBuild。
        // 具体实现由 App/Runtime 保持根构建上下文。
    }
}
```

`Prop<Theme>` 是顶层主题的唯一输入表示：`Prop::Static` 用于静态主题，`Prop::Dynamic` 用于响应式换肤。`Theme` 需要满足 `PropValue`，以便静态主题直接转换为 `Prop<Theme>`；`Signal<Theme>` 和 `ReadSignal<Theme>` 继续通过现有转换进入动态分支。不保留独立的 `ThemeSource` 类型。主题切换只重新计算主题规则和最终样式，不重新执行 `build`、component、node prepare 或 mount。动态 children 创建时恢复同一个顶层构建上下文。

**Q15：prepare → resolve/seal → mount**

Q15 区分可共享的顶层构建上下文与每个真实 Node 独立的 `RootBuild`（名称是实现示意）：前者携带顶层 `Prop<Theme>`，后者把同根 target、options、class、显式样式和默认值交给最终真实 Node。`RootBuild` 只沿 `#[component]` 的返回 builder 链传递；component 返回另一个 component builder 时递归执行同一协议。函数体中创建的旁支 child 建立自己的 `RootBuild`，不消费父 component 的同根贡献，只继承顶层主题输入和后续明确的祖先上下文。不能把同根收集器放进任意 `into_node()` 都能消费的全局 TLS，也不能让整个应用共用一个 RootBuild。

进入以下三阶段前，调用方创建 builder 并执行普通 setter；`with_hovered` 在方法调用处取得 hover 上下文并立即执行配置回调。回调可以先建立依赖 hover 的参数，再由 prepare 求出该 builder 的默认参数、构造 options 并执行 component 函数体；无需为了它把普通 setter 改成延迟操作队列。builder 阶段提前建立的状态需在后续构建中正确保活和清理，具体要求见 4.5.2。

协议分为三个阶段：

1. **prepare/defaults**：计算参数的生效值，构造 options 和 class metadata，创建 node 所需的信号、资源和默认样式，并把它们登记到当前 `RootBuild`。node 的默认样式必须在这里准备完成；不能依赖 mount 后再补，也不能为了收集默认值重跑 node 函数。现有 node 函数体若同时承担默认值准备和布局/绘制消费，需要迁移到明确的 prepare 与 mount 边界。
2. **resolve/seal**：对同根的所有 target 做一次统一级联，保留每个属性的稀疏声明、selector specificity 和 source order，得到最终的响应式样式绑定图。显式 style 的优先级最高，顶层 theme 次之，fallback 默认值再次之；component 外层的显式 style 覆盖返回 builder 的显式 style；同一 builder 对同一属性允许后写覆盖先写。多个 target 共用一个真实 Node 并命中同一属性时，权重一致按源码先后顺序决定胜负。
3. **mount**：只在 seal 完成后绑定 transition、transform、font 等节点状态，调用 `use_layout`、`use_paint`、绘制/事件 hook，并注册真实 Node。每个真实 Node 只建立一次这些消费者和一次物化；主题、options、class 变化只更新已经建立的响应式样式绑定。

**node 声明的阶段边界（已确认）**：允许为重构修改 `#[node]` 的声明 API，并迁移现有 node 函数体。默认样式和资源准备属于 prepare；函数显式交出延迟执行的 mount 部分，后者在 seal 之后接收完成级联的样式，再建立 layout、paint、transition 等消费者。宏负责生成 builder 和接入构建协议，不尝试自动分析任意函数体来拆阶段，也不重跑函数体。`NodePlan(defaults, mount)` 是这一方向的候选接口示意；具体类型、trait、构造语法和资源/scope 的移交方式仍待设计，不将该名称视为已定公开 API。

共根 target 的主题声明不能先分别 resolve 成普通 `Style` 再合并，因为这样会丢失 specificity、source order 和“未声明”状态。Theme 的规则表示必须保留逐属性候选及其元数据；现有 `Fn(ComponentRef, Style) -> Style` 形式不足以支持跨 target 的统一级联。

该协议解决现有 node/component 的不对称时序：`use_layout` 在构建时消费 style，因此主题必须在 mount 前完成；transition、transform 和 paint/layout 消费也必须统一移动到 seal 之后。已物化的 prebuilt Node 不重新 materialize，也不因后来挂到新父节点而补回缺失的主题结构上下文。

如果首版保留 `>` / descendant / sibling selector，祖先和兄弟上下文仍需覆盖延迟创建的 children；这属于结构匹配问题，不是 theme scope 继承问题。

顶层 theme 不允许在子树内重复安装。规则可以分模块定义，但只能由应用顶层组合；主题规则的动态变化覆盖已经存在的 target 和之后创建的 target。

### 4.4 祖先与组合器的构建约束

Q15 的 `RootBuild` 只沿 component 返回 builder 链传递。函数体中创建的旁支 child 不消费父 RootBuild，而是建立自己的 RootBuild；它只继承顶层 `Prop<Theme>` 和后续明确的祖先上下文。不能用任意 `into_node()` 都可消费的临时 TLS 代替这个 continuation 协议。

`children()` 当前会 eager 物化子 builder；Q16 的设计已收口并并入 Q15 实现：setter 只保存待构建的 children 描述，不执行 child component/node 函数体，也不物化 child builder；调用方无需手写 lazy closure。父返回链完成 prepare/seal 并建立父 mount 上下文后，各 child 以独立 RootBuild 执行自己的 prepare → seal → mount，父 layout 等消费者再接入物化后的 children。seal 的单位是单个真实 Node 的同根返回链，不要求整棵树先完成 prepare。

动态 children 同样保存构建计划：例如 map_keyed 先保存列表输入、key 函数与 builder 创建函数，在父上下文就绪后才启动计算；后续新增 key 恢复 owned 构建上下文并创建独立 RootBuild，保留 key 继续复用原 Node。保存的是顶层 Prop<Theme> 输入，不是主题值快照。待构建内容由 builder/children 计划持有，未执行就丢弃时释放已持有资源；物化后的 child 由 children 容器持有，scope 随节点清理，失败时回收本次构建资源。具体描述类型、trait、容器接入与清理 guard 随 Q15 落实，不再作为独立架构选择，见[Q16](#questions-children)。

已经物化的 `Node`、`Vec<Node>`、`Rc<[Node]>` 以及含已物化节点的 tuple，其响应式 scope、layout 和 effect 已建立；再次调用 `into_node()` 不会重新构建。若它们构建时已能访问顶层 theme，单 target 的主题匹配可以正常执行；挂到新父节点下不承诺事后补回缺失的祖先/兄弟匹配上下文。

Q24 已确定首版不支持 `>` / descendant / `+` / `~` 结构组合器；首版只实现单个 compound selector 和逗号 selector list。组合器的匹配关系属于 target/节点结构，不属于 provider scope；未来重新加入时，参与匹配的具体树和中间布局节点语义由 Q17 确定，reactive children 的增删重排上下文由 Q18 确定。

### 4.5 class 与 hover

- **已确认：只提供 `.classes(self, classes: impl Into<Prop<HashSet<ClassName>>>) -> Self`**。静态集合、`Signal<HashSet<ClassName>>`、`ReadSignal<HashSet<ClassName>>` 和 `Prop<HashSet<ClassName>>` 使用同一入口；实现时需补充静态 HashSet 到 Prop 的转换。默认集合为空，同一 builder 后一次调用替换前一次输入，包括此前的响应式绑定。
- 传入集合完整决定当前 target 的 class；builder 不额外追加隐藏的名称，不保留 `.class(name, active)` 或 `class_set(...)`，也不合并不同输入。多个来源或布尔条件由调用方通过 `computed` / `map` 等显式生成最终集合。共根构建仍保留各 target 的输入归属；同一 builder 的替换规则不表示将不同 target 的集合自动合并。
- `has_class(name)` 只查询当前集合中的存在性，不保存 active:false 或“注册但未激活”状态。选择器里 `.class` 表示存在、`.!class` 表示不存在（见 4.5.1）；主题计算中读取动态集合会建立依赖，新增原先不存在的名字也能触发重算。
- 运行期增删通过修改外部源 `Signal<HashSet<ClassName>>` 完成，无需持有 Node 或 Component；组件和主题只持有只读输入。集合变化只更新已有的主题和样式绑定，不重新执行 component/node 构建。HashSet 可以直接存入 Signal；`Rc<HashSet<ClassName>>` 仅是减少快照复制的一种实现优化，不是响应式或所有权的必要条件，是否采用及转换方式留待实现与测量。
- 当前 factory 创建 hover signal 并挂 `use_hovered`。新设计通过 `Themeable::enable_hovered()` 显式请求跟踪，供主题 `:hover` 消费状态；普通响应式配置使用 `StyleableExt::with_hovered`，立即取得只读状态并配置 builder（见 4.5.2）。`:hover` 不自动安装 tracker，剩余的指针命中语义见[Q20](#questions-class-hover)。
- 新语法的 class 名允许 ASCII 字母、数字、`-`、`_`，且必须以英文字母开头；当前实现按 Rust ident 解析，需要自定义 class-name lexer（见 4.5.1）。

**ClassName 与构造入口（已确认）**：包装单个名称，不额外包装整个集合。`ClassName` 字段私有，不暴露可绕过验证的字符串修改接口；通过公开安全 API 得到的值始终满足 `[A-Za-z][A-Za-z0-9_-]*`。此决定以 `HashSet<ClassName>` 取代此前的 `HashSet<String>` 输入约定，完整集合替换、动态增删及响应式语义不变。

- 字面量使用 `class!("compact")` / `class!("my-class")` 构造，可用于 const；即使调用处不是 const，宏也必须保证编译期校验，非法名称编译失败。仅把普通构造函数标为 const fn 不足以保证这一点。具体使用过程宏诊断还是强制常量求值，可在实现时选择。
- 运行时字符串使用 fallible 构造，例如 `ClassName::try_from(input_string)`，返回 Result；非法输入在名称构造边界交给调用方处理，不留到 theme resolve 时 panic、忽略或恢复旧集合。
- 静态和动态集合都只接收 ClassName；动态集合中的代码字面量仍可通过 class! 在编译期确认。`.classes(...)` setter 和主题解析不反复校验集合项，也不提供未经验证的 String 集合直通入口。
- Clone、Eq、Hash 按名称内容工作，静态与运行时构造的同名值相等；支持按 str 借用查询，维持 has_class(name: &str) 和集合 remove("compact") 的无分配查询。底层 Cow<'static, str> 是候选存储实现，不作为公开表示承诺。
- `class!` 只构造名称，不是被取消的 `.class(name, active)` 单项绑定 API。暂不引入 class 声明/注册与引用检查，也不通过该宏证明名称已被任何 selector 使用。

```rust
const COMPACT: ClassName = class!("compact");
let runtime_name = ClassName::try_from(input_string)?;
let classes = signal(HashSet::from([COMPACT, runtime_name]));
let view = button("Save").classes(classes);

// class!("2compact")：编译期错误，名称必须以 ASCII 字母开头。
```

**接口归属（已确认，尚未实现）**：

```rust
pub trait Themeable: ThemeTarget + Styleable {
    fn classes(self, classes: impl Into<Prop<HashSet<ClassName>>>) -> Self;

    fn enable_hovered(self) -> Self;
}

pub trait StyleableExt: Styleable {
    // 其它已有扩展方法省略。
    fn with_hovered(
        self,
        configure: impl FnOnce(Self, ReadSignal<bool>) -> Self,
    ) -> Self;
}
```

`ThemeTarget` 提供声明级 identity/options，`Themeable` 在此基础上提供主题目标配置。未标注 `#[theme]` 的 node/component 不获得 ThemeTarget/Themeable，但仍可通过 StyleableExt 使用 `with_hovered`。两种 hover 入口按状态的消费方式划分：`enable_hovered` 不交出信号，状态由主题 `:hover` 使用；`with_hovered` 在配置回调中直接提供信号，调用方可以立即建立响应式依赖。仅使能跟踪并不能为普通配置代码提供状态的消费入口，因此 `enable_hovered` 不放入 StyleableExt。

底层 hover 上下文和跟踪实现由两个入口共享；`with_hovered` 不要求 `Self: Themeable`，调用也不隐式创建主题身份。运行期状态位置已确认采用 `NodeInner.hovered: Option<Signal<bool>>`，具体提前状态接入与资源移交见 4.5.2；trait 归属本身不预设 blanket impl。

#### 4.5.1 复合选择器（最新结论）

旧提案用 `[.xxx]` 选 class、`[xxx]` 选 option，但两者共享 `[]` 命名空间，class-only 组件仍需要同名字段；option 的裸标识符还会和表达式里的枚举变体、局部变量、常量冲突。新决策改成 CSS 式 compound：

```text
element.class1.!class2[$progress < 2]:hover
```

```text
compound   := identity? class* option? hover?
identity   := path | '*'
class      := '.' class_name
            | '.!' class_name
option     := '[' option_expr ']'
hover      := ':hover'
class_name := [A-Za-z][A-Za-z0-9_-]*
```

`option_expr` 是 Rust 布尔表达式，其中 `$ident` 是 option 引用。

`element` 是 identity 的占位，实际写 builder 类型或其类型别名（例如 `Button` / `ButtonBuilder`）。各部分含义：

- `.class1`：class 存在。
- `.!class2`：class 不存在；class 存储仍然只有“存在/不存在”，不是 active:false 状态。
- `[$progress < 2]`：option 谓词。`$` 只标记 option 引用；`H2`、局部变量、常量等其它标识符保持普通 Rust 语义。
- `:hover`：目前唯一支持的伪类。

示例：

```rust
theme! {
    Button[$primary] { background: blue; }
    Button.primary { color: white; }
    Button.compact.!disabled { padding: 8px; }
    Button.selected[$primary] { background: blue; }
    // OR: 同一 body，逗号分隔
    Button[$primary], Button.selected { background: blue; }
    Button[$level == H2]:hover { opacity: 80%; }
    .selected { opacity: 90%; }
    * { transition: 150ms ease-in-out; }
}
```

规则：

- class 只用 `.xxx` / `.!xxx`；`[]` 只用于 option 谓词，class 不再进 `[]`，`[.xxx]` 删除。
- `.class` / `.!class` 的匹配读取 `.classes(...)` 输入在本次 resolve 的集合快照（`HashSet`），只区分存在与不存在；动态集合变化（包括运行期增删名字）会自动重算主题。
- 多 class 直接相邻，例如 `.a.b`；class 项之间顺序不限，compound 的相邻部分本来就是 AND，不需要也不允许在 class 之间写 `&&`（`&&` / `||` 只用于 `[]` 内的 option 表达式）。
- option 谓词必须依附具名 identity：`[$primary]`、`*[$primary]` 编译期报错。class-only 写 `.selected`，universal 写 `*`。只有 class、没有 identity 的 `.selected[$primary]` 也要报错，因为 option 需要具体 options 类型。
- `$` 只在 `[]` 内解析；选择器里 `[]` 外出现 `$` 应报错。
- 同一 compound 内各部分是 AND；option 与 class 的 OR 用逗号分隔多个选择器。不在 `[]` 里混写 `.class`。
- 每个 compound 最多一个 `[]` 块，且必须在所有 class 之后；`[]` 之后只能跟 `:hover`。多写 `[]`、把 `[]` 放到 class 前、把 `:hover` 放到中间，都编译错误。块内用 `&&` / `||` / `!` / 括号组合 `$` 引用；`!$disabled` 是普通 bool 取反，非 bool option 必须写成比较或方法调用（如 `$level == H2`）。
- 顺序固定：identity? → class* → `[]`? → `:hover`?。identity 如果存在必须在最前；class 组在 `[]` 之前；`:hover` 必须在最后。
- `Button.primary` 是 class，`Button[$primary]` 是 option；两个命名空间在语法上分开。旧 `[primary]` 的“option 或 class”回退删除。
- `:not(...)` 仍不支持；否定 class 用 `.!class`。`*.selected` 等价于 `.selected`，但推荐后者。
- class 名不规定语义命名，但只允许 `[A-Za-z][A-Za-z0-9_-]*`：ASCII 字母、数字、`-`、`_`，且必须以 ASCII 字母开头；`.classes(...)` 的静态和动态集合项也遵循此格式。
- selector 中的非法 class 名使用带 span 的 `syn::Error`；`class!("...")` 也必须在编译期拒绝非法字面量。运行时字符串通过 ClassName 的 fallible 构造验证，动态集合只存已验证的名称；不在主题解析中重复校验或为非法集合项 panic，见[Q19](#questions-class-hover)。

**与旧语法的差异**

- 旧 `[primary]`（option + class 回退）删除；factory 也直接删除，不做过渡。新写法是 `Button[$primary]` 或 `Button.primary`。
- 旧 `[.primary]` 提案删除；class 直接写成 compound 的 `.primary`。
- 旧 `button.primary` 中的 `.primary` 保留为 class 语法，但语义统一为 `has_class`；它现在是 compound 的 class 部分。
- 旧 `*[.selected]` 改为 `.selected`；旧 `*[option]` 改为错误（option 需要具名 identity）。

##### 解析与代码生成约束

- **compound 的解析**：identity 可以省略（`.selected`）；有 identity 时必须在最前；之后依次解析 class 组（`.class` / `.!class`，可重复）、最多一个 `[$...]`、最多一个 `:hover`。多写 `[]` 或顺序错误直接报错。当前“一次 class 后缀 + 一个 bracket”的解析器需要重写。
- **class 名不是 Rust ident**：`-` 会被 tokenize 成单独的 `-`，所以 `.my-class` 是 `.` `my` `-` `class` 多个 token；不能直接用 `syn::Ident`。写一个 class-name lexer：首字符 ASCII 字母，后续 ASCII 字母/数字/`-`/`_`，在 `.`、`[`、`:`、组合器或空白处结束；非法字符用 `syn::Error` 报错并带 span。
- **`$ident` 不是合法 Rust 表达式前缀**：先把 `$ident` 在 token 层替换成 hygienic 占位 ident（不要做字符串替换，避免误伤字符串、注释、方法调用），用 `syn::Expr` 解析整个 `[]`，再把占位 ident 改写成 `options.ident()`。这样 `H2`、局部变量、常量和方法调用都保持原样。
- **`.!class` 的解析**：`.` 后接受 class-name lexer 的结果；`!` 紧跟 `.` 表示取反。`[]` 外的 `.` 才表示 class，`[]` 内的 `.` 属于 Rust 表达式（字段/方法调用）。不要用字符串替换，避免把 `.5` 误判成 class。
- **class-only 不要求 options**：`.selected` 在 options 不存在时也要能工作；只有出现 `$` 引用时才要求具名 identity。
- **option 与 class 的类型规则不同**：`$primary` 走 accessor（返回 `bool`），`.primary` 走 `has_class`；`$progress < 32` 是普通 Rust 比较。展开前按 token 区分，不靠 AST 猜测。
- **specificity**：遵循 CSS 分列比较；`:hover` 按 CSS 伪类增加一个 CLASS 权重，`[]` 按 `$` option 引用的出现次数计 CLASS 权重。例如 `Button.primary[$active]:hover` 为 `(ID, CLASS, TYPE) = (0, 3, 1)`；selector 列表按 Q14 规则处理，不再引入 provider scope 权重。
- **删除 `PrefixOptionPaths` 的全局单段路径改写**：`$` 已经提供了显式边界，不再需要猜测裸路径是不是 option。
- `$primary` 直接使用 accessor 返回的 `bool`，不需要旧的 `is_set()` / `SelectorValue`；后者只服务于 factory 旧语法，最终可以删除。

具体未决项统一列在第 7 节。

#### 4.5.2 `with_hovered`：builder 阶段的即时配置

**已确认接口，尚未实现。** 保留 `with_hovered` 名称，放入 `StyleableExt: Styleable`，作为 builder 的配置方法；回调接收当前 builder 和框架提供的只读 hover 信号，无需 ThemeTarget/Themeable：

```rust
fn with_hovered(
    self,
    configure: impl FnOnce(Self, ReadSignal<bool>) -> Self,
) -> Self;
```

调用 `with_hovered` 时立即创建或取得 builder 保存的 hover 上下文，然后立即执行一次 `configure(self, hovered)` 并返回配置后的 builder。回调不保存到 prepare，也不要求 `'static`；回调内部创建的长期响应式计算仍须满足其自身的生命周期约束。取得信号不需要先物化 Node 或分配 NodeId，初始 hover 值为 `false`。

```rust
let view = text("普通状态")
    .opacity(0.5)
    .with_hovered(|view, hovered| {
        view
            .text(hovered.select("悬停中", "普通状态"))
            .opacity(hovered.select(1.0, 0.6))
            .transition(Transition::ease_in_out(0.2))
    })
    .opacity(0.8);
```

上例中回调当场执行，最后的 `.opacity(0.8)` 按既定的“同一 builder 后写覆盖先写”规则覆盖回调设置；文本保留响应式切换。普通 setter 继续直接配置 builder，不需要为此引入延迟回调队列或配置操作队列。配置回调不会提前执行 component 函数体，也不会提前建立 layout/paint 消费者。

构建顺序为：

```text
构造 builder、求值调用方参数表达式
    → 普通 setter / with_hovered 回调按调用顺序立即执行
    → prepare：默认参数求值、options 构造、component 函数逐层展开、终端 node 准备
    → resolve/seal：完成同根 target 的统一级联
    → mount：最终 Node 接管 hover 上下文并接入指针更新，建立样式消费者
```

只有配置回调是即时执行；hover 之后的变化通过 `select`、`map`、`computed` 等建立的依赖更新 text、option、class、style 或动画目标，不重新执行配置回调或重建组件。回调中直接用 `hovered.get()` 写普通 `if` 只会按当时的状态选择一次，不会自动形成持续切换。

builder 保存 hover 上下文，返回链上的 `RootBuild` 将其传给最终 Node。调用方取得只读句柄，不再通过传入可写 `Signal<bool>` 充当状态输出端；此方案取代此前把新接口设计为 `.use_hovered(signal)` 单向输出绑定、独立构建函数，以及把 `with_hovered` 回调延迟到 prepare 的提案。当前旧 `use_hovered` 的实现与迁移仍需随运行时代码处理。

同一 builder 重复调用 `with_hovered` 时复用已有 hover 上下文。多个 builder 最终共用真实根时，回调、主题 `:hover` 和 Node 的输入处理必须观察同一真实根状态；旁支 child 的状态独立。显式调用该方法即声明 hover 跟踪需求，无顶层 theme 或未标注 `#[theme]` 的节点也可以使用。主题目标只需为 `:hover` 启用跟踪而不在配置代码中消费信号时，使用 `Themeable::enable_hovered()`。两者都不创建 ThemeTarget；`enable_hovered` 要求调用者已具备 ThemeTarget，`:hover` 规则不会反向自动安装跟踪。

**运行期状态位置（已确认）**：在 `NodeInner` 中增加 `hovered: Option<Signal<bool>>`。`None` 表示该真实 Node 未显式启用 hover，不分配 hover 信号，主题 `:hover` 不匹配；`Some(signal)` 是该真实 Node 的统一 hover 状态，由指针系统更新，主题与配置代码取得只读视图。同根 target 共享这一真实 Node 的状态，旁支 child 独立。信号及派生计算的资源由 Node 持有的 scope 管理，字段中的 Signal 只是句柄，不能靠复制句柄保活。builder 阶段按需提前创建的状态经 RootBuild 移交给最终 Node；mount 不另建一个与提前句柄无关的信号。

**指针命中与遮挡语义（已确认）**：hover 与普通指针输入使用同一套有效命中、绘制顺序和遮挡规则。每次命中只选一个最上层目标；hover 路径是该目标及其真实 Node 祖先，路径上的已启用节点更新状态，重叠但被遮挡的兄弟不同时 hover。决定命中/遮挡时不因节点是否启用 hover 而改变排序或穿透；未启用 hover 的覆盖层仍会遮挡下面的节点。若要穿透，须由独立、明确的命中策略表达；视觉透明或 `opacity: 0` 本身不代表穿透。实现需要命中路径，而不能继续把当前返回所有相交节点的集合直接当作 hover 集合。

**输入行为（已确认）**：首版鼠标无论是否按下，都按当前位置的有效命中路径更新 hover；按下本身不改变 hover，拖动捕获不把 hover 锁在起始节点。拖动事件接收方与 hover 目标分别维护：从 A 按下并拖到 B，A 可继续接收拖动事件，但在 A 不再位于当前命中路径时其 hover 为 false，B 在当前路径上则为 true；共同祖先按路径规则保持状态。触摸不产生 hover，触摸反馈使用 pressed/手势状态，触摸模拟的鼠标事件也不计入 hover。鼠标离开窗口或窗口失焦时清除 hover；恢复后根据重新确认的光标位置计算，不保留失效的旧命中。

**界面变化与观察接口（已确认）**：光标不动时，滚动、布局、transform、节点显隐或遮挡变化也可能改变 hover；每轮布局和 transform 更新完成后，使用最新光标位置重新命中并比较新旧路径。hover 引起布局变化时交给下一轮重新检查，不在一次输入处理中递归求稳定点。首版只维护一个鼠标光标的一条 hover 路径；多个鼠标按键共享该位置。提供只读查询 `NodeRef::hovered() -> Option<ReadSignal<bool>>`：已启用且存活的 Node 返回同一状态句柄，未启用或已销毁返回 `None`；查询不分配信号、不启用跟踪，也不延长 Node 生命周期。多指针聚合留作后续扩展。

**实现待验证项**：builder 提前创建的信号及回调派生计算如何归属 scope；未物化就丢弃 builder、回调 panic 或构建失败时如何释放；mount 后如何移交并随 Node 生命周期保活与清理。多个共根 builder 已各自交出不同 ReadSignal 时，仍须统一或转接，保持旧句柄和依赖有效；单一 NodeInner 字段不自动解决该问题。具体句柄统一与 scope 移交机制仍待实现验证，尚未确认 hover group 或 batch 广播等具体方案。

### 4.6 性能

如果所有 `#[node]` / `#[component]` 都成为主题目标：

- 每个 target 都可能需要 identity、options snapshot 和 class set；
- 当前 `Style::themed_defaults` 会为每个样式字段各建一个 `computed_always`（约 66 个/节点：50 个属性 + margin/padding/border/inset 各 4 边；目前只作用于 factory 实例），这部分成本与带主题的节点数相乘，可能先于规则匹配成为瓶颈；
- 有主题时，每个节点都会执行主题规则匹配；
- `*` 规则会让所有节点都付出成本；
- options / class / hover 的响应式变化都会触发主题重算；class 集合是单个响应式值，任一 class 变化都会让读取该集合的节点重算（per-class 细分以后可作为优化）。

#### 4.6.1 `computed` 与 `computed_always`

两者都是有缓存的响应式计算：创建时先计算一次，依赖变化后重新计算；它们都不是每帧无条件执行。

- `computed<T: PartialEq>` 会执行自己的计算函数，但只有新结果与上一次结果不相等时才通知下游。它减少的是下游级联重算，不保证跳过当前 computed 本身的计算。
- `computed_always<T>` 不要求 `PartialEq`，每次依赖变化都保存结果并通知下游，即使新旧值相等。

因此，主题总 resolver 可以暂时使用 `computed_always`（主题结果通常会重新构造，且部分样式值未必适合比较），但最终可比较的样式属性应尽量使用 `computed`，以避免无意义地触发布局、绘制和 transition 下游。

全部主题样式使用 `computed_always` 有明确的性能风险：

- 每个 themed node 可能创建主题总 resolver 加约 66 个字段级 computed；即使命中结果没有变化，字段也会向下游发送更新；
- hover 或 class 高频切换会重复执行完整主题匹配和字段传播；新的响应式 `HashSet` 还会使读取整个集合的节点因任一 class 变化而重算；
- option 变化但最终命中的 style 没有变化时，仍可能触发布局和绘制 effect；
- “存在 ThemeChain 但没有规则命中”与“没有 ThemeChain”的成本可能明显不同；
- `build_taffy`、transition binding 等后续层还会组合或监听这些动态值，字段级无条件通知会放大成本。

建议：

- 保留“没有 `ThemeChain` 就跳过”的 fast path；
- 主题按 identity 预分组规则，避免每个节点遍历所有规则；
- 主题层保留每个属性的稀疏“未声明”状态，在级联完成前不要用引擎默认值填满所有字段，否则会破坏组件默认值回退和响应式主题规则退出后的恢复；
- 静态 theme、静态 options/class 且没有动态伪类时，优先走一次性解析和 `Prop::Static` 路径；
- 只为实际受主题影响的字段建立响应式绑定，最终尽量只物化一次布局样式；
- 先做基准测试，再决定是否值得让每个 node 都成为 boundary。

### 4.7 Runtime、实例 ID 与生命周期

当前代码存在两套相互独立的 thread-local Runtime：[响应式 Runtime](../mue-core/src/runtime.rs#L15-L57) 管理 signals、effects 和 scopes，[UI Runtime](../mue-macroquad/src/runtime.rs#L22-L71) 管理 nodes、Taffy、gestures、timers 和 fonts。[App](../mue-macroquad/src/app.rs#L16-L40) 只是持有 root `Node` 并驱动 frame，不创建独立 Runtime。vendored macroquad/miniquad 使用单一图形 context/native display；本次以现有 UI Runtime 为 ID 唯一性边界，不把它解释成已经实现的窗口级 Runtime。

**范围限定（已确认）**：本次暂不考虑长期的多窗口、多 root 或 Runtime 实例化，不引入 WindowId、AppId 或 root namespace。ID registry 先放在现有 `mue-macroquad::runtime::Runtime`；ID 唯一性不按 theme scope 隔离。

#### 4.7.1 ID 语义

构造端候选写法是 `.id("save")`，选择器候选写法是 `#save` / `Button#save`。这是实例 ID，不是声明级 ThemeKey 或内部 NodeId。纯 `#save` 不提供 options 的具体类型；带 `$option` 的谓词仍必须依附具名 target。ID 相关开放项统一列在第 7 节。

- builder 阶段只保存 ID metadata；Node 注册/物化时向当前 Runtime claim。
- 同一 UI Runtime 内，所有仍然存活的 Node 的 ID 必须唯一；重复 ID 报错，不静默覆盖。
- ID 绑定 Node 的存活期，而不是当前 child list 的可见期。`show_if(false, node)`、导航退出动画和 keyed children cache 暂时隐藏节点时，节点仍占用 ID。
- Node 真正 dispose 时释放 ID。`NodeRef` 的复制不会延长生命周期，其句柄自身没有 owning Drop；`Node` 内的 `Owned<NodeRef>` 在 Drop 时调用 `NodeRef::dispose`。沿用这条释放链，不另建一套实例生命周期。ID 清理应是 Node dispose 的一部分，不依赖 effect 的 `on_cleanup`，后者还会在 effect 重跑前执行。
- registry 只保存非 owning 的 `NodeId`，不得通过强持有 `Node` 阻止它销毁；theme selector 直接读取当前 target 的 ID metadata，不需要通过 registry 查找才能匹配。
- claim 失败必须回滚本次 Node 构建产生的资源，不能遗留半注册的 Node、scope、children 或 ID 记录。
- registry 至少保存 `Id -> NodeId`。释放时必须校验当前记录仍属于该 `NodeId`；`SlotMap` 的 generational `NodeId` 可防止旧实例误删新实例。
- 第一版 ID 不响应式。动态状态使用 class 或 option；如果未来支持动态 ID，应提供显式、事务性的更新 API，而不是普通 `Prop<String>`。

#### 4.7.2 响应式替换约束

当前 [map_keyed](../mue-macroquad/src/node/children.rs#L99-L115) 在构建新节点后才丢弃旧 cache 中未复用的节点，因此旧 key 换成新 key、但两者声明相同 ID 时会发生真实的存活期重叠。`NodeId` generation 只能防止误删，不能消除 claim 时的冲突。

- 同 key 重排复用原 Node，保持原 ID；删除 key 后必须等 owning Node 真正释放，ID 才可复用。`map_keyed` 并不缓存已经删除的 key。
- 后续实现优先明确 retire/claim 顺序：先确定本轮删除、保留和新增的 key，释放确定删除的旧节点，再构建新增节点；保留实例不受影响。此项是待实现要求，不表示当前 `map_keyed` 已按此顺序运行。
- 如调用方刻意保留旧 Node，或退出动画仍持有旧页面，该 ID 仍被占用；新实例不得自动抢占。可复用页面或列表项须避免硬编码重复 ID。
- 暂不引入新的树提交事务架构，也不以静默覆盖或短暂允许重复作为替代。

### 4.8 迁移与测试

ID 相关实现还应覆盖以下测试：

- 同 Runtime 的重复 ID 拒绝；未物化或被丢弃的 builder 不占用 ID；
- Node dispose 后 ID 可复用，借用 NodeRef 不延长占用；
- `show_if(false)` 和导航退出动画保留实例期间 ID 继续占用；
- keyed children 同 key 重排保持 ID，删除释放 ID；同 ID 替换需按先 retire 后 claim 的顺序；
- 旧 NodeId 的释放不得删除新记录，重复 dispose 不误删；
- claim 失败后的构建资源和 ID registry 回滚；
- ID selector 从 target metadata 匹配；顶层 theme 不创建额外的 ID 命名空间。

- factory 直接删除，不做过渡；旧 `[primary]` option 语法不保留，`.class` 作为 compound 的 class 部分保留；examples/docs/tests 一次性迁移到新语法；
- 当前 `mue-components/tests/theme.rs` 更接近编译冒烟测试，缺少样式结果、祖先匹配、class 命中、layout 生效等断言；
- 删除 factory 时一次性迁移 examples、docs、tests，并补充：
  - `#[node]` / `#[component]` 的 self / option / class / hover 匹配；
  - `Themeable: ThemeTarget + Styleable` 提供 `classes` / `enable_hovered`；`StyleableExt: Styleable` 提供 `with_hovered`，普通未标注节点可用后者，不因此创建主题身份；
  - `enable_hovered()` 只启用跟踪，`with_hovered` 即时执行一次并复用同一上下文；未显式使能时 `:hover` 不匹配，主题不自动安装 tracker；
  - `enable_hovered()` 只启用 hover 跟踪；`with_hovered` 回调在方法调用处立即执行一次，可借用局部值且无额外 `'static` 限制；普通 setter 与回调内 setter 按调用顺序覆盖，默认参数和 options 使用配置完成后的参数；
  - hover 变化更新 text、option、class、动画目标等依赖，不重跑回调或 component；重复请求复用上下文，共根句柄状态一致，旁支 child 独立；
  - 提前创建 hover 上下文后丢弃 builder、回调 panic、构建失败和 Node dispose 的状态/派生计算清理；
  - `#[theme(...)]` 显式 allowlist、裸 `#[theme]` 与空 options；未标注的内容/回调不生成字段；
  - options accessor 跨 crate 可见、字段私有、无 accessor trait；默认生效值匹配、只求值一次、不允许依赖其它参数；
  - `.class` / `.!class` 不访问 options；
  - 显式标注不存在/重复/style 参数、未具体化泛型、`impl Trait`、闭包、函数指针和 `Box<dyn Trait>` 的编译期诊断；
  - `Button.compact.!disabled[$progress < 2]:hover` 的解析、匹配和 specificity；
  - selector 与 `class!("...")` 接受合法 class 名（`my-class` / `card_2` / `A1`），在编译期拒绝数字/`-`/`_` 开头、空串和其它非法字符；宏在普通表达式与 const 中都执行编译期验证；
  - 运行时 ClassName fallible 构造拒绝非法名称，静态/运行时同名值的 Eq/Hash 一致，str 借用查询和 remove 可用；非法名称无法经安全 API 进入动态集合；
  - 多个 `[]` 块、`[]` / `:hover` 错序的编译错误；
  - `$` 表达式里的普通标识符（`H2`、局部变量、常量）不被改写；
  - compound 内 AND、逗号 OR、`.class` / `.!class` 的匹配；
  - `.classes(...)` 支持静态与动态集合，默认空集合；`.!class` 在名称不存在时匹配；
  - 同一 builder 后一次 `.classes(...)` 完整替换前一次输入，不做并集；替换旧动态输入后，旧源不再决定该 target 的 class；
  - 运行期通过外部 Signal 增删 class 名会触发主题重算，原先不存在的名字新增后也能被选择器匹配；显式派生集合支持布尔条件和多个来源，class 变化不重跑 component/node；
  - 节点销毁清理 class 的派生计算和主题订阅，由外部 scope 管理的源信号仍可使用；
  - `[$option]` 没有具名 identity 的错误路径（`[$option]`、`*[$option]`）；
  - `#[node]` 的主题是否真的影响 width、padding、font-size 等布局/绘制属性；
  - 同一顶层 theme 下 component / node target 的级联顺序；
  - prebuilt children 和 tuple children 的祖先/兄弟语义；
  - 顶层 theme 安装早于根视图构建，子树内安装被拒绝；规则模块只在顶层统一组合；
  - 后续动态创建的 children 使用同一个顶层 `Prop<Theme>`，顶层动态换肤覆盖已有和新增 target；
  - 无主题 fast path、动态主题、节点销毁后的响应式清理。

## 5. API 形状示意

本节展示已确认的 API 形状，`Button` 是示意名称；Q15 的 prepare/defaults、resolve/seal、mount 协议见 4.3，具体内部 trait 和 builder 约束仍待实现验证。

```rust
pub trait ThemeTarget: 'static {
    type Options: 'static;

    fn key() -> ThemeKey;

}
```

标注函数生成：

```rust
#[component]
#[theme(primary, progress)]
fn button(caption: Rc<str>, #[default] primary: bool, progress: i32) -> ButtonBuilder {
    // 只有 primary / progress 进入 options；caption 保持普通内容参数。
}

pub struct ButtonBuilder;   // 生成的 builder，直接实现 ThemeTarget
pub struct ButtonOptions;   // options accessor view，见 4.2.2
pub type Button = ButtonBuilder; // 默认生成的 selector 友好别名，不是新类型

impl ThemeTarget for ButtonBuilder {
    type Options = ButtonOptions;

    fn key() -> ThemeKey {
        ThemeKey::of::<Self>() // stable std::any::TypeId
    }

}
```

options view 的字段保持私有，方法公开；主题宏通过 `reference.options::<Button>()`（或 `ButtonBuilder`）拿到 `&ButtonOptions`，再调用 accessor。`Button` 若存在只是 `ButtonBuilder` 的类型别名。

顶层主题入口：

```rust
let active_theme = signal(light_theme());
let app = App::with_theme(active_theme, || root_view());

// 后续只更新主题规则与样式绑定，不重建 root_view。
active_theme.set(dark_theme());
```

`App::with_theme` 的主题参数是 `impl Into<Prop<Theme>>`；不定义独立的 `ThemeSource` 包装。

主题：

```rust
theme! {
    Button[$primary] { background: blue; }
    Button.compact { padding: 8px; }
    Button.compact.selected { padding: 8px 12px; }
    Button[$progress < 2]:hover { opacity: 80%; }
    .selected { opacity: 90%; }
}
```

class 集合入口：

```rust
fn classes(self, classes: impl Into<Prop<HashSet<ClassName>>>) -> Self;
```

静态集合：

```rust
button("Save")
    .primary(true)
    .classes(HashSet::from([class!("compact")]))
```

运行期增删 class：

```rust
let classes = signal(HashSet::from([class!("compact")]));
let view = button("Save").classes(classes);

// view 交给 UI 树持有，之后只修改源信号即可。
classes.update(|set| {
    set.insert(class!("busy"));
});
classes.update(|set| {
    set.remove("compact");
});
```

根据布尔状态派生完整集合：

```rust
let selected = signal(false);
let classes = selected.map(|selected| {
    let mut set = HashSet::from([class!("compact")]);
    if selected {
        set.insert(class!("selected"));
    }
    set
});
let view = button("Save").classes(classes);

selected.set(true); // 集合和主题随之更新，不重建组件。
```

`.classes(first).classes(second)` 只使用 second，不隐式合并。上述 API 尚未实现；静态集合直接传入所需的 Prop 转换应随实现补齐。动态源及调用方派生集合须由生命周期覆盖视图使用期的 scope 管理，信号句柄本身不保活。

factory 删除后需要接住的职责：

- identity：具体 builder 类型 / `ThemeTarget`，泛型 selector 使用 `Badge<u32>`，默认生成 CamelCase alias；
- options：组件参数 + options view；
- defaults：builder 默认值或函数体里的 style 默认值；
- style callback：组件函数本身；
- classes：target 保存 `.classes(...)` 的完整集合输入；
- lazy content：`.create(content)` 的替代，或 lazy children；
- theme resolve：从 `FactoryInstance` 抽成通用逻辑。

## 6. 建议的实施顺序（Q15 方向已接受）

1. **建立构建上下文和目标身份**：抽 `ThemeTarget` / `ThemeKey`，定义内部 `RootBuild` continuation 和顶层 `Prop<Theme>` 安装入口；按新 compound 语法重写 selector parser 的数据模型。
2. **实现 prepare/defaults → resolve/seal → mount**：给 `#[node]` / `#[component]` 接入同一协议；component 沿返回 builder 链递归传递同根贡献，旁支 child 建立独立 RootBuild；迁移 node 默认值准备，保证 transition、transform、layout、paint 和 Node 注册只在 mount 阶段执行一次。
3. **接入 options、class 与规则级联**：通过函数级 `#[theme(...)]` 显式列出 options，生成 accessor view；按方案 B 从生效值 clone 出 options；class 使用 ClassName 与 class! 编译期宏，只通过 `.classes(...)` 接收完整的静态或响应式 `HashSet<ClassName>`，后调用替换；Theme 保存稀疏属性候选、specificity 和 source order；实现显式 style > theme > fallback 的逐属性级联。
4. **实现 children 的准备/物化边界**：按 Q16 已选机制保存待构建 children，在父上下文准备好后物化，各 child 使用独立 RootBuild；动态创建时恢复 owned 顶层构建上下文。已物化的 prebuilt `Node` 不重新构建、不事后补祖先语义。结构组合器仍按 Q24 后置，目标树及祖先/兄弟匹配由 Q17/Q18 后续确定；补性能和边界测试。
5. **删除 factory**：一次性迁移 examples、tests、docs 到新 selector，去掉 `Factory` / `FactoryInstance` / `FactoryUse` 以及 factory stack / sibling context 的依赖。`IntoNode` 仍作为最终物化接口，但 `.children(builder)` 的转换不能继续在父上下文建立前 eager 完成。

每一步都应保持可编译、可测试，避免出现“factory 删了、主题和 class 还没接住”的中间态。组合器实现前，先分别测试 lazy children、prebuilt children 和 reactive children 的上下文语义；性能至少覆盖无主题、有主题但无匹配、静态命中、hover/class 高频变化，以及 option 变化但最终 style 不变的场景。

<a id="open-questions"></a>

## 7. 尚待讨论的问题

本节是所有开放问题的统一入口。Q 编号用于后续逐项讨论；“候选/倾向”仅记录提案，不代表已经接受。已确认方向见第 2 节，现状和实现约束见第 3～4 节。

| 分组 | 问题编号 | 主要内容 |
| --- | --- | --- |
| [身份](#questions-identity) | Q03 | 泛型 selector / builder 实现细节 |
| [options](#questions-options) | Q06；Q09 为未来路线 | options 所有权；accessor 先返回 owned 值，临时借用后置 |
| [级联与样式应用](#questions-cascade) | Q12、Q15 | 属性级联边界、共享根的 target 合并、时序协议实现（Q14 已确认） |
| [children](#questions-children) | Q16～Q18 | Q16 已选准备/物化机制的实现、组合器边界、动态上下文 |
| [class / hover](#questions-class-hover) | Q19～Q20 | ClassName/编译期宏、hover 首版语义已确认；实现验证与后续扩展 |
| [ID](#questions-id) | Q21～Q22 | 语法、错误 API、声明与 Node 的归属 |
| [性能与范围](#questions-performance) | Q23 | 绑定策略、显式 opt-in |

<a id="questions-identity"></a>

### 7.1 声明身份、builder 与别名

**Q03 · 泛型组件的具体 identity**

- 已确认：泛型组件按具体实例化类型区分 identity；selector 使用 Rust 类型路径语法，例如 `Badge<u32>`。不同具体类型不共享 identity。
- 实现待验证：宏如何生成带泛型的 builder alias/`ThemeTarget`，selector parser 如何接受泛型参数，以及所需的 `'static` / `TypeId` 边界。

<a id="questions-options"></a>

### 7.2 options、参数声明与访问语义

**Q06 · 被标注 options 的长期所有权（未决）**

- `Box<dyn T>` 暂不允许。被标注参数必须能通过现有 `Access` 读取；当前 `get` 要求 `Copy`，`get_clone` 要求 `Clone`，并受 `'static` 约束。
- 方案 B 需要 options 与组件函数共同持有参数；`Prop::Dynamic` 只需共享 signal 句柄，但 `Prop::Static(T)` 的非 `Clone` 所有权如何处理仍未决。候选是继续要求 `Clone`、引入共享包装，或调整参数传递/持有模型。Q09 的临时借用不能单独解决长期所有权问题。

**Q09 · accessor 返回 owned 值，临时借用留作未来路线（已确认）**

- 本次重构沿用 Copy 值读取和非 Copy 类型的 owned Clone 返回值；accessor 内部执行 tracked read。临时借用不是首版接口，也不再作为实施前置问题。Q06 的共同持有约束仍独立处理。
- 未来可以研究为 `Prop<T>` / 动态 signal 提供 tracked read 的临时借用，例如 `with(|value: &T| ...)`；借用不得逃逸 resolve。它只能减少读取时复制，不能消除方案 B 构造 options 时的 clone/所有权成本。
- 若将来采用借用，需先解决当前 signals 共用 runtime 的 `RefCell<SlotMap<...>>` 的约束，不能让全局 runtime borrow 包住用户回调；还需验证多 option、重复读取、computed、嵌套读取、重入、更新/销毁及 panic 清理，并明确借用期间的 mutation 策略。

<a id="questions-cascade"></a>

### 7.3 级联、specificity 与样式应用

**Q12 · CSS 级联如何映射到组件系统（优先级与同层顺序已确认，具体实现仍待完成）**

- 已确认的属性级优先级为：调用方显式 style 最高，顶层 theme 命中声明居中，fallback 默认值最低；每个属性独立合并，未声明属性继续向低优先级回退。fallback 包括组件/node 的普通默认值和框架最终默认值。
- 调用方 builder 上的 `.style(...)` / 单属性 setter，以及 `#[component]` 函数体内部对其返回 builder 的 `.style(...)` / style setter，均属于同一最高优先级显式层；组件实现中不推荐手动设置样式。
- `#[component]` 返回另一个 builder 时，Q15 的 RootBuild 沿返回链传递同根贡献；component 外层的显式 style 仍属于最高层，并覆盖返回 builder 的显式 style。
- 同一 builder 对同一属性允许后写覆盖先写。多个 target 共用一个真实 Node 并命中同一属性时，权重一致按源码先后顺序决定胜负。
- 同一条规则命中多个共根 target 时先去重，只形成一个候选；该候选按规则自身的 specificity 和源码顺序参与级联，不按命中 target 的数量增加权重，也不引入 target 顺序作为额外排序键。

**Q14 · selector 列表与并列规则（已确认；provider scope 问题已移除）**

- 已确认：逗号列表指 `selector1, selector2`；每个 selector 独立计算 specificity；相同权重按代码顺序，靠后的规则优先。`[]` 中每个 `$` 引用增加一个 CLASS 权重，其余权重遵循 CSS。
- ThemeProvider scope 不属于本设计：只有一条应用顶层 theme 规则链；同一 target 内的主题规则按 selector specificity 比较，同权重按代码顺序决定胜负。多个 target 共用根的合并仍见 Q12。
- `!important` 等机制暂不实现；CSS 的 origin/layer 等机制暂不纳入首版。

**Q15 · node / component 样式应用时序（总体方向与级联边界已确认，实施协议仍待完成）**

- component 沿返回 builder 链传递同根 `RootBuild`；返回另一个 component builder 时递归执行同一协议。函数体内的旁支 child 建立独立 RootBuild，不消费父 component 的同根贡献。
- 终端 node 使用 `prepare/defaults → resolve/seal → mount`：prepare 完成生效参数、options/class、资源和默认样式；resolve/seal 保留稀疏属性候选并完成同根 target 的统一级联；mount 在 seal 后只绑定一次 transition、transform、layout、paint 和 hooks，并只物化一次 Node。
- `with_hovered` 属于 builder 配置阶段：回调在方法调用处立即执行，不存到 prepare；普通 setter 保持即时写入和后写覆盖。回调先配置 hover 相关参数，再由 prepare 求默认参数、构造 options 并执行对应 component 函数体，详见 4.5.2。
- 已确认：node 声明显式拆分 prepare 与 mount，允许修改声明 API 和迁移现有函数体，不要求兼容旧的混合函数体。默认值和资源准备放在 prepare，显式交出的 mount 在 seal 后读取完成级联的 style；宏不猜测任意函数体中的阶段，不重跑函数体。`NodePlan(defaults, mount)` 仅为候选接口示意，具体类型/trait、语法和资源移交方式待设计。
- `RootBuild` 是返回链的 continuation，不放进任意 `into_node()` 都可消费的临时 TLS。动态 children 必须保存并恢复 owned 构建上下文；scope 必须覆盖 prepare 到 mount，并在失败时清理。
- Theme 必须保留逐属性候选、specificity 和 source order，不能先把每个 target resolve 成普通 `Style` 再合并；现有 `Fn(ComponentRef, Style) -> Style` 形式需要替换或扩展。
- 顶层入口采用 `App::with_theme(impl Into<Prop<Theme>>, build)`，不保留 `ThemeSource`。`Prop::Dynamic` 的主题切换只更新已有 resolver 和样式绑定，不重新执行 root/component/node 构建。
- 级联边界已确定：component 外层显式 style 覆盖返回 builder 的显式 style；同一 builder 后写覆盖先写；共根 target 权重一致按源码先后顺序；同一条规则命中多个共根 target 时按规则自身的 specificity/source order 处理，不按命中次数加权。
- Q15 之所以仍保留“待完成”状态，不是因为三阶段架构或级联政策尚未选择，而是因为当前代码还没有对应的实现协议：显式 prepare/mount 的 node 声明接口及宏接入尚需落实；`RootBuild` 需要跨返回 builder 链传递、由 scope 保活并在失败时清理；动态 children 需要恢复 owned 构建上下文；Theme 需要从 `Fn(ComponentRef, Style) -> Style` 扩展为保留稀疏属性候选、specificity 和 source order 的规则表示。

<a id="questions-children"></a>

### 7.4 children 与组合器

**Q16 · children 的准备/物化协议（设计已收口，并入 Q15 实现）**

- `.children(...)` 保存待构建的 children 描述，不执行 child component/node 函数体、不 eager 物化 builder；调用方无需额外包 lazy closure。
- 父返回链 prepare → seal → 建立父 mount 上下文后，各 child 用独立 RootBuild 执行自己的 prepare → seal → mount，父 layout 等消费者随后接入 children。child 继承顶层 Prop<Theme> 等可共享构建上下文，不消费父返回链的 target/class/显式样式贡献；不要求整棵树统一 prepare 后才 mount。
- map_keyed 等动态列表先保存输入和创建函数，父上下文就绪后启动计算；后续新增 key 恢复保存的 owned 构建上下文，保留 key 复用原 Node。保存主题输入而非主题快照，动态换肤覆盖已有和新增 child。
- prebuilt Node 直接接入，不重新物化、不补回缺失的祖先/兄弟匹配上下文。混合 tuple 中 builder 按协议构建，Node 直接使用。首版仍不支持结构组合器，Q17/Q18 的结构匹配语义继续后置。
- 待构建内容由 builder/children 计划持有，未执行丢弃时释放已持有资源；物化后由 children 容器持有 child，相关 scope 随节点清理，构建失败回收本次资源。具体 prepared 类型、trait、tuple/Vec/Rc 容器接入和清理 guard 随 Q15 实现验证，不再等待独立架构决策。

**Q17 · 组合器匹配哪一种树**

- `>` / descendant 是按显式 opt-in 的 ThemeTarget 结构，还是所有 node/target 的结构？`Button > Text` 中插入 `Flexbox` 会不会改变匹配？此处的 target 边界不表示 provider scope。
- `+` / `~` 只支持有序 tuple，还是也支持 reactive list？node target 首版是否支持组合器？

**Q18 · 动态与祖先上下文的存储**

- reactive children 后续新增、删除、重排如何恢复 context 和 sibling 顺序，而不只在首次构建时 push/pop？
- 若支持 `Button[$primary] > Text`，如何保存祖先 options、class、hover 等可响应的数据？当前 ancestor id 列表不足以匹配这些条件。

<a id="questions-class-hover"></a>

### 7.5 class 与 hover

**Q19 · ClassName、编译期宏与运行时构造（已确认，待实现）**

- 集合输入改为 `.classes(impl Into<Prop<HashSet<ClassName>>>)`；继续完整替换、默认空集合、后调用替换先调用及其绑定，不隐式追加或取并集；不恢复单 class 布尔绑定入口。
- ClassName 包装单个名称，字段私有，安全 API 不允许绕过格式校验；不额外包装整个集合。格式为 `[A-Za-z][A-Za-z0-9_-]*`。
- `class!("...")` 构造字面量名称，保证编译期验证，可用于 const；运行时字符串通过 fallible 构造（例如 TryFrom<String>）返回 Result。
- 验证在名称构造边界完成；静态/动态集合都只存 ClassName，不在 setter 或 theme resolve 中反复验证、panic 或恢复旧集合。静态和运行时名称按内容做 Eq/Hash，支持 str 借用查询。
- 暂不讨论 class 声明/注册及引用合法性；格式检查不要求名称预先声明。Cow<'static, str> 等内部存储及宏实现方式随实现选择，不影响上述契约。

**Q20 · hover 配置接口、安装与指针语义（首版语义已确认，剩余为实现验证与后续扩展）**

- 已确认：`enable_hovered(self) -> Self` 只建立或复用 hover 上下文并请求该真实根的 hover 跟踪，不向调用方传递状态。
- 已确认：`with_hovered(self, impl FnOnce(Self, ReadSignal<bool>) -> Self) -> Self` 建立或复用同一上下文并立即执行一次回调；不要求回调为 `'static`，不引入延迟配置队列；详见 4.5.2。调用 `with_hovered` 本身也会使能跟踪。
- 已确认：两个方法幂等；主题中的 `:hover` 不自动安装 tracker。未调用任一方法的节点，即使主题规则包含 `:hover`，也不通过 hover 状态匹配；动态主题后来新增 `:hover` 规则也不反向启用跟踪。
- 已确认：`classes` / `enable_hovered` 放入 `Themeable: ThemeTarget + Styleable`；`with_hovered` 放入 `StyleableExt: Styleable`。`enable_hovered` 为主题 `:hover` 提供状态消费入口，`with_hovered` 则把信号交给即时配置回调；未标注 `#[theme]` 的节点通过后者使用普通响应式 hover，不获得 ThemeTarget/Themeable。这取代此前两个 hover 方法都对普通节点开放的约定。底层 hover 机制仍不依赖主题解析，也不创建新的 theme scope。
- 已确认：运行期状态保存在 `NodeInner.hovered: Option<Signal<bool>>`；None 表示未启用且不分配信号，Some 为该真实 Node 的统一状态。指针系统写入，主题与配置代码只读；同根 target 共用，旁支 child 独立。提前状态经 RootBuild 移交，mount 不另建无关信号；资源由 Node 持有的 scope 管理，不靠句柄保活。
- 实现待验证：builder 阶段信号及派生计算的 scope 归属、丢弃/panic/构建失败清理，以及多个共根 builder 已交出不同句柄时的统一/转接。NodeInner 字段已定，不代表句柄统一机制已经选定。
- 已确认命中/遮挡：hover 与普通指针输入共用有效命中、绘制顺序和遮挡规则；每次只选最上层目标，hover 路径包括该目标及真实 Node 祖先，被遮挡的重叠兄弟不同时 hover。hover 启用只决定是否维护该路径节点的状态，不改变命中排序/遮挡；未启用 hover 的覆盖层仍遮挡，穿透需独立显式命中策略，透明度不隐含穿透。实现需产出命中路径，不能直接使用当前所有相交节点集合。
- 已确认输入行为：鼠标按下/拖动期间仍按当前位置更新 hover，与拖动捕获/事件接收方分别维护，不锁定起始节点；触摸及触摸模拟鼠标不产生 hover。鼠标离窗或窗口失焦时清除，恢复后以重新确认的位置重算。
- 已确认：界面变化后在布局/transform 更新完成时重新命中；首版只维护单个鼠标光标的一条路径；`NodeRef::hovered() -> Option<ReadSignal<bool>>` 提供物化后的只读查询，不分配信号、不启用跟踪、不延长 Node 生命周期。
- 后续扩展：多指针聚合。实现验证仍包括提前状态的 scope 归属、移交/清理，以及多个共根 builder 不同句柄的统一/转接。

<a id="questions-id"></a>

### 7.6 实例 ID 的剩余接口问题

**Q21 · ID 的公开语法与名字格式**

- `.id("save")`、`#save`、`Button#save` 是当前候选；ID 字符格式、单个 compound 的 ID 数量、与 identity/class/`[]`/hover 的排列如何规定？
- 无具名 identity 的 `#save` 只提供实例匹配，不能提供 `$option` 的具体类型；如何做相应诊断？

**Q22 · claim 的接口、归属与失败处理**

- Node 物化返回 Node 的现有接口下，重复 ID 以 Result、统一构建错误或 panic/诊断呈现？错误处理不能导致资源泄漏。
- 声明级 component 和内层 node 共用根时，ID 属于哪个 target，是否透传到根 metadata？Node 注册晚于 target frame 建立，claim 与 metadata 的准备过程如何衔接？
- non-owning `Id -> NodeId`、条件释放、失败回滚、先释放确定删除节点再创建新增节点均是已记录要求；具体实现/测试如何接入现有 dispose 与 keyed list？

<a id="questions-performance"></a>

### 7.7 性能与首版功能边界

**Q23 · 响应式绑定与性能方案**

- 无主题跳过、静态一次性解析、按 identity 预分组、仅绑定受影响字段等建议如何通过基准选定实施顺序？
- resolver 与字段级 `computed` / `computed_always` 如何组合，避免最终 style 不变仍反复通知 layout/paint/transition？
- Copy clone 通常无需特化；per-class 跟踪、interned class ID 等优化是否有实际收益，留给测量决定。

**Q24 · 首版选择器范围（已确认）**

- 首版支持单个 compound selector 和逗号分隔的 selector list：identity/universal、`.class` / `.!class`、option 谓词和 `:hover`。ID 是否同时进入首版仍由 Q21/Q22 决定。
- 首版不支持 `>`、descendant、`+`、`~` 结构组合器；Q16～Q18 稳定 deferred children、目标树和动态祖先/兄弟上下文后，再单独加入组合器。

### 7.8 已确认、无需重复选择的事项

- factory 直接删除，不做兼容层；主题采用显式 `#[theme]` opt-in；未标注的 node/component 不生成 ThemeTarget/options；options 使用函数级 `#[theme(...)]` 显式 allowlist；被标注参数保留编译期检查；所有权方案 B 不变。
- theme 只允许在应用顶层提供，不允许嵌套/局部 provider；规则可以分模块定义并由顶层统一组合，动态主题由同一个顶层 `Prop<Theme>` 提供。target 间级联和结构组合器范围不由此自动确定。
- class 名采用 ClassName 包装，字面量通过 class! 编译期校验，运行时构造返回 Result，暂不检查预先声明；只通过 `.classes(...)` 接收完整的静态或响应式 HashSet<ClassName>，默认空集合；同一 builder 后调用替换前调用，不隐式追加或取并集，也不提供单 class 布尔绑定。多个来源由调用方显式合成；匹配只检查名称是否存在，修改源 Signal 无需持有 Node 或 Component。
- 主题必须在相关 style 被消费前接入；Q15 采用 `prepare/defaults → resolve/seal → mount`，同根贡献沿 component 返回 builder 链传递，旁支 child 使用独立 RootBuild。
- Q16 已选择 children 的“准备 → 父上下文内物化”机制，setter 不提前物化 child builder；具体类型/trait 与动态上下文接入待实现。Q09 的 accessor 先返回 Copy / owned Clone 值，临时借用只留作未来可能路线。
- `Themeable: ThemeTarget + Styleable` 提供 `classes` / `enable_hovered`，只用于主题目标；`StyleableExt: Styleable` 提供 `with_hovered`，普通节点通过回调消费信号。两种 hover 入口共享上下文并显式使能跟踪，回调即时执行、不延迟到 prepare、无额外 `'static` 限制；链式 setter 按调用顺序覆盖，`:hover` 不自动安装 tracker。
- ID 在当前 UI Runtime 的所有存活实例间唯一；不按 theme scope 分隔；builder 不 claim、物化注册、Node dispose 释放；隐藏/缓存/退出动画仍占用；首版 ID 不响应式，不静默覆盖。
- 释放校验 generational NodeId，registry 不 owning Node，claim 失败回滚；keyed replacement 优先先释放确定删除的旧实例再创建新增实例，不引入新树提交事务架构。
- 首版选择器范围已确认：支持 compound 与逗号 selector list（identity/universal、class、否定 class、option、hover）；ID 是否纳入首版由 Q21/Q22 决定；`>`、descendant、`+`、`~` 组合器后置到 Q16～Q18。
- 多窗口、多 root、WindowId/AppId/root namespace 与 Runtime 实例化不属于本次范围。


## 8. 参考

- 当前主题与 selector 文档：[themes.md](themes.md)
- 当前 factory 文档：[factories.md](factories.md)
- 主题解析：[mue-macroquad/src/theme.rs](../mue-macroquad/src/theme.rs)
- factory 实现：[mue-macroquad/src/factory.rs](../mue-macroquad/src/factory.rs)
- 宏生成逻辑：[mue-macros/src/component.rs](../mue-macros/src/component.rs)
- 主题宏：[mue-macros/src/theme.rs](../mue-macros/src/theme.rs)
- children 转换：[mue-macroquad/src/node/children.rs](../mue-macroquad/src/node/children.rs)
- 布局消费 style：[mue-macroquad/src/layout.rs](../mue-macroquad/src/layout.rs)
