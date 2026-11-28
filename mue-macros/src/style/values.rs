//! Shared value parsers: numbers, lengths, keywords, and enumerations.
use super::*;

pub(super) fn number(input: ParseStream) -> Result<(f32, String, Span)> {
    let negative = if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        true
    } else {
        if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
        }
        false
    };
    let (mut value, mut unit, span) = if input.peek(Token![.]) {
        let dot: Token![.] = input.parse()?;
        let lit: LitInt = input.parse()?;
        let digits = lit.base10_digits().replace('_', "");
        let value = format!("0.{digits}")
            .parse::<f32>()
            .map_err(|_| syn::Error::new(lit.span(), "invalid decimal"))?;
        (value, lit.suffix().to_owned(), dot.spans[0])
    } else if input.peek(LitInt) {
        let lit: LitInt = input.parse()?;
        (
            lit.base10_parse::<f32>()?,
            lit.suffix().to_owned(),
            lit.span(),
        )
    } else if input.peek(LitFloat) {
        let lit: LitFloat = input.parse()?;
        (
            lit.base10_parse::<f32>()?,
            lit.suffix().to_owned(),
            lit.span(),
        )
    } else {
        return Err(input.error("expected a number, style length, or {Rust expression}"));
    };
    if unit.is_empty() && input.peek(Ident) {
        let next = input.fork().parse::<Ident>()?.to_string();
        if [
            "px", "em", "rem", "vw", "vh", "deg", "rad", "turn", "grad", "ms", "s",
        ]
        .contains(&next.as_str())
        {
            unit = input.parse::<Ident>()?.to_string();
        }
    }
    if input.peek(Token![%]) {
        input.parse::<Token![%]>()?;
        if !unit.is_empty() {
            return Err(input.error("invalid percentage"));
        }
        unit = "%".into();
    }
    if negative {
        value = -value;
    }
    if !value.is_finite() {
        return Err(syn::Error::new(span, "value must be finite"));
    }
    Ok((value, unit, span))
}

pub(super) fn numeric(input: ParseStream, negative: bool) -> Result<f32> {
    let (value, unit, span) = number(input)?;
    if !unit.is_empty() || (!negative && value < 0.) {
        return Err(syn::Error::new(
            span,
            "expected a non-negative unitless number",
        ));
    }
    Ok(value)
}

