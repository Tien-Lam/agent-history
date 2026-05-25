use std::collections::hash_map::Entry;
use std::collections::HashMap;

use aghist::cli_error::ErrorEnvelope;
use aghist::model::{Provider, SessionId};

pub(super) type SessionGroupKey = (String, Provider, SessionId);

pub(super) fn llm_config_from_env(
    llm_model: Option<&str>,
) -> Result<aghist::llm::LlmConfig, ErrorEnvelope> {
    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config
            .with_model(model.to_string())
            .map_err(|e| map_llm_error(&e))?;
    }
    Ok(config)
}

pub(super) fn ordered_session_groups<Row, Group>(
    rows: Vec<Row>,
    mut key_for: impl FnMut(&Row) -> SessionGroupKey,
    mut init: impl FnMut(&Row) -> Group,
    mut push: impl FnMut(&mut Group, Row),
) -> Vec<Group> {
    let mut order: Vec<SessionGroupKey> = Vec::new();
    let mut grouped: HashMap<SessionGroupKey, Group> = HashMap::new();

    for row in rows {
        let key = key_for(&row);
        match grouped.entry(key.clone()) {
            Entry::Occupied(mut entry) => push(entry.get_mut(), row),
            Entry::Vacant(entry) => {
                order.push(key);
                let mut group = init(&row);
                push(&mut group, row);
                entry.insert(group);
            }
        }
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        if let Some(group) = grouped.remove(&key) {
            out.push(group);
        }
    }
    out
}

pub(super) fn map_llm_error(e: &aghist::llm::LlmError) -> ErrorEnvelope {
    use aghist::llm::LlmError;
    let env = ErrorEnvelope::new("llm-error", e.to_string());
    match e {
        LlmError::MissingApiKey => {
            env.with_hint("Set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) and re-run.")
        }
        LlmError::ApiStatus {
            status: 401 | 403, ..
        } => env.with_hint("Verify ANTHROPIC_API_KEY is valid and has access to the chosen model."),
        LlmError::ApiStatus { status: 429, .. } => {
            env.with_hint("Rate limited — retry with --limit lowered or wait and retry.")
        }
        _ => env,
    }
}
