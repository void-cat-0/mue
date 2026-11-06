use std::collections::HashSet;

use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{
    Expr, FnArg, GenericParam, Ident, ItemFn, Meta, Pat, Type, parse_macro_input, parse_quote,
    visit_mut::{self, VisitMut},
};

pub(crate) fn expand(attr: TokenStream, input: TokenStream, node: bool) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(
            Tokens::from(attr),
            "node and component attributes do not accept arguments",
        )
        .into_compile_error()
        .into();
    }
    let input = parse_macro_input!(input as ItemFn);
    let result = crate::paths::host().and_then(|host| expand_fn(input, node, host));
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

struct Argument {
    ident: Ident,
    ty: Type,
    default: Option<Expr>,
    model: bool,
}

fn expand_fn(mut input: ItemFn, node: bool, host: Tokens) -> syn::Result<Tokens> {
    if input.sig.asyncness.is_some()
        || input.sig.constness.is_some()
        || input.sig.unsafety.is_some()
        || input.sig.abi.is_some()
        || input.sig.variadic.is_some()
    {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "node and component functions must be safe synchronous Rust functions",
        ));
    }
    if let Some(attr) = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("theme"))
    {
        return Err(syn::Error::new_spanned(
            attr,
            "the theme attribute is no longer supported",
        ));
    }

    let mut args = Vec::new();
    let mut injected_context = None;
    for (position, arg) in input.sig.inputs.iter_mut().enumerate() {
        let FnArg::Typed(arg) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "node and component functions cannot take self",
            ));
        };
        let Pat::Ident(pat) = arg.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &arg.pat,
                "parameters must be simple identifiers",
            ));
        };
        if pat.by_ref.is_some() || pat.subpat.is_some() {
            return Err(syn::Error::new_spanned(
                pat,
                "parameters must be simple identifiers",
            ));
        }
        let ident = pat.ident.clone();
        if node && position == 0 {
            for attr in &arg.attrs {
                if attr.path().is_ident("default") || attr.path().is_ident("model") {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "the injected context parameter cannot have #[default] or #[model] attributes",
                    ));
                }
                if attr.path().is_ident("theme") {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "the theme attribute is no longer supported",
                    ));
                }
            }
            injected_context = Some(ident);
            continue;
        }
        if ident == "style" {
            return Err(syn::Error::new_spanned(
                arg,
                "style is reserved for the generated builder style field",
            ));
        }
        if ident == "build" {
            return Err(syn::Error::new_spanned(
                arg,
                "build is reserved for the injected first parameter of #[node] functions",
            ));
        }
        let mut default = None;
        let mut model = false;
        let mut retained = Vec::new();
        for attr in std::mem::take(&mut arg.attrs) {
            if attr.path().is_ident("default") {
                if default.is_some() {
                    return Err(syn::Error::new_spanned(attr, "duplicate default attribute"));
                }
                default = Some(match &attr.meta {
                    Meta::Path(_) => parse_quote!(::std::default::Default::default()),
                    Meta::List(_) => attr.parse_args::<Expr>()?,
                    Meta::NameValue(_) => {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "expected #[default] or #[default(expression)]",
                        ));
                    }
                });
            } else if attr.path().is_ident("model") {
                if model || !matches!(attr.meta, Meta::Path(_)) {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "expected a single #[model] attribute",
                    ));
                }
                model = true;
            } else if attr.path().is_ident("theme") {
                return Err(syn::Error::new_spanned(
                    attr,
                    "the theme attribute is no longer supported",
                ));
            } else {
                retained.push(attr);
            }
        }
        arg.attrs = retained;
        let value_ty = (*arg.ty).clone();
        let ty: Type = if model {
            parse_quote!(#host::__private::core::signal::Signal<#value_ty>)
        } else {
            parse_quote!(#host::__private::core::prop::Prop<#value_ty>)
        };
        arg.ty = Box::new(ty.clone());
        args.push(Argument {
            ident,
            ty,
            default,
            model,
        });
    }

    if node && injected_context.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "#[node] functions require an injected first parameter",
        ));
    }

    let mut names = args
        .iter()
        .map(|arg| arg.ident.to_string())
        .collect::<HashSet<_>>();
    // Defaults cannot capture ordinary parameters or the injected context.
    if let Some(context) = &injected_context {
        names.insert(context.to_string());
    }
    for arg in &args {
        if let Some(mut default) = arg.default.clone() {
            let mut validator = DefaultValidator {
                names: &names,
                error: None,
            };
            validator.visit_expr_mut(&mut default);
            if let Some(error) = validator.error {
                return Err(error);
            }
        }
    }

    let ident = &input.sig.ident;
    let vis = &input.vis;
    let camel = ident.to_string().to_upper_camel_case();
    let builder = format_ident!("{}Builder", camel, span = ident.span());
    let alias = Ident::new(&camel, ident.span());
    let generics = &input.sig.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let prop = quote!(#host::__private::core::prop::Prop);
    let attrs = &input.attrs;

    let marker_types: Vec<_> = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(quote!(*const #ident))
            }
            GenericParam::Lifetime(param) => {
                let lifetime = &param.lifetime;
                Some(quote!(&#lifetime ()))
            }
            GenericParam::Const(_) => None,
        })
        .collect();
    let mut marker_name = "__mue_marker".to_owned();
    while args.iter().any(|arg| arg.ident == marker_name.as_str()) {
        marker_name.push('_');
    }
    let marker = Ident::new(&marker_name, Span::mixed_site());
    let marker_field = (!marker_types.is_empty()).then(|| {
        quote! {
            #marker: ::std::marker::PhantomData<fn() -> (#(#marker_types,)*)>,
        }
    });
    let marker_init = (!marker_types.is_empty()).then(|| {
        quote! {
            #marker: ::std::marker::PhantomData,
        }
    });
    let fields = args.iter().map(|arg| {
        let Argument {
            ident, ty, default, ..
        } = arg;
        if default.is_some() {
            quote!(#ident: ::std::option::Option<#ty>,)
        } else {
            quote!(#ident: #ty,)
        }
    });
    let required: Vec<_> = args.iter().filter(|arg| arg.default.is_none()).collect();
    let constructor_args: Vec<_> = required
        .iter()
        .map(|arg| {
            let Argument { ident, ty, .. } = arg;
            quote!(#ident: impl ::std::convert::Into<#ty>)
        })
        .collect();
    let constructor_names = required.iter().map(|arg| &arg.ident);
    let initializers = args.iter().map(|arg| {
        let ident = &arg.ident;
        if arg.default.is_some() {
            quote!(#ident: ::std::option::Option::None,)
        } else {
            quote!(#ident: ::std::convert::Into::into(#ident),)
        }
    });
    let setters = args.iter().map(|arg| {
        let Argument {
            ident, ty, default, ..
        } = arg;
        let value = if default.is_some() {
            quote!(::std::option::Option::Some(::std::convert::Into::into(
                value
            )))
        } else {
            quote!(::std::convert::Into::into(value))
        };
        quote! {
            pub fn #ident(mut self, value: impl ::std::convert::Into<#ty>) -> Self {
                self.#ident = #value;
                self
            }
        }
    });
    let effective = args.iter().map(|arg| {
        let Argument {
            ident,
            default,
            model,
            ..
        } = arg;
        let value = match default {
            Some(default) => {
                let default = if *model {
                    quote!(#host::__private::core::signal::signal(#default))
                } else {
                    quote!(#prop::Static(#default))
                };
                quote!(self.#ident.unwrap_or_else(|| #default))
            }
            None => quote!(self.#ident),
        };
        quote!(let #ident = #value;)
    });
    let invocation_args = args.iter().map(|arg| &arg.ident);
    // A local item cannot capture impl generics. Declare them on the helper and
    // forward type/const arguments even when they only occur in its return type.
    let call_generics: Vec<_> = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(quote!(#ident))
            }
            GenericParam::Const(param) => {
                let ident = &param.ident;
                Some(quote!(#ident))
            }
            GenericParam::Lifetime(_) => None,
        })
        .collect();
    let turbofish = (!call_generics.is_empty()).then(|| quote!(::<#(#call_generics),*>));

    let build = Ident::new("__mue_build", Span::mixed_site());
    let scope = Ident::new("__mue_scope", Span::mixed_site());
    let result = Ident::new("__mue_result", Span::mixed_site());
    let finish = if node {
        quote!(#result)
    } else {
        quote!(#host::node::BuildNode::build(#result, #build))
    };
    let mut helper_name = "__mue_original".to_owned();
    while args.iter().any(|arg| arg.ident == helper_name.as_str()) {
        helper_name.push('_');
    }
    let helper = Ident::new(&helper_name, Span::mixed_site());
    let mut original = input.clone();
    original.sig.ident = helper.clone();
    original.vis = syn::Visibility::Inherited;
    let invoke = if node {
        quote!(#helper #turbofish (#build, #(#invocation_args),*))
    } else {
        quote!(#helper #turbofish (#(#invocation_args),*))
    };

    Ok(quote! {
        #[doc = concat!("Styleable builder returned by [`", stringify!(#ident), "`].")]
        #vis struct #builder #generics #where_clause {
            style: #host::style::Style,
            #(#fields)*
            #marker_field
        }
        #[allow(type_alias_bounds)]
        #vis type #alias #generics #where_clause = #builder #ty_generics;

        impl #impl_generics #builder #ty_generics #where_clause {
            pub fn new(#(#constructor_args),*) -> Self {
                Self {
                    style: ::std::default::Default::default(),
                    #(#initializers)*
                    #marker_init
                }
            }
            #(#setters)*
        }
        impl #impl_generics #host::style::Styleable for #builder #ty_generics #where_clause {
            fn style_mut(&mut self) -> &mut #host::style::Style { &mut self.style }
        }
        impl #impl_generics #host::node::BuildNode for #builder #ty_generics #where_clause {
            fn build(self, mut #build: #host::node::NodeContext) -> #host::node::Node {
                #original
                let #scope = #build.scope();
                #scope.run(move || {
                    #(#effective)*
                    #build.push_style(self.style);
                    let #result = #invoke;
                    #finish
                })
            }
        }
        #(#attrs)*
        #vis fn #ident #impl_generics(#(#constructor_args),*) -> #builder #ty_generics #where_clause {
            #builder #turbofish ::new(#(#constructor_names),*)
        }
    })
}

struct DefaultValidator<'a> {
    names: &'a HashSet<String>,
    error: Option<syn::Error>,
}

impl VisitMut for DefaultValidator<'_> {
    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
            && self
                .names
                .contains(&path.path.segments[0].ident.to_string())
        {
            self.error = Some(syn::Error::new_spanned(
                path,
                "default expressions cannot reference parameters",
            ));
            return;
        }
        visit_mut::visit_expr_path_mut(self, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn expand(source: &str, node: bool) -> syn::Result<Tokens> {
        let expanded = expand_fn(syn::parse_str(source)?, node, quote!(crate))?;
        syn::parse2::<syn::File>(expanded.clone())
            .unwrap_or_else(|error| panic!("generated invalid Rust: {error}\n{expanded}"));
        Ok(expanded)
    }

    #[test]
    fn ordinary_builders_call_the_original_with_the_owner_without_extra_bounds() {
        let generated = expand(
            "pub fn plain<T>(build: NodeContext, value: T) -> Node { build.finish(Style::new(), move |style| { drop(value); }) }",
            true,
        )
        .unwrap()
        .to_string();
        assert!(generated.contains("BuildNode for PlainBuilder < T >"));
        assert!(generated.contains("mut __mue_build : crate :: node :: NodeContext"));
        assert!(generated.contains("-> crate :: node :: Node"));
        assert!(generated.contains("scope ()"));
        assert!(generated.contains("run (move ||"));
        assert!(generated.contains("push_style (self . style)"));
        assert!(generated.contains("__mue_original :: < T > (__mue_build , value)"));
        assert_eq!(generated.matches("finish (").count(), 1);
        assert!(!generated.contains("IntoNode"));
        assert!(!generated.contains("Clone"));
        assert!(!generated.contains("'static"));
        for removed in [
            "NodePlan",
            "PrepareNode",
            "PreparedNode",
            "RootBuild",
            "DefaultStyle",
        ] {
            assert!(!generated.contains(removed));
        }
    }

    #[test]
    fn injected_context_and_node_return_are_preserved_in_original_function() {
        let generated = expand(
            "fn plain(mut ctx: host::ContextAlias, value: u32) -> host::NodeAlias { ctx.finish(Style::new(), move |style| { drop(value); }) }",
            true,
        )
        .unwrap();
        let file: syn::File = syn::parse2(generated.clone()).unwrap();
        for item in file.items {
            match item {
                syn::Item::Struct(item) => {
                    assert!(
                        !item
                            .fields
                            .iter()
                            .any(|field| field.ident.as_ref().unwrap() == "ctx")
                    );
                    let styles: Vec<_> = item
                        .fields
                        .iter()
                        .filter(|field| field.ident.as_ref().unwrap() == "style")
                        .collect();
                    assert_eq!(styles.len(), 1);
                    assert_eq!(
                        styles[0].ty.to_token_stream().to_string(),
                        "crate :: style :: Style"
                    );
                }
                syn::Item::Fn(item) => {
                    assert_eq!(item.sig.inputs.len(), 1);
                    assert!(
                        !item
                            .sig
                            .inputs
                            .to_token_stream()
                            .to_string()
                            .contains("ctx")
                    );
                }
                syn::Item::Impl(item) if item.trait_.is_none() => {
                    for item in item.items {
                        let syn::ImplItem::Fn(item) = item else {
                            continue;
                        };
                        assert_ne!(item.sig.ident, "ctx");
                        if item.sig.ident == "new" {
                            assert_eq!(item.sig.inputs.len(), 1);
                        }
                    }
                }
                _ => {}
            }
        }
        let generated = generated.to_string();
        assert!(generated.contains("mut ctx : host :: ContextAlias"));
        assert!(generated.contains("-> host :: NodeAlias"));
        assert!(!generated.contains("Prop < host :: ContextAlias"));
        // Rust checks the injected owner and the original function's return type.
        for source in [
            "fn plain(build: NodeContext) -> Node { todo!() }",
            "fn plain(cx: host::ContextAlias) -> host::NodeAlias { todo!() }",
            "fn plain(arbitrary: ArbitraryType) -> Node { todo!() }",
        ] {
            assert!(expand(source, true).is_ok());
        }
    }

    #[test]
    fn ordinary_build_parameter_remains_reserved() {
        assert!(
            expand("fn plain(build: u32) -> Inner { todo!() }", false)
                .unwrap_err()
                .to_string()
                .contains("build is reserved")
        );
    }

    #[test]
    fn injected_context_accepts_any_simple_name_and_is_not_a_builder_input() {
        for name in [
            "ctx",
            "cx",
            "context",
            "build",
            "style",
            "__mue_build",
            "__mue_original",
            "_unused",
            "r#type",
        ] {
            let source = format!(
                "fn plain(mut {name}: NodeContext, value: u32) -> Node {{ {name}.finish(Style::new(), |_| {{}}) }}"
            );
            let generated = expand(&source, true).unwrap();
            let file: syn::File = syn::parse2(generated).unwrap();
            let constructor = file
                .items
                .iter()
                .find_map(|item| match item {
                    syn::Item::Fn(item) => Some(item),
                    _ => None,
                })
                .unwrap();
            assert_eq!(constructor.sig.inputs.len(), 1, "{name}");
            let input = constructor
                .sig
                .inputs
                .first()
                .unwrap()
                .to_token_stream()
                .to_string();
            assert!(input.starts_with("value :"), "{name}: {input}");
        }
    }

    #[test]
    fn declaration_impl_alias_and_helper_keep_type_lifetime_and_const_generics_separate() {
        let generated = expand(
            "pub(crate) fn generic<'a, T: ?Sized, const N: usize>(build: NodeContext, #[default] active: bool) -> Node where T: 'a { todo!() }",
            true,
        ).unwrap();
        let file: syn::File = syn::parse2(generated.clone()).unwrap();
        for item in &file.items {
            match item {
                syn::Item::Struct(item) if item.ident == "GenericBuilder" => {
                    assert_eq!(item.generics.params.len(), 3);
                    assert!(matches!(item.generics.params[2], GenericParam::Const(_)));
                    assert!(item.generics.where_clause.is_some());
                }
                syn::Item::Type(item) if item.ident == "Generic" => {
                    assert!(matches!(item.vis, syn::Visibility::Restricted(_)));
                    assert_eq!(item.generics.params.len(), 3);
                    assert_eq!(
                        item.ty.to_token_stream().to_string(),
                        "GenericBuilder < 'a , T , N >"
                    );
                }
                _ => {}
            }
        }
        let generated = generated.to_string();
        assert!(generated.contains("fn __mue_original < 'a , T : ? Sized , const N : usize >"));
        assert!(generated.contains("__mue_original :: < T , N > (__mue_build , active)"));
        assert!(generated.contains("GenericBuilder :: < T , N > :: new"));
        assert!(generated.contains("PhantomData < fn () -> (& 'a () , * const T ,) >"));
    }

    #[test]
    fn components_invoke_original_once_then_build_the_returned_builder() {
        let generated = expand(
            "pub fn wrapped<T, const N: usize>() -> Inner<T, N> { make::<T, N>() }",
            false,
        )
        .unwrap()
        .to_string();
        assert!(generated.contains("__mue_original :: < T , N > ()"));
        assert_eq!(generated.matches("let __mue_result =").count(), 1);
        assert!(generated.contains("BuildNode :: build (__mue_result , __mue_build)"));
        let scope = generated.find("run (move ||").unwrap();
        let push = generated.find("push_style (self . style)").unwrap();
        let invoke = generated.find("let __mue_result =").unwrap();
        assert!(scope < push && push < invoke);
    }

    #[test]
    fn injected_context_and_reserved_style_have_specific_errors() {
        for (source, node, message) in [
            ("fn plain() {}", true, "require an injected first parameter"),
            (
                "fn plain(#[default] ctx: NodeContext) {}",
                true,
                "cannot have #[default] or #[model]",
            ),
            (
                "fn plain(#[default(make())] ctx: NodeContext) {}",
                true,
                "cannot have #[default] or #[model]",
            ),
            (
                "fn plain(#[model] cx: NodeContext) {}",
                true,
                "cannot have #[default] or #[model]",
            ),
            (
                "fn plain(ctx: NodeContext, style: Style) {}",
                true,
                "style is reserved",
            ),
        ] {
            let error = expand(source, node).unwrap_err();
            assert!(error.to_string().contains(message), "{source}: {error}");
        }
        for source in [
            "fn plain(ctx: NodeContext, #[default(ctx)] value: u32) {}",
            "fn plain(cx: NodeContext, #[default(cx)] value: u32) {}",
        ] {
            let error = expand(source, source.contains("NodeContext")).unwrap_err();
            assert!(
                error.to_string().contains("cannot reference parameters"),
                "{source}: {error}"
            );
        }
        assert!(expand("fn plain(#[default(style)] value: u32) -> Inner {}", false).is_ok());
        let error = expand(
            "fn plain(style: u32, #[default(style)] value: u32) -> Inner {}",
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("style is reserved"));
    }

    #[test]
    fn injected_context_name_is_free_but_ordinary_build_is_reserved() {
        for source in [
            "fn plain(build: NodeContext) -> Node { todo!() }",
            "fn plain(ctx: NodeContext) -> Node { todo!() }",
            "fn plain(style: NodeContext) -> Node { todo!() }",
        ] {
            assert!(expand(source, true).is_ok(), "{source}");
        }
        assert!(
            expand("fn plain(build: u32) -> Inner {}", false)
                .unwrap_err()
                .to_string()
                .contains("build is reserved")
        );
        assert!(
            expand("fn plain(ctx: NodeContext, build: u32) -> Node {}", true)
                .unwrap_err()
                .to_string()
                .contains("build is reserved")
        );
        for (source, node) in [
            ("fn plain(style: u32) -> Inner { todo!() }", false),
            (
                "fn plain(ctx: NodeContext, style: u32) -> Node { todo!() }",
                true,
            ),
        ] {
            assert!(
                expand(source, node)
                    .unwrap_err()
                    .to_string()
                    .contains("style is reserved"),
                "{source}"
            );
        }
    }
    #[test]
    fn invalid_parameter_annotations_and_signatures_are_rejected() {
        for (source, message) in [
            (
                "fn a_node(build: NodeContext, #[model(other)] a: bool) {}",
                "single #[model]",
            ),
            (
                "fn a_node(build: NodeContext, #[model] #[model] a: bool) {}",
                "single #[model]",
            ),
            (
                "fn a_node(build: NodeContext, #[default] #[default] a: bool) {}",
                "duplicate default",
            ),
            (
                "fn a_node(build: NodeContext, #[default(a)] a: bool) {}",
                "cannot reference parameters",
            ),
            (
                "fn a_node(build: NodeContext, a: bool, #[default(a)] b: bool) {}",
                "cannot reference parameters",
            ),
            (
                "async fn a_node(build: NodeContext) {}",
                "safe synchronous Rust functions",
            ),
            (
                "fn a_node(build: NodeContext, (a, b): (u32, u32)) {}",
                "simple identifiers",
            ),
        ] {
            let error = expand(source, true).unwrap_err();
            assert!(error.to_string().contains(message), "{source}: {error}");
        }
    }

    #[test]
    fn removed_attributes_are_rejected_on_functions_and_parameters() {
        for node in [false, true] {
            for attr in [
                "#[theme]",
                "#[theme()]",
                "#[theme(value)]",
                "#[theme = true]",
            ] {
                let build = if node { "build: NodeContext, " } else { "" };
                for source in [
                    format!("{attr} fn plain({build}value: bool) {{}}"),
                    format!("fn plain({build}{attr} value: bool) {{}}"),
                ] {
                    let error = expand(&source, node).unwrap_err();
                    assert!(
                        error
                            .to_string()
                            .contains("the theme attribute is no longer supported"),
                        "{source}: {error}"
                    );
                }
            }
        }
    }
}
