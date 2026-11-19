use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::quote;

fn dependency(name: &str) -> Option<TokenStream> {
    match crate_name(name).ok()? {
        FoundCrate::Itself => Some(quote!(crate)),
        FoundCrate::Name(name) => {
            let ident = syn::Ident::new(&name, Span::call_site());
            Some(quote!(::#ident))
        }
    }
}

pub fn host() -> syn::Result<TokenStream> {
    dependency("mue-macroquad")
        .or_else(|| {
            dependency("mue-components").map(|components| quote!(#components::__private::host))
        })
        .or_else(|| dependency("mue").map(|mue| quote!(#mue::renderer)))
        .ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "add mue, mue-macroquad, or mue-components as a dependency",
            )
        })
}

/// The path to the `mue-components` crate, for the token value form.
pub fn components() -> syn::Result<TokenStream> {
    dependency("mue-components")
        .or_else(|| dependency("mue").map(|mue| quote!(#mue::components)))
        .ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "add mue or mue-components to use token!(key), or pass an explicit theme with token!(theme, key)",
            )
        })
}
