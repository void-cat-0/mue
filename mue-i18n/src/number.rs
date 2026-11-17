use std::{borrow::Cow, rc::Rc, sync::Arc};

use fluent::types::{FluentNumber, FluentNumberOptions};
use fluent::{FluentArgs, FluentResource, FluentValue};
use fluent_syntax::ast;
use fluent_syntax::unicode::unescape_unicode_to_string;

use crate::error::TranslationIssue;

const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_991.0;
const MAX_FRACTION_DIGITS: usize = 18;
const MAX_OTHER_DIGITS: usize = 21;

type NumberResult<T = ()> = Result<T, &'static str>;

/// Validate before Fluent clones, stringifies or selects on any supplied value.
/// Custom values, None and Error are deliberately unsupported, even if unused.
/// Omitting diagnostics skips their allocation, not validation of any argument.
pub(crate) fn validate_arguments(
    args: &FluentArgs<'_>,
    mut issues: Option<&mut Vec<TranslationIssue>>,
) -> bool {
    let mut valid = true;
    for (name, value) in args.iter() {
        let result = match value {
            FluentValue::String(_) => Ok(()),
            FluentValue::Number(number) => validate_number(number),
            _ => Err("only String and Number arguments are supported"),
        };
        if let Err(reason) = result {
            valid = false;
            if let Some(issues) = issues.as_deref_mut() {
                issues.push(TranslationIssue::InvalidArgument {
                    name: Arc::from(name),
                    reason: Arc::from(reason),
                });
            }
        }
    }
    valid
}

/// Normalize the zero minimum-fraction option without copying ordinary arguments.
/// Borrow names and strings; clone numbers to leave the original input/options intact.
/// Fluent 0.16 otherwise renders an integer with Some(0) as `1.` rather than `1`.
pub(crate) fn normalized_arguments<'a>(args: &'a FluentArgs<'_>) -> Option<FluentArgs<'a>> {
    if !args.iter().any(|(_, value)| {
        matches!(value, FluentValue::Number(number) if number.options.minimum_fraction_digits == Some(0))
    }) {
        return None;
    }
    Some(
        args.iter()
            .map(|(name, value)| {
                let value = match value {
                    FluentValue::String(value) => {
                        FluentValue::String(Cow::Borrowed(value.as_ref()))
                    }
                    FluentValue::Number(number) => {
                        let mut number = number.clone();
                        normalize_zero_fraction(&mut number);
                        FluentValue::Number(number)
                    }
                    _ => value.clone(),
                };
                (Cow::Borrowed(name), value)
            })
            .collect(),
    )
}

fn normalize_zero_fraction(number: &mut FluentNumber) {
    if number.options.minimum_fraction_digits == Some(0) {
        number.options.minimum_fraction_digits = None;
    }
}

/// Fluent 0.16 converts f64.to_string() fractions to u64 with expect, then
/// multiplies f by 10^(minimum_fraction_digits - v). At most 18 decimal places
/// bounds both that power and the resulting fraction below 10^18 < u64::MAX.
/// The original value and every inherited option are checked, not just NUMBER
/// call-site options. Tiny floats requiring more decimal places are rejected.
pub(crate) fn validate_number(number: &FluentNumber) -> NumberResult {
    if !number.value.is_finite() {
        return Err("number must be finite");
    }
    if number.value.abs() > MAX_EXACT_INTEGER {
        return Err("number magnitude must not exceed 2^53 - 1");
    }
    let decimal = number.value.abs().to_string();
    if let Some((integer, fraction)) = decimal.split_once('.') {
        if !integer.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.is_empty()
            || fraction.len() > MAX_FRACTION_DIGITS
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("number must have at most 18 decimal fraction digits");
        }
    } else if !decimal.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("number must have a bounded plain decimal representation");
    }
    validate_options(&number.options)
}

