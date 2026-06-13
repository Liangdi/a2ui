//! Basic catalog function implementations for A2UI.
//!
//! Provides validation, logical, formatting, localization, and side-effect
//! functions that implement the `FunctionImplementation` trait.

use std::collections::HashMap;

use chrono::{NaiveDateTime, Timelike, Datelike};
use regex::Regex;
use serde_json::Value;

use crate::core::catalog::function_api::{FunctionImplementation, ReturnType};
use crate::core::error::A2uiError;
use crate::core::model::data_context::DataContext;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Extract a required string argument, returning a descriptive error if missing.
fn require_str<'a>(
    args: &'a HashMap<String, Value>,
    key: &str,
    func_name: &str,
) -> std::result::Result<&'a str, A2uiError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            A2uiError::InvalidFunctionCall(format!(
                "{func_name}: missing or non-string argument '{key}'"
            ))
        })
}

/// Extract an optional f64 argument.
fn opt_f64(args: &HashMap<String, Value>, key: &str) -> Option<f64> {
    args.get(key).and_then(|v| v.as_f64())
}

/// Extract an optional bool argument.
fn opt_bool(args: &HashMap<String, Value>, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

// ===========================================================================
// 1. Required
// ===========================================================================

/// Validation function that returns `true` when the value is non-null,
/// non-empty-string, and non-empty-array.
pub struct RequiredFunction;

impl FunctionImplementation for RequiredFunction {
    fn name(&self) -> &'static str {
        "required"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args.get("value").cloned().unwrap_or(Value::Null);
        let present = match &val {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
            _ => true,
        };
        Ok(Value::Bool(present))
    }
}

// ===========================================================================
// 2. Regex
// ===========================================================================

/// Validation function that tests a string against a regex pattern.
pub struct RegexFunction;

impl FunctionImplementation for RegexFunction {
    fn name(&self) -> &'static str {
        "regex"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let value = require_str(args, "value", "regex")?;
        let pattern = require_str(args, "pattern", "regex")?;

        let re = Regex::new(pattern).map_err(|e| {
            A2uiError::InvalidFunctionCall(format!("regex: invalid pattern '{pattern}': {e}"))
        })?;

        Ok(Value::Bool(re.is_match(value)))
    }
}

// ===========================================================================
// 3. Length
// ===========================================================================

/// Validation function that checks string length bounds.
pub struct LengthFunction;

impl FunctionImplementation for LengthFunction {
    fn name(&self) -> &'static str {
        "length"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let value = require_str(args, "value", "length")?;
        let len = value.chars().count() as f64;

        if let Some(min) = opt_f64(args, "min") {
            if len < min {
                return Ok(Value::Bool(false));
            }
        }
        if let Some(max) = opt_f64(args, "max") {
            if len > max {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }
}

// ===========================================================================
// 4. Numeric
// ===========================================================================

/// Validation function that checks whether a value is a valid number within
/// optional bounds.
pub struct NumericFunction;

impl FunctionImplementation for NumericFunction {
    fn name(&self) -> &'static str {
        "numeric"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args.get("value").cloned().unwrap_or(Value::Null);

        // Accept either a JSON number or a numeric string.
        let num = match &val {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        };

        let Some(n) = num else {
            return Ok(Value::Bool(false));
        };

        if let Some(min) = opt_f64(args, "min") {
            if n < min {
                return Ok(Value::Bool(false));
            }
        }
        if let Some(max) = opt_f64(args, "max") {
            if n > max {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }
}

// ===========================================================================
// 5. Email
// ===========================================================================

/// Validation function that tests a value against a simple email pattern.
pub struct EmailFunction;

impl FunctionImplementation for EmailFunction {
    fn name(&self) -> &'static str {
        "email"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let value = require_str(args, "value", "email")?;

        // /^[^\s@]+@[^\s@]+\.[^\s@]+$/
        let re = Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap();
        Ok(Value::Bool(re.is_match(value)))
    }
}

// ===========================================================================
// 6. And
// ===========================================================================

/// Logical AND over an array of booleans.
pub struct AndFunction;

impl FunctionImplementation for AndFunction {
    fn name(&self) -> &'static str {
        "and"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let arr = args
            .get("values")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall("and: missing or non-array argument 'values'".into())
            })?;

        let all_true = arr.iter().all(|v| v.as_bool().unwrap_or(false));
        Ok(Value::Bool(all_true))
    }
}

