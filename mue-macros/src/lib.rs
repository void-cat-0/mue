mod component;
mod theme;

use proc_macro::TokenStream;

/// Generate a styleable node builder. Parameters accept `Into<Prop<T>>` inputs;
/// `#[model]` parameters accept writable signals. Use `#[default(value)]` for
/// an optional parameter, or `#[default]` to use its type's default value.
#[proc_macro_attribute]
pub fn node(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, true)
}

/// Generate a component builder with the same `#[default]` and `#[model]`
/// parameter annotations as [`node`].
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, false)
}

#[proc_macro]
pub fn theme(input: TokenStream) -> TokenStream {
    theme::expand(input)
}
