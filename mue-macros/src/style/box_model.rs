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
        "border-radius" => {
            let single_expression = input.fork();
            if expression(&single_expression)?.is_some() && end(&single_expression) {
                let expr = expression(input)?.expect("expression was parsed above");
                setter("radius", expr)
            } else {
                let mut values = Vec::new();
                while !end(input) {
                    values.push(length(input, host, false, false, false, false)?);
                    if values.len() > 4 {
                        return Err(input.error("expected one to four corner radii"));
                    }
                }
                corners(values, host, span)?
            }
        }
        "border-top-left-radius"
        | "border-top-right-radius"
        | "border-bottom-right-radius"
        | "border-bottom-left-radius" => {
            let corner = name
                .strip_prefix("border-")
                .and_then(|name| name.strip_suffix("-radius"))
                .expect("matched corner property")
                .replace('-', "_");
            setter(
                &format!("radius_{corner}"),
                length(input, host, false, false, false, false)?,
            )
        }
        _ => return Ok(None),
    }))
}

/// Expand one to four `border-radius` values clockwise from the top-left corner.
fn corners(values: Vec<TokenStream>, host: &TokenStream, span: Span) -> Result<TokenStream> {
    let style = Ident::new("__style", Span::mixed_site());
    let indices = match values.len() {
        1 => [0, 0, 0, 0],
        2 => [0, 1, 0, 1],
        3 => [0, 1, 2, 1],
        4 => [0, 1, 2, 3],
        _ => return Err(syn::Error::new(span, "expected one to four corner radii")),
    };
    let names: Vec<_> = (0..values.len())
        .map(|i| Ident::new(&format!("__radius_{i}"), Span::mixed_site()))
        .collect();
    let vars = indices.map(|i| names[i].clone());
    let methods = ["top_left", "top_right", "bottom_right", "bottom_left"]
        .map(|corner| format_ident!("radius_{corner}"));
    Ok(quote!({
        #(let #names: #host::__private::core::prop::Prop<#host::style::Size> = (#values).into();)*
        #style #(.#methods(#vars.clone()))*
    }))
}