// ===========================================================================
// 7. Or
// ===========================================================================

/// Logical OR over an array of booleans.
pub struct OrFunction;

impl FunctionImplementation for OrFunction {
    fn name(&self) -> &'static str {
        "or"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let arr = args
            .get("values")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall("or: missing or non-array argument 'values'".into())
            })?;

        let any_true = arr.iter().any(|v| v.as_bool().unwrap_or(false));
        Ok(Value::Bool(any_true))
    }
}

// ===========================================================================
// 8. Not
// ===========================================================================

/// Logical NOT of a single boolean value.
pub struct NotFunction;

impl FunctionImplementation for NotFunction {
    fn name(&self) -> &'static str {
        "not"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args
            .get("value")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall(
                    "not: missing or non-boolean argument 'value'".into(),
                )
            })?;

        Ok(Value::Bool(!val))
    }
}

// ===========================================================================
// 9. FormatNumber
// ===========================================================================

/// Format a number with optional grouping and decimal precision.
pub struct FormatNumberFunction;

impl FunctionImplementation for FormatNumberFunction {
    fn name(&self) -> &'static str {
        "formatNumber"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall(
                    "formatNumber: missing or non-numeric argument 'value'".into(),
                )
            })?;

        let grouping = opt_bool(args, "grouping").unwrap_or(true);
        let decimals = opt_f64(args, "decimals").map(|d| d as usize);

        let formatted = format_number_impl(val, grouping, decimals);
        Ok(Value::String(formatted))
    }
}

/// Core number formatting logic.
fn format_number_impl(val: f64, grouping: bool, decimals: Option<usize>) -> String {
    let abs = val.abs();
    let sign = if val < 0.0 { "-" } else { "" };

    // Integer and fractional parts.
    let int_part = abs.trunc() as u64;

    let int_str = if grouping {
        format_with_grouping(int_part)
    } else {
        int_part.to_string()
    };

    let frac_str = match decimals {
        Some(d) => {
            // Round to exactly `d` decimal places.
            let rounded = format!("{abs:.d$}");
            // rounded is "NNN.FFF"; take everything after the dot.
            if d == 0 {
                String::new()
            } else {
                rounded
                    .find('.')
                    .map(|pos| rounded[pos + 1..].to_string())
                    .unwrap_or_default()
            }
        }
        None => {
            // Use the original decimal representation to avoid float noise.
            // `format!("{}", f64)` gives a reasonably short representation.
            let s = format!("{abs}");
            if let Some(dot) = s.find('.') {
                let frac = &s[dot + 1..];
                // "0" means no meaningful fractional part.
                if frac == "0" {
                    String::new()
                } else {
                    frac.to_string()
                }
            } else {
                String::new()
            }
        }
    };

    if frac_str.is_empty() {
        format!("{sign}{int_str}")
    } else {
        format!("{sign}{int_str}.{frac_str}")
    }
}

/// Insert comma thousands separators into an integer string.
fn format_with_grouping(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let mut count = 0;
    for ch in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(ch);
        count += 1;
    }
    result.chars().rev().collect()
}

// ===========================================================================
// 10. FormatCurrency
// ===========================================================================

/// Format a number as a currency string.
pub struct FormatCurrencyFunction;

impl FunctionImplementation for FormatCurrencyFunction {
    fn name(&self) -> &'static str {
        "formatCurrency"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall(
                    "formatCurrency: missing or non-numeric argument 'value'".into(),
                )
            })?;

        let currency = require_str(args, "currency", "formatCurrency")?;

        let grouping = opt_bool(args, "grouping").unwrap_or(true);
        let decimals = opt_f64(args, "decimals").map(|d| d as usize);

        let formatted = format_number_impl(val, grouping, decimals);
        Ok(Value::String(format!("{currency} {formatted}")))
    }
}

// ===========================================================================
// 11. FormatDate
// ===========================================================================

/// Format an ISO-8601 datetime string using a TR35-style pattern.
pub struct FormatDateFunction;

