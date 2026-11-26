//! Display, positioning, flex, alignment, and overflow.
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
        "display" => enumeration(
            input,
            host,
            method,
            "Display",
            &[
                ("flex", "Flex"),
                ("grid", "Grid"),
                ("block", "Block"),
                ("none", "None"),
            ],
            false,
        )?,
        "box-sizing" => enumeration(
            input,
            host,
            method,
            "BoxSizing",
            &[("border-box", "BorderBox"), ("content-box", "ContentBox")],
            false,
        )?,
        "position" => enumeration(
            input,
            host,
            method,
            "Position",
            &[("relative", "Relative"), ("absolute", "Absolute")],
            false,
        )?,
        "flex-direction" => enumeration(
            input,
            host,
            method,
            "FlexDirection",
            &[
                ("row", "Row"),
                ("column", "Column"),
                ("row-reverse", "RowReverse"),
                ("column-reverse", "ColumnReverse"),
            ],
            false,
        )?,
        "flex-wrap" => enumeration(
            input,
            host,
            method,
            "FlexWrap",
            &[
                ("nowrap", "NoWrap"),
                ("wrap", "Wrap"),
                ("wrap-reverse", "WrapReverse"),
            ],
            false,
        )?,
        "align-items" | "align-self" | "justify-items" | "justify-self" => {
            let mut values = vec![
                ("start", "Start"),
                ("end", "End"),
                ("flex-start", "FlexStart"),
                ("flex-end", "FlexEnd"),
                ("center", "Center"),
                ("baseline", "Baseline"),
                ("stretch", "Stretch"),
                ("normal", "None"),
            ];
            if name.ends_with("-self") {
                values.push(("auto", "None"));
            }
            enumeration(input, host, method, "AlignItems", &values, true)?
        }
        "align-content" | "justify-content" => enumeration(
            input,
            host,
            method,
            "AlignContent",
            &[
                ("start", "Start"),
                ("end", "End"),
                ("flex-start", "FlexStart"),
                ("flex-end", "FlexEnd"),
                ("center", "Center"),
                ("stretch", "Stretch"),
                ("space-between", "SpaceBetween"),
                ("space-around", "SpaceAround"),
                ("space-evenly", "SpaceEvenly"),
                ("normal", "None"),
            ],
            true,
        )?,
        "overflow" => {
            let x = enum_value(
                input,
                host,
                "Overflow",
                &[
                    ("visible", "Visible"),
                    ("hidden", "Hidden"),
                    ("clip", "Clip"),
                    ("scroll", "Scroll"),
                ],
                false,
            )?;
            if end(input) {
                let value = Ident::new("__overflow", Span::mixed_site());
                quote!({
                    let #value: #host::__private::core::prop::Prop<#host::style::values::Overflow> = (#x).into();
                    #style.overflow_x(#value.clone()).overflow_y(#value)
                })
            } else {
                let y = enum_value(
                    input,
                    host,
                    "Overflow",
                    &[
                        ("visible", "Visible"),
                        ("hidden", "Hidden"),
                        ("clip", "Clip"),
                        ("scroll", "Scroll"),
                    ],
                    false,
                )?;
                quote!(#style.overflow_x(#x).overflow_y(#y))
            }
        }
        "overflow-x" | "overflow-y" => enumeration(
            input,
            host,
            method,
            "Overflow",
            &[
                ("visible", "Visible"),
                ("hidden", "Hidden"),
                ("clip", "Clip"),
                ("scroll", "Scroll"),
            ],
            false,
        )?,
        "flex-grow" | "flex-shrink" => setter(method, scalar(input, &quote!(f32), false)?),
        "flex" => flex(input, host)?,
        "z-index" => setter("z_index", {
            if let Some(value) = token(input, &quote!(i32))? {
                value
            } else if let Some(expr) = expression_with_token(input, &quote!(i32))? {
                expr
            } else {
                let (value, unit, at) = number(input)?;
                if !unit.is_empty() || value.fract() != 0. {
                    return Err(syn::Error::new(at, "z-index expects an integer"));
                }
                let value = value as i32;
                quote!(#value)
            }
        }),
        "aspect-ratio" => {
            let value = if let Some(value) = token(input, &quote!(Option<f32>))? {
                value
            } else if let Some(expr) = expression_with_token(input, &quote!(Option<f32>))? {
                expr
            } else if input.peek(Ident) {
                keyword(input, &["auto"])?;
                quote!(None)
            } else {
                let numerator = numeric(input, false)?;
                let denominator = if input.peek(Token![/]) {
                    input.parse::<Token![/]>()?;
                    numeric(input, false)?
                } else {
                    1.
                };
                if numerator <= 0. || denominator <= 0. {
                    return Err(syn::Error::new(span, "aspect-ratio must be positive"));
                }
                let ratio = numerator / denominator;
                quote!(Some(#ratio))
            };
            setter("aspect_ratio", value)
        }
        _ => return Ok(None),
    }))
}

pub(super) fn flex(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    let style = Ident::new("__style", Span::mixed_site());
    if input.peek(Ident) {
        let name = keyword(input, &["none", "auto", "initial"])?;
        let (grow, shrink) = match name.as_str() {
            "none" => (0f32, 0f32),
            "auto" => (1., 1.),
            _ => (0., 1.),
        };
        return Ok(
            quote!(#style.flex_grow(#grow).flex_shrink(#shrink).flex_basis(#host::style::Size::Auto)),
        );
    }
    let fork = input.fork();
    let (_, unit, _) = number(&fork)?;
    if !unit.is_empty() {
        let basis = length(input, host, false, true, false, true)?;
        return Ok(quote!(#style.flex_grow(1f32).flex_shrink(1f32).flex_basis(#basis)));
    }
    let grow = numeric(input, false)?;
    let mut shrink = 1f32;
    if !end(input) && !input.peek(Ident) && !input.peek(syn::token::Brace) {
        let fork = input.fork();
        let (_, unit, _) = number(&fork)?;
        if unit.is_empty() {
            shrink = numeric(input, false)?;
        }
    }
    let basis = if end(input) {
        quote!(#host::style::Size::Percent(0.))
    } else {
        length(input, host, false, true, false, true)?
    };
    Ok(quote!(#style.flex_grow(#grow).flex_shrink(#shrink).flex_basis(#basis)))
}
