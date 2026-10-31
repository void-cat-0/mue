use proc_macro::TokenStream;
use proc_macro2::{Group, Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{
    Ident, LitStr, Token, TypePath, braced,
    parse::{Parse, ParseStream},
};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    syn::parse::<ThemeRules>(input)
        .and_then(expand_rules)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

pub(crate) fn expand_class(input: TokenStream) -> TokenStream {
    syn::parse::<ClassInput>(input)
        .and_then(expand_class_name)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

struct ClassInput(LitStr);

impl Parse for ClassInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let value: LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("class! accepts exactly one string literal"));
        }
        validate_class_name(&value.value())
            .map_err(|message| syn::Error::new(value.span(), message))?;
        Ok(Self(value))
    }
}

fn expand_class_name(input: ClassInput) -> syn::Result<TokenStream2> {
    let host = crate::paths::host()?;
    let value = input.0;
    Ok(quote! {
        const { #host::theme::ClassName::from_static_checked(#value) }
    })
}

fn validate_class_name(value: &str) -> Result<(), &'static str> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("class name cannot be empty");
    };
    if !first.is_ascii_alphabetic() {
        return Err("class name must start with an ASCII letter");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        return Err("class name may contain only ASCII letters, digits, '_' and '-'");
    }
    Ok(())
}

struct ThemeRules(Vec<RuleBlock>);

impl Parse for ThemeRules {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut rules = Vec::new();
        while !input.is_empty() {
            let mut selectors = vec![parse_compound(input)?];
            while input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                selectors.push(parse_compound(input)?);
            }
            if !input.peek(syn::token::Brace) {
                return Err(input.error(
                    "expected a style block; selector ids and combinators are not supported",
                ));
            }
            let body;
            braced!(body in input);
            let declarations: TokenStream2 = body.parse()?;
            validate_theme_declarations(declarations.clone())?;
            rules.push(RuleBlock {
                selectors,
                declarations,
            });
            if input.peek(Token![;]) {
                input.parse::<Token![;]>()?;
            }
        }
        Ok(Self(rules))
    }
}

struct RuleBlock {
    selectors: Vec<CompoundSelector>,
    declarations: TokenStream2,
}

struct ClassSelector {
    name: String,
    negated: bool,
    span: Span,
}

struct OptionPredicate {
    expression: syn::Expr,
    dollar_references: usize,
}

struct CompoundSelector {
    identity: Option<TypePath>,
    classes: Vec<ClassSelector>,
    predicate: Option<OptionPredicate>,
    hovered: bool,
}

impl CompoundSelector {
    fn specificity_class_count(&self) -> usize {
        self.classes.len()
            + usize::from(self.hovered)
            + self
                .predicate
                .as_ref()
                .map_or(0, |predicate| predicate.dollar_references)
    }
}

fn parse_compound(input: ParseStream<'_>) -> syn::Result<CompoundSelector> {
    let wildcard = input.peek(Token![*]);
    let identity = if wildcard {
        input.parse::<Token![*]>()?;
        None
    } else if input.peek(Ident)
        || input.peek(Token![::])
        || input.peek(Token![crate])
        || input.peek(Token![self])
        || input.peek(Token![super])
        || input.peek(Token![Self])
    {
        Some(input.parse::<TypePath>()?)
    } else {
        None
    };

    let mut classes = Vec::new();
    while input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let negated = if input.peek(Token![!]) {
            input.parse::<Token![!]>()?;
            true
        } else {
            false
        };
        let (name, span) = parse_class_lexer(input)?;
        classes.push(ClassSelector {
            name,
            negated,
            span,
        });
    }

    let predicate = if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        if content.is_empty() {
            return Err(content.error("option selector cannot be empty"));
        }
        if identity.is_none() {
            return Err(content.error("an option predicate requires a named type identity"));
        }
        let tokens: TokenStream2 = content.parse()?;
        let (tokens, dollar_references) = rewrite_dollar_references(tokens)?;
        Some(OptionPredicate {
            expression: syn::parse2(tokens)?,
            dollar_references,
        })
    } else {
        None
    };

    let hovered = if input.peek(Token![:]) {
        input.parse::<Token![:]>()?;
        let pseudo: Ident = input.parse()?;
        if pseudo != "hover" {
            return Err(syn::Error::new(pseudo.span(), "only :hover is supported"));
        }
        true
    } else {
        false
    };

    if identity.is_none() && !wildcard && classes.is_empty() && !hovered {
        return Err(input.error("selector compound must contain at least one condition"));
    }
    if input.peek(syn::token::Bracket) {
        return Err(input.error("an option predicate may occur once, before :hover"));
    }
    if input.peek(Token![:]) {
        return Err(input.error("only one :hover pseudo-class is supported"));
    }
    if input.peek(Token![.]) {
        return Err(input.error("class selectors must precede the option predicate and :hover"));
    }
    Ok(CompoundSelector {
        identity,
        classes,
        predicate,
        hovered,
    })
}

