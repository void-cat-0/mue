//! Box sizes, edges, gaps, and corner radii.
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
        "width" | "height" | "min-width" | "min-height" | "max-width" | "max-height"
        | "flex-basis" => setter(method, length(input, host, false, true, false, true)?),
        "padding" | "margin" | "inset" | "border-width" => {
            let (prefix, auto, negative) = match name {
                "border-width" => ("border", false, false),
                "margin" => ("margin", true, true),
                "inset" => ("inset", true, true),
                _ => ("padding", false, false),
            };
            let single_expression = input.fork();
            let expression_is_complete =
                expression(&single_expression)?.is_some() && end(&single_expression);
            if expression_is_complete {
                let expr = expression(input)?.expect("expression was parsed above");
                let method = if prefix == "border" { "border" } else { prefix };
                setter(method, expr)
            } else {
                let mut values = Vec::new();
                while !end(input) {
                    values.push(length(
                        input,
                        host,
                        false,
                        auto,
                        negative,
                        name != "border-width",
                    )?);
                    if values.len() > 4 {
                        return Err(input.error("expected one to four style lengths"));
                    }
                }
                rect(prefix, values, host, span)?
            }
        }
        "top" | "right" | "bottom" | "left" => setter(
            &format!("inset_{name}"),
            length(input, host, false, true, true, true)?,
        ),
        "padding-top"
        | "padding-right"
        | "padding-bottom"
        | "padding-left"
        | "margin-top"
        | "margin-right"
        | "margin-bottom"
        | "margin-left"
        | "border-top-width"
        | "border-right-width"
        | "border-bottom-width"
        | "border-left-width" => {
            let margin = name.starts_with("margin");
            let method = method.strip_suffix("_width").unwrap_or(method);
            setter(
                method,
                length(
                    input,
                    host,
                    false,
                    margin,
                    margin,
                    !name.starts_with("border"),
                )?,
            )
        }
        "gap" => {
            let row = length(input, host, false, false, false, true)?;
            let column = if end(input) {
                None
            } else {
                Some(length(input, host, false, false, false, true)?)
            };
            match column {
                Some(column) => quote!(#style.row_gap(#row).column_gap(#column)),
                None => setter("gap", row),
            }
        }
        "row-gap" | "column-gap" => setter(method, length(input, host, false, false, false, true)?),
        "border-radius" => setter("radius", length(input, host, false, false, false, false)?),
        _ => return Ok(None),
    }))
}
