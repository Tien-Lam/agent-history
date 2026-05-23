use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::provider::ProviderError;

pub(crate) struct JsonlRecord<T> {
    pub line_number: usize,
    pub value: T,
}

pub(crate) struct JsonlError {
    pub line_number: usize,
    pub error: serde_json::Error,
}

pub(crate) struct JsonlScan<T> {
    pub records: Vec<JsonlRecord<T>>,
}

pub(crate) struct JsonlStats {
    pub line_count: usize,
    pub parse_errors: usize,
}

pub(crate) fn visit_jsonl_records<T, F, E>(
    path: &Path,
    mut on_record: F,
    mut on_error: E,
) -> Result<JsonlStats, ProviderError>
where
    T: DeserializeOwned,
    F: FnMut(JsonlRecord<T>),
    E: FnMut(JsonlError),
{
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut line_count = 0;
    let mut parse_errors = 0;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let line_number = idx + 1;
        line_count += 1;
        match serde_json::from_str(&line) {
            Ok(value) => on_record(JsonlRecord { line_number, value }),
            Err(error) => {
                parse_errors += 1;
                on_error(JsonlError { line_number, error });
            }
        }
    }

    Ok(JsonlStats {
        line_count,
        parse_errors,
    })
}

pub(crate) fn parse_jsonl_records<T>(path: &Path) -> Result<JsonlScan<T>, ProviderError>
where
    T: DeserializeOwned,
{
    let mut records = Vec::new();
    visit_jsonl_records(path, |record| records.push(record), |_| {})?;

    Ok(JsonlScan { records })
}
