mod component;
mod theme;

use proc_macro::TokenStream;

/// Generate a styleable node builder. Mark scalar length parameters with
/// `#[length]` to accept `IntoLengthValue` inputs; other parameters retain their
/// normal `Into<Prop<T>>` input bound. Length parameters must be `f32` and cannot
/// also be `#[model]` parameters.
#[proc_macro_attribute]
pub fn node(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, true)
}

/// Generate a component builder, with the same `#[length]`, `#[default]`, and
/// `#[model]` parameter annotations as [`node`].
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, false)
}

#[proc_macro]
pub fn theme(input: TokenStream) -> TokenStream {
    theme::expand(input)
}
