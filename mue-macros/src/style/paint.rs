//! Opacity, foreground, and background paint.
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
        "opacity" => {
            let value = if let Some(value) = token(input, &quote!(f32))? {
                value
            } else if let Some(expr) = expression_with_token(input, &quote!(f32))? {
                expr
            } else {
                let (mut value, unit, at) = number(input)?;
                if unit == "%" {
                    value /= 100.;
                } else if !unit.is_empty() {
                    return Err(syn::Error::new(
                        at,
                        "opacity expects a number or percentage",
                    ));
                }
                let value = value.clamp(0., 1.);
                quote!(#value)
            };
            setter("opacity", value)
        }
        "color" | "background" | "background-color" => setter(
            if name == "color" {
                "color"
            } else {
                "background"
            },
            color(input, host)?,
        ),
        "border-color" => setter("border_color", color(input, host)?),
        "outline-color" => setter("outline_color", color(input, host)?),
        "outline-width" => setter(
            "outline_width",
            length(input, host, false, false, false, false)?,
        ),
        "outline-offset" => setter(
            "outline_offset",
            length(input, host, false, false, true, false)?,
        ),
        "border" | "outline" => line_shorthand(name, input, host)?,
        _ => return Ok(None),
    }))
}

/// `border` / `outline`: `none`, or a width, an optional `solid`, and a color in any order.
fn line_shorthand(name: &str, input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    let style = Ident::new("__style", Span::mixed_site());
    let zero = quote!(#host::style::Size::Px(0.));
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "none" {
        input.parse::<Ident>()?;
        return Ok(if name == "border" {
            quote!(#style.border((#zero,)))
        } else {
            quote!(#style.outline_width(#zero))
        });
    }
    let mut width = None;
    let mut paint = None;
    while !end(input) {
        let starts_number = input.peek(LitInt)
            || input.peek(LitFloat)
            || input.peek(Token![.])
            || input.peek(Token![-]);
        if starts_number && width.is_none() {
            width = Some(length(input, host, false, false, false, false)?);
        } else if input.peek(Ident) && input.fork().parse::<Ident>()? == "solid" {
            input.parse::<Ident>()?;
        } else if paint.is_none() {
            paint = Some(color(input, host)?);
        } else {
            return Err(input.error(format!(
                "expected `{name}: <width> [solid] <color>` or `{name}: none`"
            )));
        }
    }
    let width = width.unwrap_or_else(|| quote!(#host::style::Size::Px(1.)));
    let (width_method, color_method) = if name == "border" {
        (quote!(border), format_ident!("border_color"))
    } else {
        (quote!(outline_width), format_ident!("outline_color"))
    };
    let width = if name == "border" {
        quote!((#width,))
    } else {
        width
    };
    Ok(match paint {
        Some(paint) => quote!(#style.#width_method(#width).#color_method(#paint)),
        None => quote!(#style.#width_method(#width)),
    })
}
