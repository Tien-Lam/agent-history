use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::provider::ProviderError;

const MAX_JSONL_LINE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct JsonlRecord<T> {
    pub line_number: usize,
    pub value: T,
}

pub(crate) struct JsonlError {
    pub line_number: usize,
    pub error: String,
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
    on_record: F,
    on_error: E,
) -> Result<JsonlStats, ProviderError>
where
    T: DeserializeOwned,
    F: FnMut(JsonlRecord<T>),
    E: FnMut(JsonlError),
{
    visit_jsonl_records_with_max_line_bytes(path, MAX_JSONL_LINE_BYTES, on_record, on_error)
}

pub(super) fn visit_jsonl_records_with_max_line_bytes<T, F, E>(
    path: &Path,
    max_line_bytes: usize,
    mut on_record: F,
    mut on_error: E,
) -> Result<JsonlStats, ProviderError>
where
    T: DeserializeOwned,
    F: FnMut(JsonlRecord<T>),
    E: FnMut(JsonlError),
{
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut physical_line_number = 0;
    let mut line_count = 0;
    let mut parse_errors = 0;

    while let Some(line) = read_bounded_line(&mut reader, max_line_bytes)? {
        physical_line_number += 1;
        let line_number = physical_line_number;
        let line = match line {
            BoundedLine::Text(line) => line,
            BoundedLine::TooLong => {
                line_count += 1;
                parse_errors += 1;
                on_error(JsonlError {
                    line_number,
                    error: format!("JSONL line exceeds {max_line_bytes} byte limit"),
                });
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        line_count += 1;
        match serde_json::from_str(&line) {
            Ok(value) => on_record(JsonlRecord { line_number, value }),
            Err(error) => {
                parse_errors += 1;
                on_error(JsonlError {
                    line_number,
                    error: error.to_string(),
                });
            }
        }
    }

    Ok(JsonlStats {
        line_count,
        parse_errors,
    })
}

enum BoundedLine {
    Text(String),
    TooLong,
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    max_line_bytes: usize,
) -> Result<Option<BoundedLine>, ProviderError> {
    let mut bytes = Vec::new();
    let read = reader
        .by_ref()
        .take(max_line_bytes.saturating_add(2) as u64)
        .read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }

    let mut content_len = bytes.len();
    if bytes.ends_with(b"\n") {
        content_len = content_len.saturating_sub(1);
        if content_len > 0 && bytes.get(content_len - 1) == Some(&b'\r') {
            content_len -= 1;
        }
    }

    if content_len > max_line_bytes {
        drain_line(reader)?;
        return Ok(Some(BoundedLine::TooLong));
    }

    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    let line = String::from_utf8(bytes)
        .map_err(|e| ProviderError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    Ok(Some(BoundedLine::Text(line)))
}

fn drain_line<R: BufRead>(reader: &mut R) -> Result<(), ProviderError> {
    let mut discard = Vec::new();
    loop {
        discard.clear();
        let read = reader.read_until(b'\n', &mut discard)?;
        if read == 0 || discard.ends_with(b"\n") {
            return Ok(());
        }
    }
}

pub(crate) fn parse_jsonl_records<T>(path: &Path) -> Result<JsonlScan<T>, ProviderError>
where
    T: DeserializeOwned,
{
    let mut records = Vec::new();
    visit_jsonl_records(path, |record| records.push(record), |_| {})?;

    Ok(JsonlScan { records })
}
