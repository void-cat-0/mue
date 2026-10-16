use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{FnArg, ItemFn, Meta, Pat, Type, parse_macro_input, parse_quote};

pub(crate) fn expand(input: TokenStream, node: bool) -> TokenStream {
    let macroquad = match crate_name("mue-macroquad").expect("mue-macroquad not found") {
        FoundCrate::Itself => quote! { crate },
        FoundCrate::Name(name) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { ::#ident }
        }
    };
    let option = quote! { ::std::option::Option };
    let signal = quote! { ::mue_core::signal::Signal };
    let prop = quote! { ::mue_core::prop::Prop };
    let into = quote! { ::std::convert::Into };

    let mut input = parse_macro_input!(input as ItemFn);
    let (impl_generics, ty_generics, where_clause) = input.sig.generics.split_for_impl();
    let vis = &input.vis;
    let ident = &input.sig.ident;
    let docs: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect();

    #[derive(Clone)]
    enum Arg {
        Style(Type),
        Arg {
            ident: syn::Ident,
            ty: proc_macro2::TokenStream,
            default: Option<proc_macro2::TokenStream>,
            is_length: bool,
        },
    }

    let args: Vec<_> = input
        .sig
        .inputs
        .iter_mut()
        .map(|f| match f {
            FnArg::Receiver(_) => panic!("Node functions cannot have `self` parameter"),
            FnArg::Typed(pat_type) => {
                let Pat::Ident(pat) = pat_type.pat.as_ref() else {
                    panic!("Node function parameters must be simple identifiers");
                };
                let ident = &pat.ident;
                if ident == "style" {
                    return Arg::Style(parse_quote!(#macroquad::style::Style));
                }

                let inner_ty = (*pat_type.ty).clone();

                let mut default = None;
                let mut is_model = false;
                let mut is_length = false;
                pat_type.attrs.retain_mut(|attr| {
                    if let Meta::Path(path) = &attr.meta {
                        if path.is_ident("default") {
                            default = Some(quote! { Default::default() });
                            return false;
                        } else if path.is_ident("length") {
                            is_length = true;
                            return false;
                        } else if path.is_ident("model") {
                            is_model = true;
                            return false;
                        }
                    }
                    if let Meta::List(list) = &attr.meta
                        && list.path.is_ident("default")
                    {
                        default = Some(list.tokens.clone());
                        return false;
                    }

                    true
                });

                if is_length {
                    assert!(!is_model, "#[length] cannot be combined with #[model]");
                    assert!(
                        matches!(&inner_ty, Type::Path(path) if path.path.is_ident("f32")),
                        "#[length] requires an f32 parameter"
                    );
                }

                if is_model {
                    *pat_type.ty = syn::parse_quote! { #signal<#inner_ty> };
                    Arg::Arg {
                        ident: ident.clone(),
                        is_length,
                        ty: quote! { #signal<#inner_ty> },
                        default: default.map(|d| quote! { ::mue_core::signal::signal(#d) }),
                    }
                } else {
                    *pat_type.ty = syn::parse_quote! { #prop<#inner_ty> };
                    Arg::Arg {
                        ident: ident.clone(),
                        is_length,
                        ty: quote! { #prop<#inner_ty> },
                        default: default.map(|d| quote! { #prop::Static(#d) }),
                    }
                }
            }
        })
        .collect();

    let mut fields = args.clone();
    if !fields.iter().any(|arg| matches!(arg, Arg::Style(_))) {
        fields.push(Arg::Style(parse_quote!(#macroquad::style::Style)));
    }

    let fields_decl = fields.iter().map(|arg| match arg {
        Arg::Style(ty) => {
            quote! { style: #ty }
        }
        Arg::Arg {
            ident, ty, default, ..
        } => {
            if default.is_some() {
                quote! { #ident: #option<#ty> }
            } else {
                quote! { #ident: #ty }
            }
        }
    });
    let setters = fields.iter().filter_map(|arg| match arg {
        Arg::Arg {
            ident,
            ty,
            default,
            is_length,
        } => {
            let bound = if *is_length {
                quote! { #macroquad::style::IntoLengthValue }
            } else {
                quote! { #into<#ty> }
            };
            let mut value = if *is_length {
                quote! { #macroquad::style::IntoLengthValue::into_length_value(value) }
            } else {
                quote! { #into::into(value) }
            };
            if default.is_some() {
                value = quote! { Some(#value) };
            }
            Some(quote! {
                pub fn #ident(mut self, value: impl #bound) -> Self {
                    self.#ident = #value;
                    self
                }
            })
        }
        _ => None,
    });

    let new_args: Vec<_> = args
        .iter()
        .filter_map(|arg| match arg {
            Arg::Arg {
                ident,
                ty,
                default,
                is_length,
            } => {
                if default.is_none() {
                    let bound = if *is_length {
                        quote! { #macroquad::style::IntoLengthValue }
                    } else {
                        quote! { #into<#ty> }
                    };
                    Some(quote! { #ident: impl #bound })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    let new_arg_names = args.iter().filter_map(|arg| match arg {
        Arg::Arg { ident, default, .. } => {
            if default.is_none() {
                Some(quote! { #ident })
            } else {
                None
            }
        }
        _ => None,
    });
    let prop_struct_init = fields.iter().map(|arg| match arg {
        Arg::Style(_) => quote! { style: #macroquad::style::Style::default() },
        Arg::Arg {
            ident,
            default,
            is_length,
            ..
        } => {
            if default.is_none() {
                if *is_length {
                    quote! { #ident: #macroquad::style::IntoLengthValue::into_length_value(#ident) }
                } else {
                    quote! { #ident: #into::into(#ident) }
                }
            } else {
                quote! { #ident: None }
            }
        }
    });

    let invoke_args = args.iter().map(|arg| match arg {
        Arg::Style(_) => {
            if node {
                quote! { style }
            } else {
                quote! { &mut self.style }
            }
        }
        Arg::Arg { ident, default, .. } => {
            let default = default.as_ref().map(|d| quote! { .unwrap_or_else(|| #d) });
            quote! { self.#ident #default }
        }
    });

    let builder_name = syn::Ident::new(
        &format!(
            "{}Builder",
            input.sig.ident.to_string().to_upper_camel_case()
        ),
        input.sig.ident.span(),
    );

    let into_node = if node {
        if args.iter().any(|arg| matches!(arg, Arg::Style(_))) {
            quote! {
                #macroquad::node::Node::build_with_style(self.style, move |style| {
                    #ident(#( #invoke_args ),*)
                })
            }
        } else {
            quote! {
                #macroquad::node::Node::build(move || #ident(#( #invoke_args ),*))
            }
        }
    } else {
        quote! {
            let mut result = #ident(#( #invoke_args ),*);
            result.style_mut().provide_defaults(self.style);
            #macroquad::node::IntoNode::into_node(result)
        }
    };
    let into_node_self = if node {
        quote! { self }
    } else {
        quote! { mut self }
    };

    let style_derive = quote! {
        impl #impl_generics #macroquad::style::Styleable for #builder_name #ty_generics #where_clause {
            fn style_mut(&mut self) -> &mut #macroquad::style::Style {
                &mut self.style
            }
        }
    };

    quote! {
        #[doc = concat!("Styleable builder returned by [`", stringify!(#ident), "`].")]
        pub struct #builder_name #ty_generics #where_clause {
            #( #fields_decl ),*
        }

        impl #impl_generics #builder_name #ty_generics #where_clause {
            pub fn new(#( #new_args ),*) -> Self {
                Self {
                    #( #prop_struct_init ),*
                }
            }

            #( #setters )*
        }

        impl #impl_generics #macroquad::node::IntoNode for #builder_name #ty_generics #where_clause {
            fn into_node(#into_node_self) -> #macroquad::node::Node {
                #input
                #into_node
            }
        }

        #style_derive

        #(#docs)*
        #vis fn #ident #impl_generics(#( #new_args ),*) -> #builder_name #ty_generics #where_clause {
            #builder_name::new(#( #new_arg_names ),*)
        }
    }
    .into()
}
