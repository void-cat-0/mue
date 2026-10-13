use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{
    Expr, Ident, Token, braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, token,
};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ThemeInput);
    expand_theme(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

struct ThemeArm {
    target: Option<proc_macro2::TokenStream>,
    binding: Option<Ident>,
    guard: Option<Expr>,
    body: Expr,
}

struct ThemeInput {
    component: Ident,
    style: Ident,
    arms: Vec<ThemeArm>,
}

impl Parse for ThemeInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![|]>()?;
        let component: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let style: Ident = input.parse()?;
        input.parse::<Token![|]>()?;
        input.parse::<Token![match]>()?;
        let subject: Ident = input.parse()?;
        if subject != component {
            return Err(syn::Error::new(
                subject.span(),
                "theme match must use the closure's component parameter",
            ));
        }
        let content;
        braced!(content in input);
        let mut arms = Vec::new();
        while !content.is_empty() {
            let target = if content.peek(Token![_]) {
                content.parse::<Token![_]>()?;
                None
            } else {
                let path: syn::Path = content.parse()?;
                if path
                    .segments
                    .iter()
                    .any(|s| !matches!(s.arguments, syn::PathArguments::None))
                {
                    return Err(syn::Error::new_spanned(
                        path,
                        "factory selectors cannot have generic arguments",
                    ));
                }
                let mut tokens = quote! { #path };
                while content.peek(Token![.]) {
                    content.parse::<Token![.]>()?;
                    let field: Ident = content.parse()?;
                    tokens = quote! { #tokens . #field };
                }
                Some(tokens)
            };
            let binding = if content.peek(token::Paren) {
                let args;
                parenthesized!(args in content);
                let name: Ident = args.parse().map_err(|_| {
                    syn::Error::new(
                        args.span(),
                        "factory parameter binding must be one identifier",
                    )
                })?;
                if !args.is_empty() {
                    return Err(syn::Error::new(
                        args.span(),
                        "factory parameter binding must be one identifier",
                    ));
                }
                Some(name)
            } else {
                None
            };
            if target.is_none() && binding.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "fallback cannot bind options",
                ));
            }
            let guard = if content.peek(Token![if]) {
                content.parse::<Token![if]>()?;
                Some(content.parse()?)
            } else {
                None
            };
            if target.is_none() && guard.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "fallback cannot have a guard",
                ));
            }
            content.parse::<Token![=>]>()?;
            let body: Expr = content.parse()?;
            arms.push(ThemeArm {
                target,
                binding,
                guard,
                body,
            });
            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }
        if arms.last().is_none_or(|arm| arm.target.is_some())
            || arms[..arms.len().saturating_sub(1)]
                .iter()
                .any(|arm| arm.target.is_none())
        {
            return Err(syn::Error::new(
                content.span(),
                "theme requires exactly one final `_ => ...` fallback",
            ));
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after theme match"));
        }
        Ok(Self {
            component,
            style,
            arms,
        })
    }
}

fn expand_theme(input: ThemeInput) -> syn::Result<proc_macro2::TokenStream> {
    let macroquad = match crate_name("mue-macroquad")
        .map_err(|error| syn::Error::new(proc_macro2::Span::call_site(), error))?
    {
        FoundCrate::Itself => quote! { crate },
        FoundCrate::Name(name) => {
            let name = Ident::new(&name, proc_macro2::Span::call_site());
            quote! { ::#name }
        }
    };
    let c = input.component;
    let s = input.style;
    let mut clones = Vec::new();
    let mut branches = Vec::new();
    let last = input.arms.len() - 1;
    for (index, arm) in input.arms.into_iter().enumerate() {
        let body = arm.body;
        if index == last {
            branches.push(quote! { #body });
            break;
        }
        let target = arm.target.expect("checked by parser");
        let handle = Ident::new(
            &format!("__mue_theme_factory_{index}"),
            proc_macro2::Span::mixed_site(),
        );
        clones.push(quote! { let #handle = (#target).clone(); });
        let condition = arm
            .guard
            .map(|guard| quote! { #guard })
            .unwrap_or_else(|| quote! { true });
        if let Some(binding) = arm.binding {
            branches.push(quote! { if let Some(#binding) = #c.options(&#handle) { if #condition { return #body; } } });
        } else {
            branches.push(
                quote! { if #c.options(&#handle).is_some() { if #condition { return #body; } } },
            );
        }
    }
    Ok(quote! {{
        #(#clones)*
        #macroquad::theme::theme(move |#c: #macroquad::theme::ComponentRef<'_>, #s: #macroquad::style::Style| {
            #(#branches)*
        })
    }})
}
