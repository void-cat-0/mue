//! Transitions and transforms.
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
        "transition" => setter("transition", transition(input, host)?),
        "transform" => {
            if let Some(expr) = expression(input)? {
                setter("transform", expr)
            } else {
                let value = transform(input, host)?;
                setter("transform", value)
            }
        }
        "transform-origin" => setter("transform_origin", transform_origin(input, host)?),
        _ => return Ok(None),
    }))
}

pub(super) fn transition(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if let Some(value) = token(input, &quote!(#host::motion::transition::Transition))? {
        return Ok(value);
    }
    if let Some(expr) =
        expression_with_token(input, &quote!(#host::motion::transition::Transition))?
    {
        return Ok(expr);
    }
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "none" {
        input.parse::<Ident>()?;
        return Ok(quote!(#host::motion::transition::Transition::Instant));
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
            "ease" if strength.is_some() => "ease_with_strength",
            _ => "ease",
        }
    );
    if curve == "spring" {
        let bounce = strength.unwrap_or(0.25);
        Ok(quote!(#host::motion::transition::Transition::spring(#seconds, #bounce)))
    } else if let Some(strength) = strength {
        Ok(quote!(#host::motion::transition::Transition::#function(#seconds, #strength)))
    } else {
        Ok(quote!(#host::motion::transition::Transition::#function(#seconds)))
    }
}

pub(super) fn transform(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "token" {
        return Err(
            input.error("token!(...) is not supported for transform; use a {Rust expression}")
        );
    }
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

pub(super) fn transform_origin(input: ParseStream, host: &TokenStream) -> Result<TokenStream> {
    if input.peek(Ident) && input.fork().parse::<Ident>()? == "token" {
        return Err(input
            .error("token!(...) is not supported for transform-origin; use a {Rust expression}"));
    }
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