// A class starts with an ASCII identifier token. Hyphens separate identifier
// or numeric fragments; unrelated neighboring tokens never become one name.
fn parse_class_lexer(input: ParseStream<'_>) -> syn::Result<(String, Span)> {
    let first = input
        .parse::<TokenTree>()
        .map_err(|_| syn::Error::new(input.span(), "expected an ASCII class name after '.'"))?;
    let span = first.span();
    let mut name = match first {
        TokenTree::Ident(ident) => ident.to_string(),
        _ => {
            return Err(syn::Error::new(
                span,
                "class name must start with an ASCII letter",
            ));
        }
    };
    validate_class_name(&name).map_err(|message| syn::Error::new(span, message))?;

    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        name.push('-');
        let next = input.fork().parse::<TokenTree>();
        let fragment = match next {
            Ok(TokenTree::Ident(ident)) => ident.to_string(),
            Ok(TokenTree::Literal(literal)) => literal.to_string(),
            _ => continue,
        };
        if fragment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            let _ = input.parse::<TokenTree>()?;
            name.push_str(&fragment);
        }
    }
    validate_class_name(&name).map_err(|message| syn::Error::new(span, message))?;
    Ok((name, span))
}

// Rewriting token groups before syn parsing also reaches opaque macro inputs.
fn rewrite_dollar_references(tokens: TokenStream2) -> syn::Result<(TokenStream2, usize)> {
    let tokens: Vec<_> = tokens.into_iter().collect();
    let mut output = Vec::new();
    let mut references = 0;
    let options = Ident::new("__mue_options", Span::mixed_site());
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Punct(punct) if punct.as_char() == '$' => {
                let Some(TokenTree::Ident(field)) = tokens.get(index + 1) else {
                    return Err(syn::Error::new(
                        punct.span(),
                        "'$' in an option predicate must be followed by a field name",
                    ));
                };
                output.extend(quote!(#options . #field ()));
                references += 1;
                index += 2;
            }
            TokenTree::Group(group) => {
                let (inner, count) = rewrite_dollar_references(group.stream())?;
                let mut rewritten = Group::new(group.delimiter(), inner);
                rewritten.set_span(group.span());
                output.push(TokenTree::Group(rewritten));
                references += count;
                index += 1;
            }
            tree => {
                output.push(tree.clone());
                index += 1;
            }
        }
    }
    Ok((output.into_iter().collect(), references))
}

// Inspect declaration names only. Values remain opaque to this check so typed
// Rust expressions keep the exact semantics of style!.
fn validate_theme_declarations(tokens: TokenStream2) -> syn::Result<()> {
    let mut at_property = true;
    let mut property = String::new();
    for tree in tokens {
        if at_property {
            match &tree {
                TokenTree::Ident(ident) => property.push_str(&ident.to_string()),
                TokenTree::Punct(punct) if punct.as_char() == '-' => property.push('-'),
                TokenTree::Punct(punct) if punct.as_char() == ':' => {
                    let property = property.replace('_', "-");
                    if property == "children" || property == "hooks" || property.starts_with("on-")
                    {
                        return Err(syn::Error::new(
                            tree.span(),
                            "theme rules cannot declare children or event hooks",
                        ));
                    }
                    at_property = false;
                }
                _ => {}
            }
        } else if matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ';') {
            at_property = true;
            property.clear();
        }
    }
    Ok(())
}