pub(super) fn scalar(input: ParseStream, ty: &TokenStream, negative: bool) -> Result<TokenStream> {
    if let Some(value) = token(input, ty)? {
        return Ok(value);
    }
    if let Some(expr) = expression_with_token(input, ty)? {
        return Ok(expr);
    }
    let value = numeric(input, negative)?;
    Ok(quote!(#value))
}

pub(super) fn length(
    input: ParseStream,
    host: &TokenStream,
    font: bool,
    auto: bool,
    negative: bool,
    percent: bool,
) -> Result<TokenStream> {
    let ty = if font {
        quote!(#host::style::FontSize)
    } else {
        quote!(#host::style::Size)
    };
    if let Some(value) = token(input, &ty)? {
        return Ok(value);
    }
    if let Some(expr) = expression_with_token(input, &ty)? {
        return Ok(expr);
    }
    if input.peek(Ident) {
        keyword(input, if auto { &["auto"] } else { &[] })?;
        return Ok(quote!(#host::style::Size::Auto));
    }
    let (value, unit, span) = number(input)?;
    length_value(value, &unit, span, host, font, negative, percent)
}

pub(super) fn length_value(
    value: f32,
    unit: &str,
    span: Span,
    host: &TokenStream,
    font: bool,
    negative: bool,
    percent: bool,
) -> Result<TokenStream> {
    if !negative && value < 0. {
        return Err(syn::Error::new(
            span,
            "negative lengths are not supported for this property",
        ));
    }
    let variant = match unit {
        "px" => "Px",
        "%" if percent => "Percent",
        "vw" => "Vw",
        "vh" => "Vh",
        "em" if font => "Em",
        "rem" => "Rem",
        "" if value == 0. => "Px",
        _ => {
            return Err(syn::Error::new(
                span,
                "unsupported length unit for this property; use px, %, vw, vh, rem (also em for font-size), or {Rust expression}",
            ));
        }
    };
    let variant = format_ident!("{variant}");
    let ty = format_ident!("{}", if font { "FontSize" } else { "Size" });
    Ok(quote!(#host::style::#ty::#variant(#value)))
}

pub(super) fn rect(
    prefix: &str,
    values: Vec<TokenStream>,
    host: &TokenStream,
    span: Span,
) -> Result<TokenStream> {
    let style = Ident::new("__style", Span::mixed_site());
    let indices = match values.len() {
        1 => [0, 0, 0, 0],
        2 => [0, 1, 0, 1],
        3 => [0, 1, 2, 1],
        4 => [0, 1, 2, 3],
        _ => return Err(syn::Error::new(span, "expected one to four style lengths")),
    };
    let names: Vec<_> = (0..values.len())
        .map(|i| Ident::new(&format!("__value_{i}"), Span::mixed_site()))
        .collect();
    let vars = indices.map(|i| names[i].clone());
    let methods = ["top", "right", "bottom", "left"].map(|side| format_ident!("{prefix}_{side}"));
    Ok(quote!({
        #(let #names: #host::__private::core::prop::Prop<#host::style::Size> = (#values).into();)*
        #style #(.#methods(#vars.clone()))*
    }))
}

pub(super) fn keyword(input: ParseStream, options: &[&str]) -> Result<String> {
    let (name, span) = property(input)?;
    if options.contains(&name.as_str()) {
        Ok(name)
    } else {
        Err(syn::Error::new(
            span,
            format!(
                "unsupported value `{name}`; expected {}",
                options.join(", ")
            ),
        ))
    }
}

pub(super) fn enum_value(
    input: ParseStream,
    host: &TokenStream,
    ty: &str,
    values: &[(&str, &str)],
    optional: bool,
) -> Result<TokenStream> {
    let value_ty = format_ident!("{ty}");
    let token_ty = if optional {
        quote!(Option<#host::style::values::#value_ty>)
    } else {
        quote!(#host::style::values::#value_ty)
    };
    if let Some(value) = token(input, &token_ty)? {
        return Ok(value);
    }
    if let Some(expr) = expression_with_token(input, &token_ty)? {
        return Ok(expr);
    }
    let (name, span) = property(input)?;
    let variant = values
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
        .ok_or_else(|| syn::Error::new(span, format!("unsupported `{ty}` value `{name}`")))?;
    if optional && variant == "None" {
        return Ok(quote!(None));
    }
    let ty = format_ident!("{ty}");
    let variant = format_ident!("{variant}");
    let value = quote!(#host::style::values::#ty::#variant);
    Ok(if optional {
        quote!(Some(#value))
    } else {
        value
    })
}

pub(super) fn enumeration(
    input: ParseStream,
    host: &TokenStream,
    method: &str,
    ty: &str,
    values: &[(&str, &str)],
    optional: bool,
) -> Result<TokenStream> {
    Ok(setter(
        method,
        enum_value(input, host, ty, values, optional)?,
    ))
}

pub(super) fn pixel_number(input: ParseStream) -> Result<f32> {
    let (value, unit, span) = number(input)?;
    if !unit.is_empty() && unit != "px" {
        return Err(syn::Error::new(span, "expected a pixel distance"));
    }
    Ok(value)
}

pub(super) fn comma(input: ParseStream) -> Result<()> {
    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }
    Ok(())
}

pub(super) fn angle(input: ParseStream) -> Result<TokenStream> {
    let (value, unit, span) = number(input)?;
    // Emit conversion expressions so generated radians do not trigger Clippy's
    // approximate-constant lint in the consuming crate.
    match unit.as_str() {
        "deg" => Ok(quote!((#value).to_radians())),
        "rad" => Ok(quote!(#value)),
        "turn" => Ok(quote!((#value) * ::std::f32::consts::TAU)),
        "grad" => Ok(quote!((#value) * ::std::f32::consts::PI / 200f32)),
        "" if value == 0. => Ok(quote!(0f32)),
        _ => Err(syn::Error::new(span, "expected deg, rad, turn, or grad")),
    }
}