fn validate_options(options: &FluentNumberOptions) -> NumberResult {
    if options
        .minimum_integer_digits
        .is_some_and(|digits| !(1..=MAX_OTHER_DIGITS).contains(&digits))
    {
        return Err("minimumIntegerDigits must be in 1..=21");
    }
    if options
        .minimum_fraction_digits
        .is_some_and(|digits| digits > MAX_FRACTION_DIGITS)
    {
        return Err("minimumFractionDigits must be in 0..=18");
    }
    if options
        .maximum_fraction_digits
        .is_some_and(|digits| digits > MAX_FRACTION_DIGITS)
    {
        return Err("maximumFractionDigits must be in 0..=18");
    }
    if options
        .minimum_significant_digits
        .is_some_and(|digits| !(1..=MAX_OTHER_DIGITS).contains(&digits))
    {
        return Err("minimumSignificantDigits must be in 1..=21");
    }
    if options
        .maximum_significant_digits
        .is_some_and(|digits| !(1..=MAX_OTHER_DIGITS).contains(&digits))
    {
        return Err("maximumSignificantDigits must be in 1..=21");
    }
    if matches!(
        (options.minimum_fraction_digits, options.maximum_fraction_digits),
        (Some(minimum), Some(maximum)) if minimum > maximum
    ) {
        return Err("minimumFractionDigits must not exceed maximumFractionDigits");
    }
    if matches!(
        (options.minimum_significant_digits, options.maximum_significant_digits),
        (Some(minimum), Some(maximum)) if minimum > maximum
    ) {
        return Err("minimumSignificantDigits must not exceed maximumSignificantDigits");
    }
    if options
        .currency
        .as_deref()
        .is_some_and(|currency| !valid_currency(currency))
    {
        return Err("currency must contain exactly three ASCII letters");
    }
    Ok(())
}