impl FunctionImplementation for FormatDateFunction {
    fn name(&self) -> &'static str {
        "formatDate"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let value = require_str(args, "value", "formatDate")?;
        let fmt = require_str(args, "format", "formatDate")?;

        // Parse as ISO 8601. Try with timezone first, then without.
        let dt = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f"))
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
            .or_else(|_| {
                // Try date-only
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            })
            .map_err(|_| {
                A2uiError::InvalidFunctionCall(format!(
                    "formatDate: could not parse datetime '{value}'"
                ))
            })?;

        let formatted = apply_date_format(&dt, fmt);
        Ok(Value::String(formatted))
    }
}

/// Apply a simple TR35-style date format pattern.
fn apply_date_format(dt: &NaiveDateTime, fmt: &str) -> String {
    let mut result = String::with_capacity(fmt.len() * 2);
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;

    let weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let months = [
        "Jan",
        "Feb",
        "Mar",
        "Apr",
        "May",
        "Jun",
        "Jul",
        "Aug",
        "Sep",
        "Oct",
        "Nov",
        "Dec",
    ];

    while i < chars.len() {
        let c = chars[i];

        // Count consecutive identical letters.
        let start = i;
        while i < chars.len() && chars[i] == c {
            i += 1;
        }
        let count = i - start;

        match c {
            'y' => match count {
                4 => result.push_str(&format!("{:04}", dt.year())),
                2 => result.push_str(&format!("{:02}", dt.year() % 100)),
                _ => result.push_str(&dt.year().to_string()),
            },
            'M' => match count {
                3 => result.push_str(months[(dt.month() - 1) as usize]),
                2 => result.push_str(&format!("{:02}", dt.month())),
                1 => result.push_str(&dt.month().to_string()),
                _ => result.push_str(&format!("{:02}", dt.month())),
            },
            'd' => match count {
                2 => result.push_str(&format!("{:02}", dt.day())),
                1 => result.push_str(&dt.day().to_string()),
                _ => result.push_str(&format!("{:02}", dt.day())),
            },
            'H' => match count {
                2 => result.push_str(&format!("{:02}", dt.hour())),
                1 => result.push_str(&dt.hour().to_string()),
                _ => result.push_str(&format!("{:02}", dt.hour())),
            },
            'm' => match count {
                2 => result.push_str(&format!("{:02}", dt.minute())),
                1 => result.push_str(&dt.minute().to_string()),
                _ => result.push_str(&format!("{:02}", dt.minute())),
            },
            's' => match count {
                2 => result.push_str(&format!("{:02}", dt.second())),
                1 => result.push_str(&dt.second().to_string()),
                _ => result.push_str(&format!("{:02}", dt.second())),
            },
            'E' => {
                // chrono::Weekday: Mon=0 .. Sun=6
                result.push_str(weekdays[dt.weekday().num_days_from_monday() as usize]);
            }
            'h' => {
                // 12-hour clock: 1-12 (no leading zero for single 'h', leading zero for 'hh')
                let hour_12 = dt.hour() % 12;
                let hour_12 = if hour_12 == 0 { 12 } else { hour_12 };
                match count {
                    2 => result.push_str(&format!("{hour_12:02}")),
                    _ => result.push_str(&hour_12.to_string()),
                }
            }
            'a' => {
                // AM/PM marker
                let ampm = if dt.hour() < 12 { "AM" } else { "PM" };
                result.push_str(ampm);
            }
            '\'' => {
                // Escaped literal between single quotes.
                // Find the closing quote.
                let mut j = start + 1;
                while j < chars.len() && chars[j] != '\'' {
                    result.push(chars[j]);
                    j += 1;
                }
                i = j + 1;
            }
            _ => {
                // Literal character — push all repetitions.
                for _ in 0..count {
                    result.push(c);
                }
            }
        }
    }

    result
}

// ===========================================================================
// 12. FormatString
// ===========================================================================

/// String interpolation with `${expression}` blocks and `\${` escaping.
pub struct FormatStringFunction;

impl FunctionImplementation for FormatStringFunction {
    fn name(&self) -> &'static str {
        "formatString"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let value = require_str(args, "value", "formatString")?;
        let result = interpolate_string(value, context);
        Ok(Value::String(result))
    }
}

