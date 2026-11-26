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
        _ => return Ok(None),
    }))
}
