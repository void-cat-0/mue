//! Authoring macros for the Macroquad renderer.
//!
//! Use [`component`] to compose styleable builders, [`node`] to declare a
//! terminal node, and [`style!`](macro@style) to construct typed styles.
//! The generated code resolves the renderer through the consuming crate's
//! `mue-macroquad` or `mue-components` dependency.

mod component;

use proc_macro::TokenStream;

/// Generate a styleable builder for a terminal node declaration.
///
/// The declaration takes the renderer's `NodeContext` as its first parameter
/// and returns a `Node` through `ctx.finish(defaults, body)`. The renderer
/// merges defaults, installs parent bindings, and materializes child plans
/// before calling `body` to initialize layout, paint, or input. A declaration
/// that returns another builder belongs under [`component`].
///
/// The macro injects the first parameter positionally; it may use any simple
/// identifier, including `mut`. The annotated type is preserved and checked by
/// Rust, so qualified paths and type aliases work. The context is excluded from
/// constructor inputs and setters and cannot use `#[default]` or `#[model]`.
/// Defaults are evaluated before context injection, so they cannot name the
/// context parameter. Non-first parameters named `build` or `style` are reserved.
///
/// Ordinary and model parameters follow the same builder rules as [`component`].
/// The generated builder implements the renderer's `BuildNode` trait and gains
/// `IntoNode` through its blanket implementation.
#[proc_macro_attribute]
pub fn node(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attr, item, true)
}

/// Generate a styleable component builder from a function returning a builder.
///
/// The body runs synchronously once when its generated builder is consumed. It
/// executes in the node's construction scope, after unset defaults are evaluated
/// and the builder's explicit style has been pushed. Return the builder itself
/// to carry the caller's styles and owner into the final node. Use [`node`]
/// for a terminal declaration that constructs a real node.
///
/// Required parameters become constructor arguments. All parameters have
/// same-named setters. Mark an input `#[default]` or `#[default(expression)]`
/// to omit it from the constructor. If unset at build time, it uses the type's
/// `Default` or the supplied expression. Defaults cannot refer to any declaration
/// parameter.
///
/// A declaration input of type `T` accepts anything convertible to `Prop<T>`.
/// The body receives that read-only property. With `#[model]`, the builder takes
/// a writable `Signal<T>` and a model default initializes a fresh signal. The
/// macro adds no `Clone` or `'static` bounds. The names `build` and `style` are
/// reserved.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attr, item, false)
}

mod paths;
mod style;

/// Build a renderer `Style` from typed declarations.
///
/// Use kebab-case property names and separate `name: value` declarations with
/// semicolons. The final semicolon is optional. Static lengths use `px`, `%`,
/// `vw`, or `vh` where supported. `font-size` also accepts `em` and `rem`.
/// Use unitless `0` for zero length. Box shorthands such as `padding` and
/// `border-width` use top, right, bottom, left order. In two-value `gap`, the
/// first value sets the row gap and the second sets the column gap.
///
/// Put one Rust expression in braces, for example `width: {width_signal};`.
/// Each expression runs when the style is constructed. The setter can accept
/// a signal or property directly. Use fluent setters for properties outside
/// the declaration grammar, such as shadows.
///
/// ```text
/// style! {
///     width: 100%;
///     padding: 16px 24px;
///     gap: 8px 12px;
///     background: #eef4ff;
///     font-size: 1.2rem;
///     transform: translateY(8px) rotate(-2deg);
///     transform-origin: center top;
///     transition: 240ms ease-in-out 0.65;
/// }
/// ```
///
/// Transform lists also support 3D functions such as `translate3d`, `rotateX`,
/// and `matrix3d`. The renderer projects them onto the 2D canvas. A transition
/// accepts a duration in `ms` or `s` and an optional curve. A strength from `0`
/// to `1` blends an ease curve from linear to full strength, or controls the
/// bounce of a spring, as in `transition: 480ms spring 0.35;`.
///
/// # Optional tokens
///
/// On supported properties, `token!(key)` reads the nearest installed
/// `mue_components::Theme` and `token!(theme, key)` reads an explicit handle.
/// Use an identifier or a string literal such as `"tile.accent"` for the key.
/// Unqualified `token!(...)` calls inside braced expressions also use the
/// style property's value type, so `color: {selected.pick(token!(active),
/// token!(muted))};` works. For a lookup of another type, use
/// `theme.get::<T>(key)` or look up the property before invoking the macro.
/// `transform` and `transform-origin` take ordinary expressions without
/// rewriting token calls.
#[proc_macro]
pub fn style(input: TokenStream) -> TokenStream {
    match syn::parse::<style::Declarations>(input).and_then(|style| style.expand()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