/// Perform basic `${...}` interpolation on a template string.
///
/// Supported expressions:
/// - `${/absolute/path}` — resolve absolute data path
/// - `${relative/path}` — resolve relative data path (via context)
/// - `${functionName(key:value,...)}` — call a registered function
/// - `\${` — escaped literal `${`
fn interpolate_string(template: &str, context: &DataContext) -> String {
    let mut result = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'$' {
            // Check for escaped `\${`
            if i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                result.push_str("${");
                i += 3;
                continue;
            }
            // Just a backslash before $ without { — keep as-is.
            result.push('\\');
            i += 1;
            continue;
        }

        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Find the closing `}`.
            let start = i + 2;
            let mut depth = 1u32;
            let mut end = start;
            while end < bytes.len() && depth > 0 {
                if bytes[end] == b'{' {
                    depth += 1;
                } else if bytes[end] == b'}' {
                    depth -= 1;
                }
                if depth > 0 {
                    end += 1;
                }
            }

            if depth == 0 {
                let expr = &template[start..end];
                let resolved = resolve_expression(expr, context);
                result.push_str(&resolved);
                i = end + 1; // skip past '}'
            } else {
                // Unmatched `${`, keep as literal.
                result.push_str("${");
                i += 2;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// Resolve a single expression inside `${...}`.
///
/// Supported forms:
/// - `/absolute/path` or `relative/path` — data interpolation
/// - `functionName(key1:value1,key2:value2)` — function call with arguments
///
/// Function call argument values can be:
/// - A data path: `/path/to/data` or `relative/path`
/// - A quoted string: `'text'`
/// - A number: `42` or `3.14`
fn resolve_expression(expr: &str, context: &DataContext) -> String {
    let trimmed = expr.trim();

    // Check for function call syntax: identifier followed by '('
    if let Some(paren_pos) = trimmed.find('(') {
        let func_name = &trimmed[..paren_pos];
        // Validate that func_name looks like an identifier (letters, digits, underscore).
        if is_identifier(func_name)
            && trimmed.ends_with(')')
            && func_name.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_')
        {
            let args_str = &trimmed[paren_pos + 1..trimmed.len() - 1];
            let args = match parse_function_args(args_str, context) {
                Ok(a) => a,
                Err(_) => return String::new(),
            };
            return context
                .call_function_by_name(func_name, &args)
                .map(|v| crate::core::model::data_context::value_to_string(&v))
                .unwrap_or_default();
        }
    }

    // Absolute data path.
    if trimmed.starts_with('/') {
        return context
            .get(trimmed)
            .map(|v| crate::core::model::data_context::value_to_string(&v))
            .unwrap_or_default();
    }

    // Relative data path.
    context
        .get(trimmed)
        .map(|v| crate::core::model::data_context::value_to_string(&v))
        .unwrap_or_default()
}

/// Check if a string is a valid identifier (alphanumeric + underscore).
fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Parse function arguments from a comma-separated `key:value` string.
///
/// Values can be:
/// - Single-quoted strings: `'hello'`
/// - Data paths: `/path` or `relative`
/// - Numbers: `42`, `3.14`
/// - Booleans: `true`, `false`
fn parse_function_args(
    args_str: &str,
    context: &DataContext,
) -> Result<HashMap<String, Value>, A2uiError> {
    let mut args = HashMap::new();
    if args_str.trim().is_empty() {
        return Ok(args);
    }

    let mut i = 0;
    let chars: Vec<char> = args_str.chars().collect();

    while i < chars.len() {
        // Skip whitespace and commas between args.
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        // Parse key (identifier).
        let key_start = i;
        while i < chars.len() && chars[i] != ':' && chars[i] != '=' {
            i += 1;
        }
        if i >= chars.len() || (chars[i] != ':' && chars[i] != '=') {
            return Err(A2uiError::InvalidFunctionCall(format!(
                "formatString: expected ':' or '=' in function args at position {i}"
            )));
        }
        let key: String = chars[key_start..i].iter().collect();
        let key = key.trim().to_string();
        i += 1; // skip ':' or '='

        // Skip whitespace after separator.
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        // Parse value.
        let val;
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '{' {
            // Nested ${...} expression — resolve recursively.
            i += 2; // skip '${'
            let mut depth = 1u32;
            let inner_start = i;
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                }
                if depth > 0 {
                    i += 1;
                }
            }
            let inner: String = chars[inner_start..i].iter().collect();
            if i < chars.len() {
                i += 1; // skip closing '}'
            }
            // Resolve the inner expression as a data path.
            val = context
                .get(inner.trim())
                .unwrap_or(Value::String(String::new()));
        } else if i < chars.len() && chars[i] == '\'' {
            // Single-quoted string.
            i += 1; // skip opening quote
            let val_start = i;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            let s: String = chars[val_start..i].iter().collect();
            if i < chars.len() {
                i += 1; // skip closing quote
            }
            val = Value::String(s);
        } else if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
            // Number (possibly negative).
            let val_start = i;
            if chars[i] == '-' {
                i += 1;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let num_str: String = chars[val_start..i].iter().collect();
            val = num_str
                .parse::<f64>()
                .map(|n| serde_json::json!(n))
                .unwrap_or(Value::String(num_str));
        } else {
            // Otherwise treat as a data path or boolean literal.
            let val_start = i;
            while i < chars.len() && chars[i] != ',' && chars[i] != ')' && !chars[i].is_whitespace()
            {
                i += 1;
            }
            let token: String = chars[val_start..i].iter().collect();
            let token = token.trim();

            // Check for boolean literals.
            if token == "true" {
                val = Value::Bool(true);
            } else if token == "false" {
                val = Value::Bool(false);
            } else {
                // Resolve as data path. If not found, use the raw token as a string.
                val = context
                    .get(token)
                    .unwrap_or_else(|| Value::String(token.to_string()));
            }
        };

        args.insert(key, val);
    }

    Ok(args)
}

