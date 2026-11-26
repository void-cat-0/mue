//! Font and text declarations.
use super::*;

pub(super) fn declaration(
    name: &str,
    span: Span,
    method: &str,
    input: ParseStream,
    host: &TokenStream,
) -> Result<Option<TokenStream>> {
    let style = Ident::new("__style", Span::mixed_site());
    let _ = (&style, span, method);
    Ok(Some(match name {
        "font-size" => setter("font_size", length(input, host, true, false, false, true)?),
        "line-height" => {
            if let Some(value) = token(input, &quote!(#host::style::Size))? {
                quote!(#style.line_height(#value).line_height_multiplier(None))
            } else if let Some(expr) = expression_with_token(input, &quote!(#host::style::Size))? {
                quote!(#style.line_height(#expr).line_height_multiplier(None))
            } else if input.peek(Ident) {
                keyword(input, &["normal"])?;
                setter("line_height_multiplier", quote!(Some(1.2f32)))
            } else {
                let (value, unit, at) = number(input)?;
                if value < 0. {
                    return Err(syn::Error::new(at, "line-height must be non-negative"));
                }
                if unit.is_empty() || unit == "%" || unit == "em" {
                    let value = if unit == "%" { value / 100. } else { value };
                    setter("line_height_multiplier", quote!(Some(#value)))
                } else {
                    let value = length_value(value, &unit, at, host, false, false, false)?;
                    quote!(#style.line_height(#value).line_height_multiplier(None))
                }
            }
        }
        "letter-spacing" => setter(
            "letter_spacing",
            length(input, host, false, false, true, false)?,
        ),
        "paragraph-spacing" => setter(
            "paragraph_spacing",
            length(input, host, false, false, false, true)?,
        ),
        "font-weight" => {
            let value = if let Some(value) = token(input, &quote!(#host::font::Weight))? {
                value
            } else if let Some(expr) = expression_with_token(input, &quote!(#host::font::Weight))? {
                expr
            } else {
                let value = if input.peek(Ident) {
                    match keyword(input, &["normal", "bold"])?.as_str() {
                        "bold" => 700.,
                        _ => 400.,
                    }
                } else {
                    numeric(input, false)?
                };
                if !(1. ..=1000.).contains(&value) || value.fract() != 0. {
                    return Err(syn::Error::new(
                        span,
                        "font-weight must be an integer from 1 to 1000",
                    ));
                }
                let value = value as u16;
                quote!(#host::font::Weight(#value))
            };
            setter("font_weight", value)
        }
        "font-family" => {
            let value = if let Some(value) = token(input, &quote!(#host::font::FamilyOwned))? {
                value
            } else if let Some(expr) =
                expression_with_token(input, &quote!(#host::font::FamilyOwned))?
            {
                expr
            } else if input.peek(LitStr) {
                let name: LitStr = input.parse()?;
                quote!(#host::font::FamilyOwned::Name(#name.into()))
            } else {
                let name = keyword(
                    input,
                    &["sans-serif", "serif", "monospace", "cursive", "fantasy"],
                )?;
                let variant = format_ident!(
                    "{}",
                    match name.as_str() {
                        "sans-serif" => "SansSerif",
                        "serif" => "Serif",
                        "monospace" => "Monospace",
                        "cursive" => "Cursive",
                        _ => "Fantasy",
                    }
                );
                quote!(#host::font::FamilyOwned::#variant)
            };
            setter("font_family", value)
        }
        "font-style" => {
            let value = if let Some(value) = token(input, &quote!(bool))? {
                value
            } else if let Some(expr) = expression_with_token(input, &quote!(bool))? {
                expr
            } else {
                let italic = keyword(input, &["normal", "italic"])? == "italic";
                quote!(#italic)
            };
            setter("font_italic", value)
        }
        "font-italic"
        | "text-wrap"
        | "text-underline"
        | "text-strikethrough"
        | "text-max-lines"
        | "text-vertical-align"
        | "text-wrap-mode" => {
            let method = match name {
                "font-italic" => "font_italic",
                "text-wrap" => "text_wrap",
                "text-underline" => "text_underline",
                "text-strikethrough" => "text_strikethrough",
                "text-max-lines" => "text_max_lines",
                "text-vertical-align" => "text_vertical_align",
                _ => "text_wrap_mode",
            };
            let ty = match name {
                "font-italic" | "text-wrap" | "text-underline" | "text-strikethrough" => {
                    quote!(bool)
                }
                "text-max-lines" => quote!(Option<usize>),
                "text-vertical-align" => quote!(#host::node::text::TextVerticalAlign),
                _ => quote!(#host::node::text::TextWrap),
            };
            let value = if let Some(value) = token(input, &ty)? {
                value
            } else {
                expression_with_token(input, &ty)?.ok_or_else(|| {
                    syn::Error::new(
                        span,
                        "this style property expects a Rust expression in braces",
                    )
                })?
            };
            setter(method, value)
        }
        "tab-width" => setter("tab_width", scalar(input, &quote!(u16), false)?),
        "text-align" => {
            let value = if let Some(value) =
                token(input, &quote!(Option<#host::style::values::TextAlign>))?
            {
                value
            } else if let Some(expr) =
                expression_with_token(input, &quote!(Option<#host::style::values::TextAlign>))?
            {
                expr
            } else {
                let name = keyword(input, &["left", "right", "center", "justify"])?;
                let name = format_ident!(
                    "{}",
                    match name.as_str() {
                        "left" => "Left",
                        "right" => "Right",
                        "center" => "Center",
                        _ => "Justified",
                    }
                );
                quote!(#host::__private::core::prop::Prop::Static(Some(#host::style::values::TextAlign::#name)))
            };
            setter("text_align", value)
        }
        "white-space" => {
            let value = if let Some(value) = token(input, &quote!(bool))? {
                value
            } else {
                let name = keyword(input, &["normal", "nowrap", "pre", "pre-wrap"])?;
                let wrap = matches!(name.as_str(), "normal" | "pre-wrap");
                quote!(#wrap)
            };
            setter("text_wrap", value)
        }
        "text-decoration" | "text-decoration-line" => {
            let (mut underline, mut strike) = (false, false);
            let first = keyword(input, &["none", "underline", "line-through"])?;
            if first != "none" {
                underline = first == "underline";
                strike = first == "line-through";
                while !end(input) {
                    let next = keyword(input, &["underline", "line-through"])?;
                    underline |= next == "underline";
                    strike |= next == "line-through";
                }
            }
            quote!(#style.text_underline(#underline).text_strikethrough(#strike))
        }
        "text-overflow" => {
            if let Some(value) = token(input, &quote!(#host::node::text::TextOverflow))? {
                setter("text_overflow", value)
            } else if let Some(expr) =
                expression_with_token(input, &quote!(#host::node::text::TextOverflow))?
            {
                setter("text_overflow", expr)
            } else {
                let name = keyword(input, &["clip", "ellipsis"])?;
                let variant = format_ident!(
                    "{}",
                    if name == "clip" {
                        "Clip"
                    } else {
                        "EllipsisEnd"
                    }
                );
                setter(
                    "text_overflow",
                    quote!(#host::node::text::TextOverflow::#variant),
                )
            }
        }
        _ => return Ok(None),
    }))
}
