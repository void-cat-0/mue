use std::collections::HashSet;

use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{
    Expr, FnArg, GenericParam, Generics, Ident, ItemFn, Meta, Pat, ReturnType, Type,
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    visit_mut::{self, VisitMut},
};

pub(crate) fn expand(input: TokenStream, node: bool) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    let result = crate::paths::host().and_then(|host| expand_fn(input, node, host));
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

struct Argument {
    ident: Ident,
    value_ty: Type,
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
    if node && matches!(input.sig.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "node functions must return a NodePlan",
        ));
    }

    let mut theme = None;
    let mut attrs = Vec::new();
    for attr in std::mem::take(&mut input.attrs) {
        if !attr.path().is_ident("theme") {
            attrs.push(attr);
            continue;
        }
        if theme.is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate theme attribute"));
        }
        let fields = match &attr.meta {
            Meta::Path(_) => Vec::new(),
            Meta::List(_) => attr
                .parse_args_with(Punctuated::<Ident, syn::Token![,]>::parse_terminated)?
                .into_iter()
                .collect(),
            Meta::NameValue(_) => {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected #[theme] or #[theme(field, ...)]",
                ));
            }
        };
        theme = Some(fields);
    }
    input.attrs = attrs;

    let mut args = Vec::new();
    for arg in &mut input.sig.inputs {
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
        if pat.ident == "style" {
            return Err(syn::Error::new_spanned(
                arg,
                "style is reserved; return node defaults in a NodePlan instead of taking a style parameter",
            ));
        }
        let ident = pat.ident.clone();
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
                    "theme is a function-level allowlist",
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
            value_ty,
            ty,
            default,
            model,
        });
    }

    let names = args.iter().map(|arg| arg.ident.to_string()).collect();
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

    let mut selected = Vec::new();
    if let Some(fields) = &theme {
        let mut seen = HashSet::new();
        for field in fields {
            if !seen.insert(field.to_string()) {
                return Err(syn::Error::new_spanned(field, "duplicate theme option"));
            }
            if field == "style" {
                return Err(syn::Error::new_spanned(
                    field,
                    "style cannot be a theme option",
                ));
            }
            let arg = args
                .iter()
                .find(|arg| arg.ident == *field)
                .ok_or_else(|| syn::Error::new_spanned(field, "unknown theme option parameter"))?;
            validate_option_type(&arg.value_ty, &input.sig.generics)?;
            selected.push(arg);
        }
    }

    let ident = &input.sig.ident;
    let vis = &input.vis;
    let camel = ident.to_string().to_upper_camel_case();
    let builder = format_ident!("{}Builder", camel, span = ident.span());
    let alias = Ident::new(&camel, ident.span());
    let options = format_ident!("{}Options", camel, span = ident.span());
    let generics = &input.sig.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let prop = quote!(#host::__private::core::prop::Prop);
    let prop_value = quote!(#host::__private::core::prop::PropValue);
    let access = quote!(#host::__private::core::signal::Access);
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

    let mut prepare_generics = generics.clone();
    if theme.is_some() {
        prepare_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #builder #ty_generics: 'static
            ));
    }
    let (prepare_impl_generics, _, prepare_where) = prepare_generics.split_for_impl();
    let theme_impl = theme.as_ref().map(|_| {
        let option_type = if selected.is_empty() { quote!(()) } else { quote!(#options) };
        quote! {
            impl #prepare_impl_generics #host::theme::ThemeTarget for #builder #ty_generics #prepare_where {
                type Options = #option_type;
            }
        }
    });
    let option_fields: Vec<_> = selected
        .iter()
        .map(|arg| {
            let Argument {
                ident, value_ty, ..
            } = arg;
            quote!(#ident: #prop<#value_ty>,)
        })
        .collect();
    let option_bounds: Vec<_> = selected
        .iter()
        .map(|arg| {
            let ty = &arg.value_ty;
            quote!(#ty: #prop_value + ::std::clone::Clone + 'static)
        })
        .collect();
    let getters = selected.iter().map(|arg| {
        let Argument {
            ident, value_ty, ..
        } = arg;
        quote! {
            pub fn #ident(&self) -> #value_ty {
                #access::get_clone(&self.#ident)
            }
        }
    });
    let option_decl = (!selected.is_empty()).then(|| {
        quote! {
            pub struct #options where #(#option_bounds,)* {
                #(#option_fields)*
            }
            impl #options where #(#option_bounds,)* {
                #(#getters)*
            }
        }
    });
    let option_values = selected.iter().map(|arg| {
        let ident = &arg.ident;
        if arg.model {
            quote!(#ident: ::std::convert::Into::<#prop<_>>::into(::std::clone::Clone::clone(&#ident)),)
        } else {
            quote!(#ident: ::std::clone::Clone::clone(&#ident),)
        }
    });
    let option_value = if selected.is_empty() {
        quote!(())
    } else {
        quote!(#options { #(#option_values)* })
    };
    let root = Ident::new("__mue_root", Span::mixed_site());
    let scope = Ident::new("__mue_scope", Span::mixed_site());
    let result = Ident::new("__mue_result", Span::mixed_site());
    let push_target = theme.as_ref().map(|_| {
        quote! {
            if #root.has_theme() {
                #root.push_target::<Self>(::std::rc::Rc::new(#option_value));
            }
        }
    });
    let finish = if node {
        quote!(#root.finish(#result))
    } else {
        quote!(#host::node::PrepareNode::prepare(#result, #root))
    };
    let mut helper_name = "__mue_original".to_owned();
    while args.iter().any(|arg| arg.ident == helper_name.as_str()) {
        helper_name.push('_');
    }
    let helper = Ident::new(&helper_name, Span::mixed_site());
    let mut original = input.clone();
    original.sig.ident = helper.clone();
    original.vis = syn::Visibility::Inherited;

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
        #option_decl
        #theme_impl
        impl #prepare_impl_generics #host::node::PrepareNode for #builder #ty_generics #prepare_where {
            fn prepare(self, mut #root: #host::node::RootBuild) -> #host::node::PreparedNode {
                #original
                let #scope = #root.scope();
                #scope.run(move || {
                    #(#effective)*
                    #root.push_style(self.style);
                    #push_target
                    let #result = #helper #turbofish (#(#invocation_args),*);
                    #finish
                })
            }
        }
        impl #prepare_impl_generics #host::node::IntoNode for #builder #ty_generics #prepare_where {
            fn into_node(self) -> #host::node::Node { #host::node::build(self) }
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

fn validate_option_type(ty: &Type, generics: &Generics) -> syn::Result<()> {
    let names = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(param.ident.to_string()),
            GenericParam::Const(param) => Some(param.ident.to_string()),
            GenericParam::Lifetime(_) => None,
        })
        .collect();
    let mut validator = OptionValidator { names, error: None };
    validator.visit_type_mut(&mut ty.clone());
    match validator.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct OptionValidator {
    names: HashSet<String>,
    error: Option<syn::Error>,
}

impl VisitMut for OptionValidator {
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
                "theme options cannot depend on open generic parameters",
            ));
            return;
        }
        visit_mut::visit_expr_path_mut(self, path);
    }

    fn visit_type_mut(&mut self, ty: &mut Type) {
        if matches!(
            ty,
            Type::ImplTrait(_)
                | Type::TraitObject(_)
                | Type::BareFn(_)
                | Type::Infer(_)
                | Type::Macro(_)
                | Type::Verbatim(_)
        ) {
            self.error = Some(syn::Error::new_spanned(
                ty,
                "theme options must be concrete value types, not callbacks or trait objects",
            ));
            return;
        }
        if matches!(ty, Type::Reference(reference) if reference.lifetime.is_none()) {
            self.error = Some(syn::Error::new_spanned(
                ty,
                "theme options require a concrete static lifetime",
            ));
            return;
        }
        visit_mut::visit_type_mut(self, ty);
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        if path.leading_colon.is_none()
            && path
                .segments
                .first()
                .is_some_and(|segment| self.names.contains(&segment.ident.to_string()))
        {
            self.error = Some(syn::Error::new_spanned(
                path,
                "theme options cannot depend on open generic parameters",
            ));
            return;
        }
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
        if lifetime.ident != "static" {
            self.error = Some(syn::Error::new(
                lifetime.span(),
                "theme options require a concrete static lifetime",
            ));
        }
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
    fn ordinary_builders_use_prepare_without_theme_metadata_or_clone_bounds() {
        let generated = expand("pub fn plain<T>(value: T) -> Plan { unreachable!() }", true)
            .unwrap()
            .to_string();
        assert!(
            generated.contains("PrepareNode for PlainBuilder < T >"),
            "{generated}"
        );
        assert!(generated.contains("scope ()"));
        assert!(generated.contains("run (move ||"));
        assert!(generated.contains("push_style (self . style)"));
        assert!(generated.contains("finish (__mue_result)"));
        assert!(generated.contains("node :: build (self)"));
        assert!(!generated.contains("ThemeTarget"));
        assert!(!generated.contains("PropValue"));
        assert!(!generated.contains("Clone"));
        assert!(!generated.contains("'static"));
        assert!(!generated.contains("build_with_style"));
    }

    #[test]
    fn bare_and_empty_theme_have_unit_options_and_no_options_struct() {
        for attr in ["#[theme]", "#[theme()]"] {
            let generated = expand(
                &format!("{attr} pub fn empty() -> Plan {{ unreachable!() }}"),
                true,
            )
            .unwrap()
            .to_string();
            assert!(generated.contains("type Options = ()"));
            assert!(generated.contains("has_theme ()"));
            assert!(generated.contains("push_target :: < Self >"));
            assert!(!generated.contains("struct EmptyOptions"));
            assert!(!generated.contains("# [theme"));
        }
    }

    #[test]
    fn only_listed_values_are_cloned_into_private_props_with_public_tracked_getters() {
        let generated = expand(
            "#[theme(active, level)] pub fn badge<T>(text: T, #[default] active: bool, #[model] level: u32) -> Plan { unreachable!() }",
            true,
        ).unwrap();
        let file: syn::File = syn::parse2(generated.clone()).unwrap();
        let options = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Struct(item) if item.ident == "BadgeOptions" => Some(item),
                _ => None,
            })
            .unwrap();
        assert!(matches!(options.vis, syn::Visibility::Public(_)));
        assert!(options.generics.params.is_empty());
        assert_eq!(options.fields.len(), 2);
        assert!(
            options
                .fields
                .iter()
                .all(|field| matches!(field.vis, syn::Visibility::Inherited))
        );
        for field in &options.fields {
            assert!(
                field
                    .ty
                    .to_token_stream()
                    .to_string()
                    .contains("prop :: Prop")
            );
        }
        let rendered = generated.to_string();
        assert!(rendered.contains("pub fn active (& self) -> bool"));
        assert!(rendered.contains("Access :: get_clone"));
        assert!(rendered.contains("PropValue + :: std :: clone :: Clone + 'static"));
        assert!(!rendered.contains("Clone :: clone (& text)"));
        assert!(!rendered.contains("T :"));
        assert!(rendered.contains("Into :: <") && rendered.contains("Clone :: clone (& level)"));
        let default = rendered.find("let active =").unwrap();
        let clone = rendered.find("Clone :: clone (& active)").unwrap();
        let invocation = rendered
            .find("let __mue_result = __mue_original :: < T >")
            .unwrap();
        assert!(default < clone && clone < invocation);
    }

    #[test]
    fn declaration_impl_alias_and_helper_keep_type_lifetime_and_const_generics_separate() {
        let generated = expand(
            "#[theme(active)] pub(crate) fn generic<'a, T: ?Sized, const N: usize>(#[default] active: bool) -> Plan where T: 'a { unreachable!() }",
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
        assert!(
            generated.contains("fn __mue_original < 'a , T : ? Sized , const N : usize >"),
            "{generated}"
        );
        assert!(generated.contains("__mue_original :: < T , N > (active)"));
        assert!(generated.contains("GenericBuilder :: < T , N > :: new"));
        assert!(generated.contains("PhantomData < fn () -> (& 'a () , * const T ,) >"));
    }

    #[test]
    fn components_invoke_original_once_then_prepare_the_returned_builder() {
        let generated = expand(
            "pub fn wrapped<T, const N: usize>() -> Inner<T, N> { make::<T, N>() }",
            false,
        )
        .unwrap()
        .to_string();
        assert!(generated.contains("__mue_original :: < T , N > ()"));
        assert_eq!(generated.matches("let __mue_result =").count(), 1);
        assert!(generated.contains("PrepareNode :: prepare (__mue_result , __mue_root)"));
        assert!(!generated.contains("provide_defaults"));
        assert!(!generated.contains("finish (__mue_result)"));
    }

    #[test]
    fn theme_allowlist_rejects_duplicate_unknown_reserved_and_parameter_annotations() {
        for (source, message) in [
            (
                "#[theme(a, a)] fn a_node(a: bool) -> Plan {}",
                "duplicate theme option",
            ),
            (
                "#[theme(missing)] fn a_node(a: bool) -> Plan {}",
                "unknown theme option",
            ),
            ("#[theme(style)] fn a_node() -> Plan {}", "style cannot"),
            (
                "#[theme] #[theme(a)] fn a_node(a: bool) -> Plan {}",
                "duplicate theme attribute",
            ),
            (
                "#[theme = true] fn a_node() -> Plan {}",
                "expected #[theme]",
            ),
            (
                "fn a_node(#[theme] a: bool) -> Plan {}",
                "function-level allowlist",
            ),
            (
                "fn a_node(style: &mut Style) -> Plan {}",
                "style is reserved",
            ),
            (
                "fn a_node(#[model(other)] a: bool) -> Plan {}",
                "single #[model]",
            ),
            (
                "fn a_node(#[model] #[model] a: bool) -> Plan {}",
                "single #[model]",
            ),
            (
                "fn a_node(#[default] #[default] a: bool) -> Plan {}",
                "duplicate default",
            ),
            (
                "fn a_node(#[default(a)] a: bool) -> Plan {}",
                "cannot reference parameters",
            ),
            (
                "fn a_node(a: bool, #[default(a)] b: bool) -> Plan {}",
                "cannot reference parameters",
            ),
            ("fn a_node() {}", "must return a NodePlan"),
        ] {
            let error = expand(source, true).unwrap_err();
            assert!(error.to_string().contains(message), "{source}: {error}");
        }
    }

    #[test]
    fn theme_options_reject_open_generics_callbacks_and_unresolved_lifetimes() {
        for (generic, ty) in [
            ("T", "T"),
            ("T", "Vec<T>"),
            ("T: Trait", "T::Assoc"),
            ("T: Trait", "<T as Trait>::Assoc"),
            ("const N: usize", "[u8; N]"),
            ("const N: usize", "Container<{ N + 1 }>"),
            ("const N: usize", "[u8; { N + 1 }]"),
            ("'a", "&'a str"),
            ("", "&str"),
            ("", "impl FnOnce()"),
            ("", "fn()"),
            ("", "unsafe extern \"C\" fn()"),
            ("", "Box<dyn Fn()>"),
            ("", "dyn Trait"),
        ] {
            let generic = if generic.is_empty() {
                String::new()
            } else {
                format!("<{generic}>")
            };
            let source = format!("#[theme(value)] fn invalid{generic}(value: {ty}) -> Plan {{}}");
            assert!(expand(&source, true).is_err(), "accepted {source}");
        }
        for ty in [
            "bool",
            "Vec<u32>",
            "ConcreteAlias",
            "<Concrete as Trait>::Assoc",
            "&'static str",
            "[u8; 4]",
        ] {
            let source =
                format!("#[theme(value)] fn valid<T, const N: usize>(value: {ty}) -> Plan {{}}");
            assert!(expand(&source, true).is_ok(), "rejected {source}");
        }
    }
}
