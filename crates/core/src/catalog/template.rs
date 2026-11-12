//! Fallible released-protocol interpolation. Legacy permissive formatting stays separate.
use crate::{
    error::A2uiError,
    model::data_context::{DataContext, value_to_string},
};
use serde_json::Value;
use std::collections::HashMap;

fn error(message: &str) -> A2uiError {
    A2uiError::InvalidFunctionCall(format!("formatString: {message}"))
}

pub(super) fn interpolate(input: &str, context: &DataContext) -> Result<String, A2uiError> {
    let mut out = String::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(next) = rest.strip_prefix("\\${") {
            out.push_str("${");
            rest = next;
        } else if let Some(next) = rest.strip_prefix("${") {
            let mut depth = 0usize;
            let mut quote = None;
            let mut escaped = false;
            let mut end = None;
            for (i, c) in next.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                    continue;
                }
                if c == '\'' || c == '"' {
                    quote = Some(c);
                    continue;
                }
                match c {
                    '{' => depth += 1,
                    '}' if depth == 0 => {
                        end = Some(i);
                        break;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            let end = end.ok_or_else(|| error("unterminated interpolation"))?;
            out.push_str(&value_to_string(&expression(&next[..end], context, 0)?));
            rest = &next[end + 1..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(out)
}

fn expression(input: &str, context: &DataContext, depth: usize) -> Result<Value, A2uiError> {
    if depth > 64 {
        return Err(error("expression nesting limit exceeded"));
    }
    let input = input.trim();
    if (input.starts_with('\'') && input.ends_with('\''))
        || (input.starts_with('"') && input.ends_with('"'))
    {
        if input.len() < 2 {
            return Err(error("unterminated string"));
        }
        let mut result = String::new();
        let mut escaped = false;
        for c in input[1..input.len() - 1].chars() {
            if escaped {
                result.push(match c {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => c,
                });
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                result.push(c);
            }
        }
        if escaped {
            return Err(error("unterminated escape"));
        }
        return Ok(Value::String(result));
    }
    if let Ok(value) = serde_json::from_str::<Value>(input) {
        if !value.is_object() && !value.is_array() {
            return Ok(value);
        }
    }
    if let Some(inner) = input.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        return expression(inner, context, depth + 1);
    }
    if let Some(open) = input.find('(') {
        if !input.ends_with(')') {
            return Err(error("unterminated function call"));
        }
        let name = input[..open].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '@')
        {
            return Err(error("invalid function name"));
        }
        let mut args = HashMap::new();
        for part in split_args(&input[open + 1..input.len() - 1])? {
            if part.trim().is_empty() {
                continue;
            }
            let (key, value) = part
                .split_once(':')
                .or_else(|| part.split_once('='))
                .ok_or_else(|| error("argument needs a name"))?;
            if args
                .insert(
                    key.trim().to_owned(),
                    expression(value, context, depth + 1)?,
                )
                .is_some()
            {
                return Err(error("duplicate argument"));
            }
        }
        return context.try_call_function_by_name(name, &args);
    }
    Ok(context.get(input).unwrap_or(Value::Null))
}

fn split_args(input: &str) -> Result<Vec<&str>, A2uiError> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| error("unbalanced expression"))?;
            }
            ',' if depth == 0 => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return Err(error("unterminated argument"));
    }
    result.push(&input[start..]);
    Ok(result)
}
