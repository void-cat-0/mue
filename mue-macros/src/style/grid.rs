//! Grid container templates and grid item placement.
use super::*;

pub(super) fn declaration(
    name: &str,
    span: Span,
    _method: &str,
    input: ParseStream,
    host: &TokenStream,
) -> Result<Option<TokenStream>> {
    let _ = span;
    Ok(Some(match name {
        "grid-template-columns" | "grid-template-rows" => {
            let method = name.replace('-', "_");
            let ty = quote!(#host::style::GridTemplate);
            let value = if let Some(value) = token(input, &ty)? {
                value
            } else if let Some(expr) = expression_with_token(input, &ty)? {
                expr
            } else {
                template(input, host)?
            };
            setter(&method, value)
        }
        "grid-auto-columns" | "grid-auto-rows" => {
            let method = name.replace('-', "_");
            let ty = quote!(#host::style::GridTracks);
            let value = if let Some(value) = token(input, &ty)? {
                value
            } else if let Some(expr) = expression_with_token(input, &ty)? {
                expr
            } else {
                let mut tracks = Vec::new();
                while !end(input) {
                    tracks.push(track(input, host)?);
                }
                if tracks.is_empty() {
                    return Err(input.error("expected at least one track size"));
                }
                quote!(#host::style::GridTracks(::std::vec![#(#tracks),*]))
            };
            setter(&method, value)
        }
        "grid-auto-flow" => {
            let ty = quote!(#host::style::GridAutoFlow);
            let value = if let Some(value) = token(input, &ty)? {
                value
            } else if let Some(expr) = expression_with_token(input, &ty)? {
                expr
            } else {
                let mut column = false;
                let mut dense = false;
                while !end(input) {
                    match keyword(input, &["row", "column", "dense"])?.as_str() {
                        "column" => column = true,
                        "dense" => dense = true,
                        _ => {}
                    }
                }
                let variant = format_ident!(
                    "{}",
                    match (column, dense) {
                        (false, false) => "Row",
                        (true, false) => "Column",
                        (false, true) => "RowDense",
                        (true, true) => "ColumnDense",
                    }
                );
                quote!(#host::style::GridAutoFlow::#variant)
            };
            setter("grid_auto_flow", value)
        }
        "grid-column" | "grid-row" => {
            let method = name.replace('-', "_");
            let ty = quote!(#host::style::GridLine);
            let value = if let Some(value) = token(input, &ty)? {
                value
            } else if let Some(expr) = expression_with_token(input, &ty)? {
                expr
            } else {
                let start = placement(input, host)?;
                let end = if input.peek(Token![/]) {
                    input.parse::<Token![/]>()?;
                    placement(input, host)?
                } else {
                    quote!(#host::style::GridPlacement::Auto)
                };
                quote!(#host::style::GridLine { start: #start, end: #end })
            };
            setter(&method, value)
        }
        _ => return Ok(None),
    }))
}

/// A track list with optional `repeat(count | auto-fill | auto-fit, tracks...)` groups.
fn template(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    let mut items = Vec::new();
    while !end(input) {
        if input.peek(Ident) && input.fork().parse::<Ident>()? == "repeat" {
            input.parse::<Ident>()?;
            let args;
            parenthesized!(args in input);
            let count = if args.peek(Ident) {
                match keyword(&args, &["auto-fill", "auto-fit"])?.as_str() {
                    "auto-fill" => quote!(#host::style::Repeat::AutoFill),
                    _ => quote!(#host::style::Repeat::AutoFit),
                }
            } else {
                let lit: LitInt = args.parse()?;
                let count: u16 = lit.base10_parse()?;
                if count == 0 {
                    return Err(syn::Error::new(lit.span(), "repeat count must be positive"));
                }
                quote!(#host::style::Repeat::Count(#count))
            };
            args.parse::<Token![,]>()?;
            let mut tracks = Vec::new();
            while !args.is_empty() {
                tracks.push(track(&args, host)?);
                comma(&args)?;
            }
            if tracks.is_empty() {
                return Err(args.error("repeat() needs at least one track"));
            }
            items.push(quote!(.repeat(#count, [#(#tracks),*])));
        } else {
            let track = track(input, host)?;
            items.push(quote!(.track(#track)));
        }
    }
    if items.is_empty() {
        return Err(input.error("expected track sizes or repeat(...)"));
    }
    Ok(quote!(#host::style::GridTemplate::new() #(#items)*))
}

/// One track size: a length, percentage, `fr`, `auto`, `min-content`,
/// `max-content`, `fit-content(px)`, or `minmax(min, max)`.
fn track(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    let ty = quote!(#host::style::GridTrack);
    if let Some(expr) = expression(input)? {
        return Ok(quote!({ let __track: #ty = #expr; __track }));
    }
    if input.peek(Ident) {
        let fork = input.fork();
        let name = property(&fork)?.0;
        if name == "minmax" || name == "fit-content" {
            property(input)?;
            let args;
            parenthesized!(args in input);
            if name == "fit-content" {
                let limit = pixel_number(&args)?;
                return Ok(quote!(#ty::fit_content(#limit)));
            }
            let min = track(&args, host)?;
            args.parse::<Token![,]>()?;
            let max = track(&args, host)?;
            if !args.is_empty() {
                return Err(args.error("minmax() takes two tracks"));
            }
            return Ok(quote!(#ty::minmax(#min, #max)));
        }
        return Ok(
            match keyword(input, &["auto", "min-content", "max-content"])?.as_str() {
                "auto" => quote!(#ty::auto()),
                "min-content" => quote!(#ty::min_content()),
                _ => quote!(#ty::max_content()),
            },
        );
    }
    let (value, unit, at) = number(input)?;
    if value < 0. {
        return Err(syn::Error::new(at, "track sizes must be non-negative"));
    }
    match unit.as_str() {
        "fr" => Ok(quote!(#ty::fr(#value))),
        "px" => Ok(quote!(#ty::px(#value))),
        "%" => Ok(quote!(#ty::percent(#value))),
        "" if value == 0. => Ok(quote!(#ty::px(0.))),
        _ => Err(syn::Error::new(
            at,
            "unsupported track unit; use px, %, fr, auto, min-content, max-content, fit-content(), or minmax()",
        )),
    }
}

/// `auto`, a line number (negative counts from the end), or `span n`.
fn placement(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if input.peek(Ident) {
        return match keyword(input, &["auto", "span"])?.as_str() {
            "auto" => Ok(quote!(#host::style::GridPlacement::Auto)),
            _ => {
                let (value, unit, at) = number(input)?;
                if !unit.is_empty() || value < 1. || value.fract() != 0. || value > u16::MAX as f32
                {
                    return Err(syn::Error::new(at, "span expects a positive integer"));
                }
                let span = value as u16;
                Ok(quote!(#host::style::GridPlacement::Span(#span)))
            }
        };
    }
    let (value, unit, at) = number(input)?;
    if !unit.is_empty() || value == 0. || value.fract() != 0. || value.abs() > i16::MAX as f32 {
        return Err(syn::Error::new(
            at,
            "grid lines are non-zero integers; negative lines count from the end",
        ));
    }
    let line = value as i16;
    Ok(quote!(#host::style::GridPlacement::Line(#line)))
}
