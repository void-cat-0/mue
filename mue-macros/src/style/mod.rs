//! style declarations lowered to the renderer's typed, reactive style properties.
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{
    Ident, LitFloat, LitInt, LitStr, Result, Token, braced,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
    visit_mut::VisitMut,
};

mod box_model;
mod color;
mod layout;
mod motion;
mod paint;
mod text;
mod token;
mod values;

use color::*;
use token::*;
use values::*;

pub struct Declarations(Vec<TokenStream>);

impl Parse for Declarations {
    fn parse(input: ParseStream) -> Result<Self> {
        let host = crate::paths::host()?;
        parse_declarations(input, &host)
    }
}

impl Declarations {
    pub fn expand(self) -> Result<TokenStream> {
        let host = crate::paths::host()?;
        let style = Ident::new("__style", Span::mixed_site());
        let storage = Ident::new("__style_storage", Span::mixed_site());
        let statements = self.0;
        Ok(quote!({
            use #host::style::{Styleable as _, StyleableExt as _};
            let mut #storage = #host::style::Style::new();
            {
                let mut #style = &mut #storage;
                #(#style = #statements;)*
            }
            #storage
        }))
    }
}

pub(super) fn property(input: ParseStream) -> Result<(String, Span)> {
    let first = Ident::parse_any(input)?;
    let mut name = first.to_string().replace('_', "-");
    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        name.push('-');
        name.push_str(&Ident::parse_any(input)?.to_string());
    }
    Ok((name, first.span()))
}

pub(super) fn expression(input: ParseStream) -> Result<Option<TokenStream>> {
    if !input.peek(syn::token::Brace) {
        return Ok(None);
    }
    let content;
    braced!(content in input);
    let value: syn::Expr = content.parse()?;
    if !content.is_empty() {
        return Err(content.error("expected one Rust expression"));
    }
    Ok(Some(quote!(#value)))
}

pub(super) fn setter(method: &str, value: TokenStream) -> TokenStream {
    let style = Ident::new("__style", Span::mixed_site());
    let method = format_ident!("{method}");
    quote!(#style.#method(#value))
}

pub(super) fn end(input: ParseStream) -> bool {
    input.is_empty() || input.peek(Token![;])
}

pub(super) fn parse_declarations(input: ParseStream, host: &TokenStream) -> Result<Declarations> {
    let mut result = Vec::new();
    while !input.is_empty() {
        let (name, span) = property(input)?;
        input.parse::<Token![:]>()?;
        let method = name.replace('-', "_");
        let statement = declaration(&name, span, &method, input, host)?
            .ok_or_else(|| syn::Error::new(span, format!("unsupported style property `{name}`")))?;
        result.push(statement);
        if !input.is_empty() {
            input.parse::<Token![;]>()?;
        }
    }
    Ok(Declarations(result))
}

/// Try each property group in turn; the first group that knows the name parses it.
fn declaration(
    name: &str,
    span: Span,
    method: &str,
    input: ParseStream,
    host: &TokenStream,
) -> Result<Option<TokenStream>> {
    for group in [
        box_model::declaration,
        layout::declaration,
        paint::declaration,
        text::declaration,
        motion::declaration,
    ] {
        if let Some(statement) = group(name, span, method, input, host)? {
            return Ok(Some(statement));
        }
    }
    Ok(None)
}
