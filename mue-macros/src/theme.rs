use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span as TokenSpan, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::visit_mut::VisitMut;
use syn::{
    Expr, Ident, Token, braced, parenthesized,
    parse::{Parse, ParseStream},
    token,
};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let tokens: TokenStream2 = input.into();
    let result = if matches!(tokens.clone().into_iter().next(), Some(TokenTree::Punct(p)) if p.as_char() == '|')
    {
        syn::parse2::<ThemeInput>(tokens).and_then(expand_theme)
    } else {
        syn::parse2::<SelectorTheme>(tokens).and_then(expand_selectors)
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
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

struct SelectorTheme {
    rules: Vec<SelectorRule>,
}
struct SelectorRule {
    chain: Vec<(Option<Ident>, SelectorRelation)>,
    predicate: Option<Expr>,
    hover: bool,
    class: Option<String>,
    body: TokenStream2,
}
#[derive(Clone, Copy)]
enum SelectorRelation {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

impl syn::parse::Parse for SelectorTheme {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut rules = Vec::new();
        while !input.is_empty() {
            let mut selectors = vec![parse_selector(input)?];
            while input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                selectors.push(parse_selector(input)?);
            }
            let body;
            braced!(body in input);
            let body: TokenStream2 = body.parse()?;
            rules.extend(selectors.into_iter().map(|mut selector| {
                selector.body = body.clone();
                selector
            }));
            if input.peek(Token![;]) {
                input.parse::<Token![;]>()?;
            }
        }
        Ok(Self { rules })
    }
}

fn parse_selector(input: syn::parse::ParseStream) -> syn::Result<SelectorRule> {
    let mut chain = Vec::new();
    let mut predicate = None;
    let mut hover = false;
    let first = if input.peek(Token![*]) {
        input.parse::<Token![*]>()?;
        None
    } else {
        Some(input.parse::<Ident>()?)
    };
    chain.push((first, SelectorRelation::Descendant));
    loop {
        let relation = if input.peek(Token![>]) {
            input.parse::<Token![>]>()?;
            Some(SelectorRelation::Child)
        } else if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            Some(SelectorRelation::AdjacentSibling)
        } else if input.peek(Token![~]) {
            input.parse::<Token![~]>()?;
            Some(SelectorRelation::GeneralSibling)
        } else if input.peek(Ident) || input.peek(Token![*]) {
            Some(SelectorRelation::Descendant)
        } else {
            None
        };
        let Some(relation) = relation else { break };
        let target = if input.peek(Token![*]) {
            input.parse::<Token![*]>()?;
            None
        } else {
            Some(input.parse::<Ident>()?)
        };
        chain.push((target, relation));
    }
    let class = if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        Some(input.parse::<Ident>()?.to_string())
    } else {
        None
    };
    if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        let expr: Expr = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("expected one option expression in selector brackets"));
        }
        predicate = Some(expr);
    }
    if input.peek(Token![:]) {
        input.parse::<Token![:]>()?;
        let pseudo: Ident = input.parse()?;
        if pseudo != "hover" {
            return Err(syn::Error::new(pseudo.span(), "only :hover is supported"));
        }
        hover = true;
    }
    Ok(SelectorRule {
        chain,
        predicate,
        hover,
        class,
        body: TokenStream2::new(),
    })
}

struct PrefixOptionPaths;
impl syn::visit_mut::VisitMut for PrefixOptionPaths {
    fn visit_expr_mut(&mut self, expression: &mut syn::Expr) {
        if let syn::Expr::Path(path) = expression
            && path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
        {
            let ident = path.path.segments[0].ident.clone();
            if ident != "true" && ident != "false" {
                // Selector expressions use the option's native value.  This
                // keeps ordinary fields (`progress < 32`) useful while still
                // allowing reactive fields through an explicit `.get()`.
                *expression = syn::parse_quote!(options.#ident);
                return;
            }
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }
}

fn expand_selectors(input: SelectorTheme) -> syn::Result<TokenStream2> {
    let host = crate::paths::host()?;
    let mut indexed: Vec<_> = input.rules.into_iter().enumerate().collect();
    indexed.sort_by_key(|(_, rule)| {
        let types = rule.chain.iter().filter(|(name, _)| name.is_some()).count();
        let predicates = usize::from(rule.predicate.is_some())
            + usize::from(rule.class.is_some())
            + usize::from(rule.hover);
        types * 100 + predicates * 10
    });
    let mut clones = Vec::new();
    let mut rules = Vec::new();
    for (index, rule) in indexed {
        let body = rule.body;
        let style = syn::parse2::<crate::style::Declarations>(body)?.expand()?;
        let mut handles = Vec::new();
        for (position, (name, _)) in rule.chain.iter().enumerate() {
            if let Some(name) = name {
                let handle = syn::Ident::new(
                    &format!("__mue_selector_{index}_{position}"),
                    TokenSpan::mixed_site(),
                );
                clones.push(quote! { let #handle = (#name).clone(); });
                handles.push(Some(handle));
            } else {
                handles.push(None);
            }
        }
        let chain_ids = handles.iter().map(|handle| match handle {
            Some(handle) => quote!(#handle.identity()),
            None => quote!(0usize),
        });
        let relations: Vec<_> = rule
            .chain
            .iter()
            .skip(1)
            .map(|(_, relation)| match relation {
                SelectorRelation::Descendant => 0u8,
                SelectorRelation::Child => 1u8,
                SelectorRelation::AdjacentSibling => 2u8,
                SelectorRelation::GeneralSibling => 3u8,
            })
            .collect();
        let chain_match = if rule.chain.len() == 1 && handles[0].is_none() {
            quote!(true)
        } else {
            quote!(reference.matches_chain(&[#(#chain_ids),*], &[#(#relations),*]))
        };
        let option = if let Some(mut expr) = rule.predicate {
            let last = handles.last().and_then(Clone::clone).ok_or_else(|| {
                syn::Error::new(
                    TokenSpan::call_site(),
                    "an option selector needs a named final component",
                )
            })?;
            if let Expr::Path(path) = &expr
                && path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1
            {
                let field = &path.path.segments[0].ident;
                let class = syn::LitStr::new(&field.to_string(), field.span());
                quote!(reference.has_class(#class) || reference.options(&#last).is_some_and(|options| options.#field.is_set()))
            } else {
                PrefixOptionPaths.visit_expr_mut(&mut expr);
                quote!(reference.options(&#last).is_some_and(|options| #expr))
            }
        } else {
            quote!(true)
        };
        let class = if let Some(class) = rule.class {
            let class = syn::LitStr::new(&class, TokenSpan::call_site());
            quote!(reference.has_class(#class))
        } else {
            quote!(true)
        };
        let hover = if rule.hover {
            quote!(reference.is_hovered())
        } else {
            quote!(true)
        };
        rules.push(quote! {
            if #chain_match && #option && #class && #hover {
                __mue_theme_style = #host::style::Styleable::style(__mue_theme_style, #style);
            }
        });
    }
    Ok(quote! {{
        #(#clones)*
        use #host::__private::core::signal::Access as _;
        use #host::theme::SelectorValue as _;
        #host::theme::theme(move |reference, mut __mue_theme_style| {
            #(#rules)*
            __mue_theme_style
        })
    }})
}