fn valid_currency(currency: &str) -> bool {
    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn validate_digit_option(
    value: &FluentValue<'_>,
    minimum: usize,
    maximum: usize,
    reason: &'static str,
) -> NumberResult {
    let FluentValue::Number(number) = value else {
        return Err(reason);
    };
    validate_number(number)?;
    if number.value.fract() != 0.0 || number.value < minimum as f64 || number.value > maximum as f64
    {
        return Err(reason);
    }
    Ok(())
}

fn validate_named_options(named: &FluentArgs<'_>) -> NumberResult {
    for (name, value) in named.iter() {
        match (name, value) {
            ("minimumIntegerDigits", value) => validate_digit_option(
                value,
                1,
                MAX_OTHER_DIGITS,
                "minimumIntegerDigits must be an integer in 1..=21",
            )?,
            ("minimumFractionDigits", value) => validate_digit_option(
                value,
                0,
                MAX_FRACTION_DIGITS,
                "minimumFractionDigits must be an integer in 0..=18",
            )?,
            ("maximumFractionDigits", value) => validate_digit_option(
                value,
                0,
                MAX_FRACTION_DIGITS,
                "maximumFractionDigits must be an integer in 0..=18",
            )?,
            ("minimumSignificantDigits", value) => validate_digit_option(
                value,
                1,
                MAX_OTHER_DIGITS,
                "minimumSignificantDigits must be an integer in 1..=21",
            )?,
            ("maximumSignificantDigits", value) => validate_digit_option(
                value,
                1,
                MAX_OTHER_DIGITS,
                "maximumSignificantDigits must be an integer in 1..=21",
            )?,
            ("type", FluentValue::String(value))
                if matches!(value.as_ref(), "cardinal" | "ordinal") => {}
            ("type", _) => return Err("NUMBER type must be cardinal or ordinal"),
            ("style", FluentValue::String(value))
                if matches!(value.as_ref(), "decimal" | "currency" | "percent") => {}
            ("style", _) => return Err("NUMBER style must be decimal, currency or percent"),
            ("currency", FluentValue::String(value)) if valid_currency(value) => {}
            ("currency", _) => return Err("currency must contain exactly three ASCII letters"),
            ("currencyDisplay", FluentValue::String(value))
                if matches!(value.as_ref(), "symbol" | "code" | "name") => {}
            ("currencyDisplay", _) => {
                return Err("currencyDisplay must be symbol, code or name");
            }
            ("useGrouping", FluentValue::String(value))
                if matches!(value.as_ref(), "true" | "false") => {}
            ("useGrouping", _) => return Err("useGrouping must be the string true or false"),
            _ => return Err("unknown NUMBER option"),
        }
    }
    Ok(())
}

/// Called only by the private registered NUMBER wrapper. Errors are recorded
/// by that wrapper before it returns FluentValue::Error to the resolver.
/// Never merge unchecked options: Fluent otherwise silently truncates/casts
/// digit counts and can produce a number which panics during plural selection.
pub(crate) fn safe_number(
    positional: &[FluentValue<'_>],
    named: &FluentArgs<'_>,
) -> NumberResult<FluentNumber> {
    let [FluentValue::Number(number)] = positional else {
        return Err("NUMBER requires exactly one numeric positional argument");
    };
    validate_number(number)?;
    validate_named_options(named)?;
    let mut number = number.clone();
    number.options.merge(named);
    // Merge changes only options; the original value was already validated.
    validate_options(&number.options)?;
    normalize_zero_fraction(&mut number);
    Ok(number)
}

/// Every numeric literal can reach a selector directly, through a term's local
/// arguments or through NUMBER. Validate all entries and branches, not just
/// NUMBER calls or the currently requested language.
pub(crate) fn validate_resource_numbers(
    resource: &FluentResource,
) -> Result<(), (Rc<str>, Rc<str>)> {
    for entry in resource.entries() {
        match entry {
            ast::Entry::Message(message) => {
                if let Some(value) = &message.value {
                    validate_pattern(value)
                        .map_err(|reason| (Rc::from(message.id.name), Rc::from(reason)))?;
                }
                for attribute in &message.attributes {
                    validate_pattern(&attribute.value).map_err(|reason| {
                        (
                            Rc::from(format!("{}.{}", message.id.name, attribute.id.name)),
                            Rc::from(reason),
                        )
                    })?;
                }
            }
            ast::Entry::Term(term) => {
                validate_pattern(&term.value)
                    .map_err(|reason| (Rc::from(format!("-{}", term.id.name)), Rc::from(reason)))?;
                for attribute in &term.attributes {
                    validate_pattern(&attribute.value).map_err(|reason| {
                        (
                            Rc::from(format!("-{}.{}", term.id.name, attribute.id.name)),
                            Rc::from(reason),
                        )
                    })?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_literal(value: &str) -> NumberResult<FluentNumber> {
    let number: FluentNumber = value.parse().map_err(|_| "invalid numeric literal")?;
    validate_number(&number)?;
    Ok(number)
}

fn validate_pattern(pattern: &ast::Pattern<&str>) -> NumberResult {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            validate_expression(expression)?;
        }
    }
    Ok(())
}

fn validate_expression(expression: &ast::Expression<&str>) -> NumberResult {
    match expression {
        ast::Expression::Inline(inline) => validate_inline(inline),
        ast::Expression::Select { selector, variants } => {
            validate_inline(selector)?;
            for variant in variants {
                if let ast::VariantKey::NumberLiteral { value } = &variant.key {
                    validate_literal(value)?;
                }
                validate_pattern(&variant.value)?;
            }
            Ok(())
        }
    }
}

fn validate_inline(inline: &ast::InlineExpression<&str>) -> NumberResult {
    match inline {
        ast::InlineExpression::NumberLiteral { value } => validate_literal(value).map(|_| ()),
        ast::InlineExpression::FunctionReference { id, arguments } => {
            validate_call_arguments(arguments)?;
            if id.name == "NUMBER" {
                validate_number_call(arguments)?;
            }
            Ok(())
        }
        ast::InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => validate_call_arguments(arguments),
        ast::InlineExpression::Placeable { expression } => validate_expression(expression),
        _ => Ok(()),
    }
}

fn validate_call_arguments(arguments: &ast::CallArguments<&str>) -> NumberResult {
    for positional in &arguments.positional {
        validate_inline(positional)?;
    }
    for named in &arguments.named {
        validate_inline(&named.value)?;
    }
    Ok(())
}

fn validate_number_call(arguments: &ast::CallArguments<&str>) -> NumberResult {
    let [positional] = arguments.positional.as_slice() else {
        return Err("NUMBER requires exactly one numeric positional argument");
    };
    let mut named = FluentArgs::with_capacity(arguments.named.len());
    for argument in &arguments.named {
        let value = match &argument.value {
            ast::InlineExpression::StringLiteral { value } => {
                FluentValue::String(unescape_unicode_to_string(value))
            }
            ast::InlineExpression::NumberLiteral { value } => {
                FluentValue::Number(validate_literal(value)?)
            }
            _ => return Err("NUMBER options must be string or numeric literals"),
        };
        named.set(argument.name.name, value);
    }
    // Variables and nested NUMBER calls are checked again after runtime merge.
    // A neutral numeric input lets us validate their static named options now.
    let number = match positional {
        ast::InlineExpression::NumberLiteral { value } => validate_literal(value)?,
        ast::InlineExpression::VariableReference { .. } => FluentNumber::from(0),
        ast::InlineExpression::FunctionReference { id, .. } if id.name == "NUMBER" => {
            FluentNumber::from(0)
        }
        _ => return Err("NUMBER requires a numeric literal, variable or nested NUMBER call"),
    };
    safe_number(&[FluentValue::Number(number)], &named).map(|_| ())
}
