//! `token!(...)` values inside `style!` declarations.
use super::*;

/// Parse a `token!(key)` or `token!(theme, key)` value.
///
/// The style property determines the value type, so callers pass it in. A bare
/// key reads the nearest installed theme; the two-argument form takes an
/// explicit theme handle.
pub(super) fn token(input: ParseStream, ty: &TokenStream) -> Result<Option<TokenStream>> {
    let fork = input.fork();
    let Ok(mac) = fork.parse::<syn::Macro>() else {
        return Ok(None);
    };
    if !mac.path.is_ident("token") {
        return Ok(None);
    }

    let mac: syn::Macro = input.parse()?;
    let args: TokenArgs = syn::parse2(mac.tokens)?;
    let mut components = None;
    Ok(Some(expand_token(args, ty, &mut components)?))
}

/// Arguments of `token!(key)` or `token!(theme, key)`.
struct TokenArgs {
    theme: Option<syn::Expr>,
    key: TokenStream,
}

impl Parse for TokenArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let first: syn::Expr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            Ok(Self {
                theme: Some(first),
                key: token_key(input)?,
            })
        } else {
            Ok(Self {
                theme: None,
                key: token_key_expr(first)?,
            })
        }
    }
}

fn expand_token(
    args: TokenArgs,
    ty: &TokenStream,
    components: &mut Option<TokenStream>,
) -> Result<TokenStream> {
    let key = args.key;
    Ok(match args.theme {
        Some(theme) => quote!((#theme).get::<#ty>(#key)),
        None => {
            let components = match components {
                Some(components) => components.clone(),
                None => {
                    let resolved = crate::paths::components()?;
                    *components = Some(resolved.clone());
                    resolved
                }
            };
            quote!(
                #components::tokens::Theme::current()
                    .expect("token! needs an installed theme")
                    .get::<#ty>(#key)
            )
        }
    })
}

fn token_key(input: ParseStream) -> Result<TokenStream> {
    if input.peek(LitStr) {
        let key: LitStr = input.parse()?;
        Ok(quote!(#key))
    } else if input.peek(Ident) {
        let key: Ident = input.parse()?;
        Ok(quote!(::core::stringify!(#key)))
    } else {
        Err(input.error("expected a token key: an identifier or a string literal"))
    }
}

fn token_key_expr(expr: syn::Expr) -> Result<TokenStream> {
    match expr {
        syn::Expr::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            let key = &path.path.segments[0].ident;
            Ok(quote!(::core::stringify!(#key)))
        }
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(key),
            ..
        }) => Ok(quote!(#key)),
        other => Err(syn::Error::new_spanned(
            other,
            "expected a token key: an identifier or a string literal",
        )),
    }
}

/// Parse `{expr}` and rewrite `token!(...)` calls inside it with `ty`.
pub(super) fn expression_with_token(
    input: ParseStream,
    ty: &TokenStream,
) -> Result<Option<TokenStream>> {
    if !input.peek(syn::token::Brace) {
        return Ok(None);
    }
    let content;
    braced!(content in input);
    let mut value: syn::Expr = content.parse()?;
    if !content.is_empty() {
        return Err(content.error("expected one Rust expression"));
    }
    rewrite_token_calls(&mut value, ty)?;
    Ok(Some(quote!(#value)))
}

/// Rewrite `token!(key)` / `token!(theme, key)` calls inside a Rust expression.
fn rewrite_token_calls(expr: &mut syn::Expr, ty: &TokenStream) -> Result<()> {
    struct Rewriter<'a> {
        ty: &'a TokenStream,
        components: Option<TokenStream>,
        error: Option<syn::Error>,
    }

    impl Rewriter<'_> {
        fn expand_macro(&mut self, node: &syn::ExprMacro) -> Result<syn::Expr> {
            let args: TokenArgs = syn::parse2(node.mac.tokens.clone())?;
            let tokens = expand_token(args, self.ty, &mut self.components)?;
            syn::parse2(tokens)
        }
    }

    impl VisitMut for Rewriter<'_> {
        fn visit_expr_mut(&mut self, node: &mut syn::Expr) {
            if self.error.is_some() {
                return;
            }
            let is_token = matches!(
                node,
                syn::Expr::Macro(expr) if expr.mac.path.is_ident("token")
            );
            if !is_token {
                syn::visit_mut::visit_expr_mut(self, node);
                return;
            }
            let syn::Expr::Macro(expr) = node else {
                unreachable!("matched a token macro above");
            };
            match self.expand_macro(expr) {
                Ok(expr) => *node = expr,
                Err(error) => self.error = Some(error),
            }
        }
    }

    let mut rewriter = Rewriter {
        ty,
        components: None,
        error: None,
    };
    rewriter.visit_expr_mut(expr);
    match rewriter.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
