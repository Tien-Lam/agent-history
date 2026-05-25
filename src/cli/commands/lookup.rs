mod diff;
mod export;
mod index;
mod search;
mod show;

pub(crate) use diff::DiffCommand;
pub(crate) use export::ExportCommand;
pub(crate) use index::IndexCommand;
pub(crate) use search::SearchCommand;
pub(crate) use show::ShowCommand;

fn parse_reference_selector(raw: &str) -> Result<String, String> {
    if raw.len() > aghist::schema_fragments::REFERENCE_MAX_BYTES {
        return Err(format!(
            "reference selector must be at most {} bytes",
            aghist::schema_fragments::REFERENCE_MAX_BYTES
        ));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aghist::schema_fragments::REFERENCE_MAX_BYTES;

    #[test]
    fn parse_reference_selector_rejects_oversized_values() {
        let raw = "s".repeat(REFERENCE_MAX_BYTES + 1);
        let err = parse_reference_selector(&raw).unwrap_err();
        assert!(err.contains(&REFERENCE_MAX_BYTES.to_string()));
    }
}
