mod component;
mod theme;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn node(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, true)
}

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(item, false)
}

#[proc_macro]
pub fn theme(input: TokenStream) -> TokenStream {
    theme::expand(input)
}