// ===========================================================================
// 13. Pluralize
// ===========================================================================

/// Resolve the correct plural form for a numeric value.
pub struct PluralizeFunction;

impl FunctionImplementation for PluralizeFunction {
    fn name(&self) -> &'static str {
        "pluralize"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::String
    }

    fn execute(
        &self,
        args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        let val = args
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                A2uiError::InvalidFunctionCall(
                    "pluralize: missing or non-numeric argument 'value'".into(),
                )
            })?;

        // Determine the plural category using simple English rules.
        let category = if val == 0.0 {
            "zero"
        } else if val == 1.0 {
            "one"
        } else {
            "other"
        };

        // Try the specific category, then fall back to "other".
        let result = args
            .get(category)
            .and_then(|v| v.as_str())
            .or_else(|| args.get("other").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();

        Ok(Value::String(result))
    }
}

// ===========================================================================
// 14. OpenUrl
// ===========================================================================

/// No-op side-effect function (cannot open URLs in a TUI environment).
pub struct OpenUrlFunction;

impl FunctionImplementation for OpenUrlFunction {
    fn name(&self) -> &'static str {
        "openUrl"
    }

    fn return_type(&self) -> ReturnType {
        ReturnType::Void
    }

    fn execute(
        &self,
        _args: &HashMap<String, Value>,
        _context: &DataContext,
    ) -> Result<Value, A2uiError> {
        // No-op in TUI environment.
        Ok(Value::Null)
    }
}

// ===========================================================================
// Builder
// ===========================================================================

