use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::LlmError;

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    content: Vec<ApiContentBlock>,
}

#[derive(Deserialize)]
struct ApiContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

pub(crate) fn response_json_object(body: &str) -> Result<String, LlmError> {
    let resp: ApiResponse = serde_json::from_str(body)
        .map_err(|e| LlmError::Parse(format!("response envelope: {e}")))?;
    let text = resp
        .content
        .into_iter()
        .filter(|b| b.kind == "text")
        .map(|b| b.text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(LlmError::NoJson("empty assistant text".into()));
    }
    extract_json_object(&text)
        .map(str::to_string)
        .ok_or_else(|| LlmError::NoJson(text.chars().take(200).collect()))
}

pub(crate) fn response_payload<T>(body: &str, label: &str) -> Result<T, LlmError>
where
    T: DeserializeOwned,
{
    let json_slice = response_json_object(body)?;
    serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "{label}: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })
}

/// Find the first balanced `{...}` object in `text`. Returns `None` if no
/// balanced object is present. Skips over braces that appear inside double-
/// quoted strings (with `\\` escape handling) so JSON-with-prose still parses.
pub(crate) fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}
