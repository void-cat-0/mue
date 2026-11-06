mod component;

use proc_macro::TokenStream;

/// Generate a styleable builder implementing the host's `node::BuildNode`.
///
/// Declare a node as `fn foo(ctx: NodeContext, args...) -> Node` and return
/// `ctx.finish(defaults, move |style| { ... })`. The macro injects the first
/// parameter positionally; any simple identifier may name it, optionally `mut`.
/// Its declared type is preserved so Rust checks it is `NodeContext`, including
/// qualified paths and type aliases. It is excluded from builder parameters and
/// setters, and cannot use `#[default]` or `#[model]`. Defaults are evaluated
/// before injection, so they cannot refer to the actual first-parameter name.
/// Non-first ordinary parameters named `build` or `style` remain reserved for now,
/// but the injected parameter may use either name (or `cx`, `ctx`, or another identifier).
///
/// The generated builder pushes its style and calls the original function with
/// the `NodeContext` owner. The author explicitly finishes the node; defaults,
/// runtime bindings, and children are ready before the `finish` callback runs.
/// Its methods are `scope()`, `push_style()`, and `finish()`.
///
/// Ordinary parameters accept `Into<Prop<T>>` inputs; `#[model]` parameters
/// accept writable signals. Use `#[default(value)]` for an optional parameter,
/// or `#[default]` to use its type's default value. Defaults are evaluated only
/// for unset parameters, inside the build scope. The macro adds no `Clone` or
/// `'static` bounds; inputs must satisfy the host's finish callback requirements.
/// Generated builders implement the host's `BuildNode`; the host's blanket impl
/// provides `IntoNode` for them.
#[proc_macro_attribute]
pub fn node(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attr, item, true)
}

/// Generate a component builder with the same ordinary parameters,
/// `#[default]`, and `#[model]` annotations as [`node`].
///
/// Components cannot declare the reserved `build` or `style` parameters. Their
/// original function runs once inside `NodeContext::scope().run`: after evaluating
/// unset defaults and pushing the component builder's style, the generated
/// implementation passes its returned builder to `BuildNode::build(result, build)`.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attr, item, false)
}

mod paths;
mod style;

/// style declarations with typed Rust expressions in braces.
#[proc_macro]
pub fn style(input: TokenStream) -> TokenStream {
    match syn::parse::<style::Declarations>(input).and_then(|style| style.expand()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