fn expand_rules(input: ThemeRules) -> syn::Result<TokenStream2> {
    let host = crate::paths::host()?;
    let mut rules = Vec::with_capacity(input.0.len());
    for (rule_index, rule) in input.0.into_iter().enumerate() {
        let mut selectors = Vec::with_capacity(rule.selectors.len());
        for (selector_index, selector) in rule.selectors.into_iter().enumerate() {
            let component = Ident::new(
                &format!("__mue_component_{rule_index}_{selector_index}"),
                Span::mixed_site(),
            );
            let mut conditions = Vec::new();
            let class_count = u32::try_from(selector.specificity_class_count()).map_err(|_| {
                syn::Error::new(Span::call_site(), "selector specificity exceeds u32")
            })?;
            let type_count = if let Some(identity) = selector.identity.as_ref() {
                conditions.push(quote!(#component.is::<#identity>()));
                1u32
            } else {
                0u32
            };
            for class in selector.classes {
                let name = LitStr::new(&class.name, class.span);
                if class.negated {
                    conditions.push(quote!(!#component.has_class(#name)));
                } else {
                    conditions.push(quote!(#component.has_class(#name)));
                }
            }
            if let Some(predicate) = selector.predicate {
                let expression = predicate.expression;
                if predicate.dollar_references != 0 {
                    let options = Ident::new("__mue_options", Span::mixed_site());
                    let identity = selector.identity.expect("checked by parser");
                    conditions.push(quote! {
                        #component.options::<#identity>()
                            .is_some_and(|#options| #expression)
                    });
                } else {
                    conditions.push(quote!((#expression)));
                }
            }
            if selector.hovered {
                conditions.push(quote!(#component.hovered()));
            }
            let predicate = if conditions.is_empty() {
                quote!(true)
            } else {
                quote!(#(#conditions)&&*)
            };
            selectors.push(quote! {
                #host::theme::Selector::new(
                    #host::theme::Specificity(0, #class_count, #type_count),
                    move |#component: #host::theme::ComponentRef<'_>| -> bool { #predicate },
                )
            });
        }
        let declarations =
            syn::parse2::<crate::style::Declarations>(rule.declarations)?.expand()?;
        rules.push(quote! {
            #host::theme::Rule::new(
                ::std::vec![#(#selectors),*],
                move || #declarations,
            )
        });
    }
    Ok(quote! {
        #host::theme::Theme::new(::std::vec![#(#rules),*])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn parse(source: &str) -> syn::Result<ThemeRules> {
        syn::parse_str(source)
    }

    #[test]
    fn parses_generic_type_paths_and_lists_without_splitting_generic_commas() {
        let rules =
            parse("crate::widgets::Badge<u32, 4>, Button.primary.!disabled { opacity: 80%; }")
                .unwrap();
        assert_eq!(rules.0.len(), 1);
        assert_eq!(rules.0[0].selectors.len(), 2);
        assert_eq!(
            rules.0[0].selectors[0]
                .identity
                .as_ref()
                .unwrap()
                .path
                .segments
                .len(),
            3
        );
        assert_eq!(rules.0[0].selectors[1].classes.len(), 2);
        assert!(rules.0[0].selectors[1].classes[1].negated);
        for source in [
            "::widgets::Badge<'static, (u32, u64), { 2 + 2 }> {}",
            "self::Button {}",
            "super::Button {}",
            "Self {}",
        ] {
            assert!(parse(source).is_ok(), "{source}");
        }
    }

    #[test]
    fn parses_class_lexer_wildcard_and_bare_hover() {
        let rules = parse(".my-class.foo_2:hover {} * {} *.my-class {} :hover {}").unwrap();
        assert_eq!(rules.0.len(), 4);
        let selector = &rules.0[0].selectors[0];
        assert_eq!(selector.classes[0].name, "my-class");
        assert_eq!(selector.classes[1].name, "foo_2");
        assert!(selector.hovered);
        assert_eq!(selector.specificity_class_count(), 3);
        for source in [".my-class-2 {}", ".a--b- {}", ".type {}"] {
            assert!(parse(source).is_ok(), "{source}");
        }
    }

    #[test]
    fn recursively_rewrites_dollars_even_inside_opaque_macro_groups() {
        let (rewritten, count) = rewrite_dollar_references(
            "matches!($level, H2) && predicate!({ [$active, ($progress > 2)] })"
                .parse()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(count, 3);
        let rendered = rewritten.to_string();
        for field in ["level", "active", "progress"] {
            assert!(
                rendered.contains(&format!("__mue_options . {field} ()")),
                "{rendered}"
            );
        }
        assert!(rendered.contains("H2"));
        assert!(!rendered.contains('$'));
    }

    #[test]
    fn counts_every_dollar_occurrence_and_keeps_list_specificities_separate() {
        let rules = parse("Button[$a || ($b && $a)], .selected:hover {} ").unwrap();
        assert_eq!(rules.0[0].selectors[0].specificity_class_count(), 3);
        assert_eq!(rules.0[0].selectors[1].specificity_class_count(), 2);
    }

    #[test]
    fn leaves_ordinary_rust_identifiers_constants_and_strings_unchanged() {
        let rules =
            parse(r#"Button[local_enabled && matches!($level, H2) && label == "$level"] {}"#)
                .unwrap();
        let predicate = rules.0[0].selectors[0].predicate.as_ref().unwrap();
        assert_eq!(predicate.dollar_references, 1);
        let expression = predicate.expression.to_token_stream().to_string();
        assert!(expression.contains("local_enabled"));
        assert!(expression.contains("H2"));
        assert!(expression.contains(r#""$level""#));
        assert!(!expression.contains("__mue_options . local_enabled"));
        let rules = parse("Button[local_enabled] {} ").unwrap();
        let predicate = rules.0[0].selectors[0].predicate.as_ref().unwrap();
        assert_eq!(predicate.dollar_references, 0);
        assert_eq!(
            predicate.expression.to_token_stream().to_string(),
            "local_enabled"
        );
    }

    #[test]
    fn rejects_unsupported_or_misordered_selectors() {
        for invalid in [
            "#button {}",
            "Button > Label {}",
            "Button + Label {}",
            "Button ~ Label {}",
            "Button Label {}",
            "[$active] {}",
            "*[$active] {}",
            ".active[$active] {}",
            "Button:not(.active) {}",
            "Button:hover:hover {}",
            "Button[$a].active {}",
            "Button:hover[$a] {}",
            "Button[$a][$b] {}",
            "Button[] {}",
            "Button[$] {}",
            "Button[$(active)] {}",
            "Button, {}",
            "{}",
            ".2fast {}",
            ".über {}",
            ".r#type {}",
            ".foo bar {}",
            ".foo-1.2 {}",
            ".foo+bar {}",
            "|c, style| match c { _ => style }",
        ] {
            assert!(
                parse(invalid).is_err(),
                "accepted invalid selector: {invalid}"
            );
        }
    }

    #[test]
    fn rejects_children_and_hooks_but_does_not_inspect_rust_value_tokens() {
        for invalid in [
            "Button { children: { build() }; }",
            "Button { hooks: { hooks }; }",
            "Button { on-tap: { || {} }; }",
            "Button { on_tap: { || {} }; }",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        assert!(parse("Button { opacity: { value(children, hooks) }; }").is_ok());
    }

    #[test]
    fn class_macro_validates_a_single_literal_in_both_expression_contexts() {
        for valid in [r#""compact""#, r#""my-class_2""#, r##"r#"raw-name"#"##] {
            assert!(syn::parse_str::<ClassInput>(valid).is_ok(), "{valid}");
        }
        for invalid in [
            r#""""#,
            r#""2fast""#,
            r#""not valid""#,
            r#""über""#,
            r#""dot.name""#,
            r#""valid", extra"#,
            "local_name",
            "42",
        ] {
            assert!(syn::parse_str::<ClassInput>(invalid).is_err(), "{invalid}");
        }
    }
}
