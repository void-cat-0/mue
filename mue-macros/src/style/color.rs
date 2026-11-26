//! Color and brush literals.
use super::*;

pub(super) fn color(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if let Some(value) = token(input, &quote!(#host::paint::shader::Brush))? {
        return Ok(value);
    }
    if let Some(expr) = expression_with_token(input, &quote!(#host::paint::shader::Brush))? {
        return Ok(expr);
    }
    if input.peek(Token![#]) || input.peek(LitStr) {
        let (digits, span) = if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            (
                lit.value().strip_prefix('#').unwrap_or("").to_owned(),
                lit.span(),
            )
        } else {
            input.parse::<Token![#]>()?;
            let token: TokenTree = input.parse()?;
            (token.to_string(), token.span())
        };
        if !matches!(digits.len(), 3 | 4 | 6 | 8) || !digits.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(syn::Error::new(
                span,
                "expected #rgb, #rgba, #rrggbb, or #rrggbbaa",
            ));
        }
        let digits = if digits.len() <= 4 {
            digits.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            digits
        };
        let byte = |i| u8::from_str_radix(&digits[i..i + 2], 16).unwrap();
        let (r, g, b, a) = (
            byte(0),
            byte(2),
            byte(4),
            if digits.len() == 8 { byte(6) } else { 255 },
        );
        return Ok(quote!(#host::paint::color::Color::from_rgba(#r,#g,#b,#a)));
    }
    let (name, span) = property(input)?;
    if matches!(name.as_str(), "rgb" | "rgba") {
        let args;
        parenthesized!(args in input);
        let mut channels = Vec::new();
        for i in 0..3 {
            let (value, unit, at) = number(&args)?;
            let value = match unit.as_str() {
                "" => value / 255.,
                "%" => value / 100.,
                _ => return Err(syn::Error::new(at, "expected color channel or percentage")),
            }
            .clamp(0., 1.);
            channels.push(value);
            if i < 2 && args.peek(Token![,]) {
                args.parse::<Token![,]>()?;
            }
        }
        let mut alpha = 1f32;
        if !args.is_empty() {
            if args.peek(Token![,]) {
                args.parse::<Token![,]>()?;
            } else if args.peek(Token![/]) {
                args.parse::<Token![/]>()?;
            }
            let (value, unit, at) = number(&args)?;
            alpha = match unit.as_str() {
                "" => value,
                "%" => value / 100.,
                _ => return Err(syn::Error::new(at, "expected alpha number or percentage")),
            }
            .clamp(0., 1.);
        }
        if !args.is_empty() {
            return Err(args.error("unexpected color argument"));
        }
        let (r, g, b) = (channels[0], channels[1], channels[2]);
        return Ok(quote!(#host::paint::color::Color::new(#r,#g,#b,#alpha)));
    }
    let (r, g, b, a) = match name.as_str() {
        "transparent" => (0, 0, 0, 0),
        "black" => (0, 0, 0, 255),
        "white" => (255, 255, 255, 255),
        "red" => (255, 0, 0, 255),
        "lime" => (0, 255, 0, 255),
        "blue" => (0, 0, 255, 255),
        "yellow" => (255, 255, 0, 255),
        "cyan" | "aqua" => (0, 255, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255, 255),
        "gray" | "grey" => (128, 128, 128, 255),
        "silver" => (192, 192, 192, 255),
        "maroon" => (128, 0, 0, 255),
        "olive" => (128, 128, 0, 255),
        "green" => (0, 128, 0, 255),
        "purple" => (128, 0, 128, 255),
        "teal" => (0, 128, 128, 255),
        "navy" => (0, 0, 128, 255),
        "orange" => (255, 165, 0, 255),
        "rebeccapurple" => (102, 51, 153, 255),
        _ => {
            return Err(syn::Error::new(
                span,
                "unsupported color; use hex, rgb()/rgba(), a basic style color name, or {Rust expression}",
            ));
        }
    };
    let (r, g, b, a) = (r as u8, g as u8, b as u8, a as u8);
    Ok(quote!(#host::paint::color::Color::from_rgba(#r,#g,#b,#a)))
}
