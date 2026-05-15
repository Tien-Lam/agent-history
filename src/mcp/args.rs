use serde_json::Value;

use crate::model::Provider;

pub(super) fn required_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing required string argument: {key}"))
}

pub(super) fn optional_str(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("argument '{key}' must be a string, got: {other}")),
    }
}

pub(super) fn optional_usize(
    args: &Value,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let raw = match args.get(key) {
        None | Some(Value::Null) => return Ok(default),
        Some(v) => v,
    };
    let n = raw
        .as_u64()
        .ok_or_else(|| format!("argument '{key}' must be a non-negative integer"))?;
    let n = usize::try_from(n).map_err(|_| format!("argument '{key}' is too large"))?;
    if n < min || n > max {
        return Err(format!(
            "argument '{key}' must be in [{min}, {max}], got {n}"
        ));
    }
    Ok(n)
}

pub(super) fn optional_provider(args: &Value, key: &str) -> Result<Option<Provider>, String> {
    let Some(slug) = optional_str(args, key)? else {
        return Ok(None);
    };
    Provider::from_slug(&slug)
        .map(Some)
        .ok_or_else(|| format!("unknown provider slug '{slug}'"))
}
