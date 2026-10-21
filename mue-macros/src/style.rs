//! style declarations lowered to the renderer's typed, reactive style properties.
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{
    Ident, LitFloat, LitInt, LitStr, Result, Token, braced,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
};

pub struct Declarations(Vec<TokenStream>);

impl Parse for Declarations {
    fn parse(input: ParseStream) -> Result<Self> {
        let host = crate::paths::host()?;
        parse_declarations(input, &host)
    }
}

impl Declarations {
    pub fn expand(self) -> Result<TokenStream> {
        let host = crate::paths::host()?;
        let style = Ident::new("__style", Span::mixed_site());
        let statements = self.0;
        Ok(quote!({
            use #host::style::{Styleable as _, StyleableExt as _};
            let mut #style = #host::style::Style::new();
            #(#style = #statements;)*
            #style
        }))
    }
}

fn property(input: ParseStream) -> Result<(String, Span)> {
    let first = Ident::parse_any(input)?;
    let mut name = first.to_string().replace('_', "-");
    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        name.push('-');
        name.push_str(&Ident::parse_any(input)?.to_string());
    }
    Ok((name, first.span()))
}

fn expression(input: ParseStream) -> Result<Option<TokenStream>> {
    if !input.peek(syn::token::Brace) {
        return Ok(None);
    }
    let content;
    braced!(content in input);
    let value: syn::Expr = content.parse()?;
    if !content.is_empty() {
        return Err(content.error("expected one Rust expression"));
    }
    Ok(Some(quote!(#value)))
}

fn setter(method: &str, value: TokenStream) -> TokenStream {
    let style = Ident::new("__style", Span::mixed_site());
    let method = format_ident!("{method}");
    quote!(#style.#method(#value))
}

fn end(input: ParseStream) -> bool {
    input.is_empty() || input.peek(Token![;])
}

fn parse_declarations(input: ParseStream, host: &TokenStream) -> Result<Declarations> {
    let style = Ident::new("__style", Span::mixed_site());
    let mut result = Vec::new();
    while !input.is_empty() {
        let (name, span) = property(input)?;
        input.parse::<Token![:]>()?;
        let method = name.replace('-', "_");
        let statement = match name.as_str() {
            "width" | "height" | "min-width" | "min-height" | "max-width" | "max-height"
            | "flex-basis" => setter(&method, length(input, host, false, true, false, true)?),
            "padding" | "margin" | "inset" | "border-width" => {
                let (prefix, auto, negative) = match name.as_str() {
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
                let method = method.strip_suffix("_width").unwrap_or(&method);
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
            "row-gap" | "column-gap" => {
                setter(&method, length(input, host, false, false, false, true)?)
            }
            "border-radius" => setter("radius", length(input, host, false, false, false, false)?),
            "font-size" => setter("font_size", length(input, host, true, false, false, true)?),
            "line-height" => {
                if let Some(expr) = expression(input)? {
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
            "opacity" => {
                let value = if let Some(expr) = expression(input)? {
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
            "display" => enumeration(
                input,
                host,
                &method,
                "Display",
                &[("flex", "Flex"), ("block", "Block"), ("none", "None")],
                false,
            )?,
            "box-sizing" => enumeration(
                input,
                host,
                &method,
                "BoxSizing",
                &[("border-box", "BorderBox"), ("content-box", "ContentBox")],
                false,
            )?,
            "position" => enumeration(
                input,
                host,
                &method,
                "Position",
                &[("relative", "Relative"), ("absolute", "Absolute")],
                false,
            )?,
            "flex-direction" => enumeration(
                input,
                host,
                &method,
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
                &method,
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
                enumeration(input, host, &method, "AlignItems", &values, true)?
            }
            "align-content" | "justify-content" => enumeration(
                input,
                host,
                &method,
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
                &method,
                "Overflow",
                &[
                    ("visible", "Visible"),
                    ("hidden", "Hidden"),
                    ("clip", "Clip"),
                    ("scroll", "Scroll"),
                ],
                false,
            )?,
            "flex-grow" | "flex-shrink" => setter(&method, scalar(input, false)?),
            "flex" => flex(input, host)?,
            "transition" => setter("transition", transition(input, host)?),
            "aspect-ratio" => {
                let value = if let Some(expr) = expression(input)? {
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
            "font-weight" => {
                let value = if let Some(expr) = expression(input)? {
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
                let value = if let Some(expr) = expression(input)? {
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
                let value = if let Some(expr) = expression(input)? {
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
                let method = match name.as_str() {
                    "font-italic" => "font_italic",
                    "text-wrap" => "text_wrap",
                    "text-underline" => "text_underline",
                    "text-strikethrough" => "text_strikethrough",
                    "text-max-lines" => "text_max_lines",
                    "text-vertical-align" => "text_vertical_align",
                    _ => "text_wrap_mode",
                };
                let value = expression(input)?.ok_or_else(|| {
                    syn::Error::new(
                        span,
                        "this style property expects a Rust expression in braces",
                    )
                })?;
                setter(method, value)
            }
            "tab-width" => setter("tab_width", scalar(input, false)?),
            "text-align" => {
                let value = if let Some(expr) = expression(input)? {
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
                let name = keyword(input, &["normal", "nowrap", "pre", "pre-wrap"])?;
                let wrap = matches!(name.as_str(), "normal" | "pre-wrap");
                setter("text_wrap", quote!(#wrap))
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
                if let Some(expr) = expression(input)? {
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
            "transform" => {
                if let Some(expr) = expression(input)? {
                    setter("transform", expr)
                } else {
                    let value = transform(input, host)?;
                    setter("transform", value)
                }
            }
            "transform-origin" => setter("transform_origin", transform_origin(input, host)?),
            _ => {
                return Err(syn::Error::new(
                    span,
                    format!("unsupported style property `{name}`"),
                ));
            }
        };
        result.push(statement);
        if !input.is_empty() {
            input.parse::<Token![;]>()?;
        }
    }
    Ok(Declarations(result))
}

fn transition(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
        return Ok(expr);
    }
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "none" {
        input.parse::<Ident>()?;
        return Ok(quote!(#host::transition::Transition::Instant));
    }
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "all" {
        input.parse::<Ident>()?;
    }
    let (duration, unit, span) = number(input)?;
    let seconds = match unit.as_str() {
        "ms" => duration / 1000.,
        "s" => duration,
        _ => return Err(syn::Error::new(span, "transition duration expects ms or s")),
    };
    if seconds < 0. {
        return Err(syn::Error::new(
            span,
            "transition duration cannot be negative",
        ));
    }
    let curve = if input.peek(Ident) {
        keyword(
            input,
            &[
                "linear",
                "ease",
                "ease-in",
                "ease-out",
                "ease-in-out",
                "spring",
            ],
        )?
    } else {
        "ease".to_owned()
    };
    let strength = if !end(input) {
        let value = numeric(input, false)?;
        if !(0. ..=1.).contains(&value) {
            return Err(syn::Error::new(
                span,
                "transition strength must be between 0 and 1",
            ));
        }
        Some(value)
    } else {
        None
    };
    if curve == "linear" && strength.is_some() {
        return Err(syn::Error::new(
            span,
            "linear transitions do not accept a strength",
        ));
    }
    let function = format_ident!(
        "{}",
        match curve.as_str() {
            "linear" => "linear",
            "ease-in" =>
                if strength.is_some() {
                    "ease_in_with_strength"
                } else {
                    "ease_in"
                },
            "ease-out" =>
                if strength.is_some() {
                    "ease_out_with_strength"
                } else {
                    "ease_out"
                },
            "ease-in-out" =>
                if strength.is_some() {
                    "ease_in_out_with_strength"
                } else {
                    "ease_in_out"
                },
            "spring" => "spring",
            "ease" =>
                if strength.is_some() {
                    "ease_with_strength"
                } else {
                    "ease"
                },
            _ => "ease",
        }
    );
    if curve == "spring" {
        let bounce = strength.unwrap_or(0.25);
        Ok(quote!(#host::transition::Transition::spring(#seconds, #bounce)))
    } else if let Some(strength) = strength {
        Ok(quote!(#host::transition::Transition::#function(#seconds, #strength)))
    } else {
        Ok(quote!(#host::transition::Transition::#function(#seconds)))
    }
}

fn number(input: ParseStream) -> Result<(f32, String, Span)> {
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

fn numeric(input: ParseStream, negative: bool) -> Result<f32> {
    let (value, unit, span) = number(input)?;
    if !unit.is_empty() || (!negative && value < 0.) {
        return Err(syn::Error::new(
            span,
            "expected a non-negative unitless number",
        ));
    }
    Ok(value)
}

fn scalar(input: ParseStream, negative: bool) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
        return Ok(expr);
    }
    let value = numeric(input, negative)?;
    Ok(quote!(#value))
}

fn length(
    input: ParseStream,
    host: &TokenStream,
    font: bool,
    auto: bool,
    negative: bool,
    percent: bool,
) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
        return Ok(expr);
    }
    if input.peek(Ident) {
        keyword(input, if auto { &["auto"] } else { &[] })?;
        return Ok(quote!(#host::style::Size::Auto));
    }
    let (value, unit, span) = number(input)?;
    length_value(value, &unit, span, host, font, negative, percent)
}

fn length_value(
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
        "rem" if font => "Rem",
        "" if value == 0. => "Px",
        _ => {
            return Err(syn::Error::new(
                span,
                "unsupported length unit for this property; use px, %, vw, vh (also em/rem for font-size), or {Rust expression}",
            ));
        }
    };
    let variant = format_ident!("{variant}");
    let ty = format_ident!("{}", if font { "FontSize" } else { "Size" });
    Ok(quote!(#host::style::#ty::#variant(#value)))
}

fn rect(
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

fn keyword(input: ParseStream, options: &[&str]) -> Result<String> {
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

fn enum_value(
    input: ParseStream,
    host: &TokenStream,
    ty: &str,
    values: &[(&str, &str)],
    optional: bool,
) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
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
fn enumeration(
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

fn flex(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
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

fn color(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
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
        return Ok(quote!(#host::color::Color::from_rgba(#r,#g,#b,#a)));
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
        return Ok(quote!(#host::color::Color::new(#r,#g,#b,#alpha)));
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
    Ok(quote!(#host::color::Color::from_rgba(#r,#g,#b,#a)))
}

fn angle(input: ParseStream) -> Result<TokenStream> {
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

fn pixel_number(input: ParseStream) -> Result<f32> {
    let (value, unit, span) = number(input)?;
    if !unit.is_empty() && unit != "px" {
        return Err(syn::Error::new(span, "expected a pixel distance"));
    }
    Ok(value)
}

fn comma(input: ParseStream) -> Result<()> {
    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }
    Ok(())
}

fn transform(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "none" {
        input.parse::<Ident>()?;
        return Ok(quote!(#host::style::Transform::default()));
    }
    let mut ops = Vec::new();
    while !end(input) {
        let name: Ident = input.parse()?;
        let args;
        parenthesized!(args in input);
        let zero = quote!(#host::style::Size::Px(0.));
        let op = match name.to_string().as_str() {
            "translate" | "translateX" | "translateY" | "translateZ" | "translate3d" => {
                if name == "translateZ" {
                    let z = pixel_number(&args)?;
                    quote!(#host::style::TransformOp::TranslateZ(#z))
                } else if name == "translate3d" {
                    let x = length(&args, host, false, false, true, true)?;
                    comma(&args)?;
                    let y = length(&args, host, false, false, true, true)?;
                    comma(&args)?;
                    let z = pixel_number(&args)?;
                    quote!(#host::style::TransformOp::Translate3d(#x,#y,#z))
                } else {
                    let first = length(&args, host, false, false, true, true)?;
                    let (x, y) = if name == "translateY" {
                        (zero, first)
                    } else if name == "translateX" {
                        (first, zero)
                    } else {
                        comma(&args)?;
                        let second = if args.is_empty() {
                            zero
                        } else {
                            length(&args, host, false, false, true, true)?
                        };
                        (first, second)
                    };
                    quote!(#host::style::TransformOp::Translate(#x,#y))
                }
            }
            "scale" | "scaleX" | "scaleY" | "scaleZ" | "scale3d" => {
                let first = numeric(&args, true)?;
                if name == "scaleZ" {
                    quote!(#host::style::TransformOp::ScaleZ(#first))
                } else if name == "scale3d" {
                    comma(&args)?;
                    let y = numeric(&args, true)?;
                    comma(&args)?;
                    let z = numeric(&args, true)?;
                    quote!(#host::style::TransformOp::Scale3d(#first,#y,#z))
                } else {
                    let (x, y) = if name == "scaleY" {
                        (1., first)
                    } else if name == "scaleX" {
                        (first, 1.)
                    } else {
                        comma(&args)?;
                        (
                            first,
                            if args.is_empty() {
                                first
                            } else {
                                numeric(&args, true)?
                            },
                        )
                    };
                    quote!(#host::style::TransformOp::Scale(#x,#y))
                }
            }
            "rotate" | "rotateZ" | "rotateX" | "rotateY" => {
                let value = angle(&args)?;
                let variant = format_ident!(
                    "{}",
                    match name.to_string().as_str() {
                        "rotateX" => "RotateX",
                        "rotateY" => "RotateY",
                        "rotateZ" => "Rotate",
                        _ => "Rotate",
                    }
                );
                quote!(#host::style::TransformOp::#variant(#value))
            }
            "perspective" => {
                let distance = pixel_number(&args)?;
                if distance <= 0. {
                    return Err(syn::Error::new(name.span(), "perspective must be positive"));
                }
                quote!(#host::style::TransformOp::Perspective(#distance))
            }
            "skew" | "skewX" | "skewY" => {
                let first = angle(&args)?;
                let (x, y) = if name == "skewY" {
                    (quote!(0f32), first)
                } else if name == "skewX" {
                    (first, quote!(0f32))
                } else {
                    comma(&args)?;
                    (
                        first,
                        if args.is_empty() {
                            quote!(0f32)
                        } else {
                            angle(&args)?
                        },
                    )
                };
                quote!(#host::style::TransformOp::Skew(#x,#y))
            }
            "matrix3d" => {
                let mut values = Vec::new();
                for index in 0..16 {
                    if index > 0 {
                        comma(&args)?;
                    }
                    values.push(numeric(&args, true)?);
                }
                quote!(#host::style::TransformOp::Matrix3d([#(#values),*]))
            }
            "matrix" => {
                let mut v = Vec::new();
                for i in 0..6 {
                    if i > 0 {
                        comma(&args)?;
                    }
                    v.push(numeric(&args, true)?);
                }
                let (a, b, c, d, e, f) = (v[0], v[1], v[2], v[3], v[4], v[5]);
                quote!(#host::style::TransformOp::Matrix(#host::math::Matrix::from_cols_array(&[#a,#b,0.,#c,#d,0.,#e,#f,1.])))
            }
            _ => {
                return Err(syn::Error::new(
                    name.span(),
                    "unsupported transform; use the supported 2D/3D translate, scale, rotate, skew, matrix, or perspective functions",
                ));
            }
        };
        if !args.is_empty() {
            return Err(args.error("unexpected transform argument"));
        }
        ops.push(op);
    }
    if ops.is_empty() {
        return Err(input.error("expected transform functions or none"));
    }
    Ok(quote!(#host::style::Transform(vec![#(#ops),*])))
}

fn transform_origin(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if let Some(expr) = expression(input)? {
        return Ok(expr);
    }
    let mut parts = Vec::new();
    while !end(input) {
        let (value, axis) = if input.peek(Ident) {
            let value = keyword(input, &["left", "right", "top", "bottom", "center"])?;
            let percent = match value.as_str() {
                "left" | "top" => 0f32,
                "right" | "bottom" => 100.,
                _ => 50.,
            };
            (
                quote!(#host::style::Size::Percent(#percent)),
                match value.as_str() {
                    "left" | "right" => 1,
                    "top" | "bottom" => 2,
                    _ => 0,
                },
            )
        } else {
            (length(input, host, false, false, true, true)?, 0)
        };
        parts.push((value, axis));
        if parts.len() > 2 {
            return Err(input.error("only 2D transform-origin is supported"));
        }
    }
    let center = quote!(#host::style::Size::Percent(50.));
    let (x, y) = match parts.as_slice() {
        [(v, 2)] => (center, v.clone()),
        [(v, _)] => (v.clone(), center),
        [(a, aa), (b, ba)] if *aa == 2 || *ba == 1 => {
            if *aa == *ba && *aa != 0 {
                return Err(input.error("transform-origin repeats the same axis"));
            }
            (b.clone(), a.clone())
        }
        [(a, aa), (b, ba)] => {
            if *aa == *ba && *aa != 0 {
                return Err(input.error("transform-origin repeats the same axis"));
            }
            (a.clone(), b.clone())
        }
        _ => return Err(input.error("expected one or two transform-origin values")),
    };
    Ok(quote!(#host::style::TransformOrigin(#x,#y)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse::Parser;

    fn parse(source: &str) -> Result<Declarations> {
        (|input: ParseStream| parse_declarations(input, &quote!(::host))).parse_str(source)
    }

    #[test]
    fn invalid_style_has_property_specific_errors() {
        for (source, message) in [
            ("unknown: 1px;", "unsupported style property"),
            ("width: 10;", "unsupported length unit"),
            ("padding: -1px;", "negative lengths"),
            ("padding: 1px 2px 3px 4px 5px;", "one to four"),
            ("padding:;", "one to four"),
            ("color: #12345;", "expected #rgb"),
            ("font-weight: 1200;", "1 to 1000"),
            ("transform: rotate(30px);", "expected deg"),
            (
                "transform: rotate(30deg, 20deg);",
                "unexpected transform argument",
            ),
            ("transform: perspective(0px);", "must be positive"),
            ("transform-origin: left right;", "same axis"),
            ("on-tap: {|| {}};", "unsupported style property"),
            ("width: 10px !important;", "expected `;`"),
        ] {
            let error = match parse(source) {
                Ok(_) => panic!("accepted {source}"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains(message), "{source}: {error}");
        }
    }

    #[test]
    fn style_values_and_rust_expressions_parse_without_a_template_language() {
        for source in [
            "opacity: .5; transform: scale(-.5, .5) skewX(10deg);",
            "margin: 0 auto; width: {width}; color: {theme_color};",
            "flex: 1 0 auto; flex: 2 30px; flex: 25%;",
            "transform: translateX(-20%) translateY(2vh) skew(10deg, 2deg) rotate(1rad);",
            "transform: perspective(600px) rotateX(45deg) rotateY(20deg) translateZ(12px) scale3d(1, 2, .5); transition: all 250ms ease-in-out;",
            "color: #001122; color: #aabbccdd; color: rgba(1, 2, 3, .5);",
            "padding-top: 100px; padding: 8px; width: 20px; width: 30px",
            "line-height: 140%; line-height: 1.5em; line-height: normal;",
            "transform-origin: top center; transform-origin: center left;",
        ] {
            assert!(parse(source).is_ok(), "{source}");
        }
    }
}