/// Construct a vector containing all built-in basic function implementations.
pub fn build_basic_functions() -> Vec<Box<dyn FunctionImplementation>> {
    vec![
        Box::new(RequiredFunction),
        Box::new(RegexFunction),
        Box::new(LengthFunction),
        Box::new(NumericFunction),
        Box::new(EmailFunction),
        Box::new(AndFunction),
        Box::new(OrFunction),
        Box::new(NotFunction),
        Box::new(FormatNumberFunction),
        Box::new(FormatCurrencyFunction),
        Box::new(FormatDateFunction),
        Box::new(FormatStringFunction),
        Box::new(PluralizeFunction),
        Box::new(OpenUrlFunction),
    ]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::data_model::DataModel;
    use serde_json::json;

    /// Build a minimal DataContext backed by an empty DataModel and no functions.
    fn empty_context() -> DataContext<'static> {
        // Safety: we leak the DataModel and HashMap to obtain 'static references.
        // This is acceptable only within tests.
        let dm = Box::leak(Box::new(DataModel::new()));
        let fns = Box::leak(Box::new(HashMap::new()));
        DataContext::new(dm, fns)
    }

    /// Build a DataContext backed by a DataModel containing the given JSON value.
    fn context_with_data(data: Value) -> DataContext<'static> {
        let dm = Box::leak(Box::new(DataModel::from_value(data)));
        let fns = Box::leak(Box::new(HashMap::new()));
        DataContext::new(dm, fns)
    }

    /// Build a DataContext with basic functions registered (for formatString function call tests).
    fn context_with_functions(data: Value) -> DataContext<'static> {
        use crate::core::catalog::function_api::FunctionImplementation;
        let dm = Box::leak(Box::new(DataModel::from_value(data)));
        let fns_map: HashMap<String, Box<dyn FunctionImplementation>> = build_basic_functions()
            .into_iter()
            .map(|f| (f.name().to_string(), f))
            .collect();
        let fns = Box::leak(Box::new(fns_map));
        DataContext::new(dm, fns)
    }

    // ---- required ----

    #[test]
    fn test_required_string() {
        let ctx = empty_context();
        let f = RequiredFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("hello"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));

        args.insert("value".into(), json!(""));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));

        args.insert("value".into(), Value::Null);
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    #[test]
    fn test_required_array() {
        let ctx = empty_context();
        let f = RequiredFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!([1, 2, 3]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));

        args.insert("value".into(), json!([]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- regex ----

    #[test]
    fn test_regex_match() {
        let ctx = empty_context();
        let f = RegexFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("hello123"));
        args.insert("pattern".into(), json!("^[a-z]+[0-9]+$"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));

        args.insert("value".into(), json!("HELLO"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- length ----

    #[test]
    fn test_length_bounds() {
        let ctx = empty_context();
        let f = LengthFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("abc"));
        args.insert("min".into(), json!(2));
        args.insert("max".into(), json!(5));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));

        args.insert("value".into(), json!("a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));

        args.insert("value".into(), json!("abcdef"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    #[test]
    fn test_length_no_bounds() {
        let ctx = empty_context();
        let f = LengthFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("anything"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    // ---- numeric ----

    #[test]
    fn test_numeric_valid() {
        let ctx = empty_context();
        let f = NumericFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(42));
        args.insert("min".into(), json!(0));
        args.insert("max".into(), json!(100));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    #[test]
    fn test_numeric_string_value() {
        let ctx = empty_context();
        let f = NumericFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("3.14"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    #[test]
    fn test_numeric_invalid_string() {
        let ctx = empty_context();
        let f = NumericFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("not a number"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    #[test]
    fn test_numeric_out_of_range() {
        let ctx = empty_context();
        let f = NumericFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(200));
        args.insert("max".into(), json!(100));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- email ----

    #[test]
    fn test_email_valid() {
        let ctx = empty_context();
        let f = EmailFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("user@example.com"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    #[test]
    fn test_email_invalid() {
        let ctx = empty_context();
        let f = EmailFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("not-an-email"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));

        args.insert("value".into(), json!("@missing-local.com"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- and ----

    #[test]
    fn test_and_all_true() {
        let ctx = empty_context();
        let f = AndFunction;

        let mut args = HashMap::new();
        args.insert("values".into(), json!([true, true, true]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    #[test]
    fn test_and_with_false() {
        let ctx = empty_context();
        let f = AndFunction;

        let mut args = HashMap::new();
        args.insert("values".into(), json!([true, false, true]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- or ----

    #[test]
    fn test_or_any_true() {
        let ctx = empty_context();
        let f = OrFunction;

        let mut args = HashMap::new();
        args.insert("values".into(), json!([false, true, false]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    #[test]
    fn test_or_all_false() {
        let ctx = empty_context();
        let f = OrFunction;

        let mut args = HashMap::new();
        args.insert("values".into(), json!([false, false, false]));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));
    }

    // ---- not ----

    #[test]
    fn test_not() {
        let ctx = empty_context();
        let f = NotFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(true));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(false));

        args.insert("value".into(), json!(false));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!(true));
    }

    // ---- formatNumber ----

    #[test]
    fn test_format_number_basic() {
        let ctx = empty_context();
        let f = FormatNumberFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(1234567.89));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("1,234,567.89"));
    }

    #[test]
    fn test_format_number_no_grouping() {
        let ctx = empty_context();
        let f = FormatNumberFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(1234567));
        args.insert("grouping".into(), json!(false));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("1234567"));
    }

    #[test]
    fn test_format_number_decimals() {
        let ctx = empty_context();
        let f = FormatNumberFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(std::f64::consts::PI));
        args.insert("decimals".into(), json!(2));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("3.14"));
    }

    #[test]
    fn test_format_number_negative() {
        let ctx = empty_context();
        let f = FormatNumberFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(-1234.5));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("-1,234.5"));
    }

    // ---- formatCurrency ----

    #[test]
    fn test_format_currency() {
        let ctx = empty_context();
        let f = FormatCurrencyFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(1234.56));
        args.insert("currency".into(), json!("USD"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("USD 1,234.56"));
    }

    // ---- formatDate ----

    #[test]
    fn test_format_date_full() {
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-03-15T14:30:00"));
        args.insert("format".into(), json!("yyyy-MM-dd HH:mm:ss"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("2024-03-15 14:30:00"));
    }

    #[test]
    fn test_format_date_weekday() {
        let ctx = empty_context();
        let f = FormatDateFunction;

        // 2024-03-15 is a Friday
        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-03-15T00:00:00"));
        args.insert("format".into(), json!("E yyyy-MM-dd"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("Fri 2024-03-15"));
    }

    #[test]
    fn test_format_date_month_name() {
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-12-25T10:00:00"));
        args.insert("format".into(), json!("MMM dd, yyyy"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("Dec 25, 2024"));
    }

    #[test]
    fn test_format_date_time_only() {
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-01-01T09:05:03"));
        args.insert("format".into(), json!("HH:mm:ss"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("09:05:03"));
    }

    // ---- formatDate 12-hour and AM/PM ----

    #[test]
    fn test_format_date_12h_midnight() {
        // Midnight (00:00) should show 12 AM
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-01-01T00:00:00"));
        args.insert("format".into(), json!("h:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("12:00 AM"));
    }

    #[test]
    fn test_format_date_12h_noon() {
        // Noon (12:00) should show 12 PM
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-06-15T12:00:00"));
        args.insert("format".into(), json!("h:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("12:00 PM"));
    }

    #[test]
    fn test_format_date_12h_afternoon() {
        // 15:30 (3:30 PM)
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-03-15T15:30:00"));
        args.insert("format".into(), json!("h:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("3:30 PM"));
    }

    #[test]
    fn test_format_date_12h_morning() {
        // 09:05 (9:05 AM)
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-03-15T09:05:00"));
        args.insert("format".into(), json!("h:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("9:05 AM"));
    }

    #[test]
    fn test_format_date_hh_leading_zero() {
        // 09:00 should show 09 with 'hh'
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-03-15T09:00:00"));
        args.insert("format".into(), json!("hh:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("09:00 AM"));
    }

    #[test]
    fn test_format_date_hh_midnight_leading_zero() {
        // Midnight should show 12 with 'hh'
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-01-01T00:30:00"));
        args.insert("format".into(), json!("hh:mm a"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("12:30 AM"));
    }

    #[test]
    fn test_format_date_12h_full_format() {
        // Full format combining 12-hour with date
        let ctx = empty_context();
        let f = FormatDateFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("2024-12-25T14:30:00"));
        args.insert("format".into(), json!("MMM dd, yyyy hh:mm a"));
        assert_eq!(
            f.execute(&args, &ctx).unwrap(),
            json!("Dec 25, 2024 02:30 PM")
        );
    }

    // ---- formatString ----

    #[test]
    fn test_format_string_data_path() {
        let ctx = context_with_data(json!({"user": {"name": "Alice"}}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("Hello, ${/user/name}!"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("Hello, Alice!"));
    }

    #[test]
    fn test_format_string_escape() {
        let ctx = empty_context();
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("escaped: \\${literal}"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("escaped: ${literal}"));
    }

    #[test]
    fn test_format_string_mixed() {
        let ctx = context_with_data(json!({"greeting": "Hello", "target": "World"}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("${/greeting}, ${/target}!"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("Hello, World!"));
    }

    // ---- formatString with nested function calls ----

    #[test]
    fn test_format_string_function_call_format_date() {
        // Call formatDate inside formatString with string literal args
        let ctx = context_with_functions(json!({"event": {"date": "2024-03-15T15:30:00"}}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert(
            "value".into(),
            json!("The event is at ${formatDate(value:${/event/date}, format:'h:mm a')}"),
        );
        assert_eq!(
            f.execute(&args, &ctx).unwrap(),
            json!("The event is at 3:30 PM")
        );
    }

    #[test]
    fn test_format_string_function_call_format_number() {
        // Call formatNumber inside formatString
        let ctx = context_with_functions(json!({"price": 1234.5}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert(
            "value".into(),
            json!("Price: ${formatNumber(value:${/price}, grouping:false)}"),
        );
        assert_eq!(
            f.execute(&args, &ctx).unwrap(),
            json!("Price: 1234.5")
        );
    }

    #[test]
    fn test_format_string_function_call_pluralize() {
        // Call pluralize inside formatString
        let ctx = context_with_functions(json!({"count": 5}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert(
            "value".into(),
            json!("${pluralize(value:${/count}, one:'item', other:'items')}"),
        );
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("items"));
    }

    #[test]
    fn test_format_string_unknown_function() {
        // Unknown function should resolve to empty string
        let ctx = context_with_functions(json!({}));
        let f = FormatStringFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!("result: ${unknownFunc()}"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("result: "));
    }

    // ---- pluralize ----

    #[test]
    fn test_pluralize_one() {
        let ctx = empty_context();
        let f = PluralizeFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(1));
        args.insert("one".into(), json!("item"));
        args.insert("other".into(), json!("items"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("item"));
    }

    #[test]
    fn test_pluralize_other() {
        let ctx = empty_context();
        let f = PluralizeFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(5));
        args.insert("one".into(), json!("item"));
        args.insert("other".into(), json!("items"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("items"));
    }

    #[test]
    fn test_pluralize_zero() {
        let ctx = empty_context();
        let f = PluralizeFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(0));
        args.insert("zero".into(), json!("no items"));
        args.insert("one".into(), json!("item"));
        args.insert("other".into(), json!("items"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("no items"));
    }

    #[test]
    fn test_pluralize_zero_fallback() {
        let ctx = empty_context();
        let f = PluralizeFunction;

        let mut args = HashMap::new();
        args.insert("value".into(), json!(0));
        args.insert("one".into(), json!("item"));
        args.insert("other".into(), json!("items"));
        assert_eq!(f.execute(&args, &ctx).unwrap(), json!("items"));
    }

    // ---- openUrl ----

    #[test]
    fn test_open_url_noop() {
        let ctx = empty_context();
        let f = OpenUrlFunction;

        let args = HashMap::new();
        assert_eq!(f.execute(&args, &ctx).unwrap(), Value::Null);
    }

    // ---- builder ----

    #[test]
    fn test_build_basic_functions_count() {
        let fns = build_basic_functions();
        assert_eq!(fns.len(), 14);

        let names: Vec<&str> = fns.iter().map(|f| f.name()).collect();
        assert!(names.contains(&"required"));
        assert!(names.contains(&"regex"));
        assert!(names.contains(&"length"));
        assert!(names.contains(&"numeric"));
        assert!(names.contains(&"email"));
        assert!(names.contains(&"and"));
        assert!(names.contains(&"or"));
        assert!(names.contains(&"not"));
        assert!(names.contains(&"formatNumber"));
        assert!(names.contains(&"formatCurrency"));
        assert!(names.contains(&"formatDate"));
        assert!(names.contains(&"formatString"));
        assert!(names.contains(&"pluralize"));
        assert!(names.contains(&"openUrl"));
    }
}
