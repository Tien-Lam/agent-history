use std::process::Output;

use assert_cmd::assert::Assert;
use serde_json::Value;

pub fn assert_stdout(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is valid UTF-8")
}

pub fn assert_stderr(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is valid UTF-8")
}

pub fn output_stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is valid UTF-8")
}

pub fn output_stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is valid UTF-8")
}

pub fn assert_stdout_json(assert: &Assert) -> Value {
    parse_json(&assert_stdout(assert), "stdout")
}

pub fn output_stdout_json(output: &Output) -> Value {
    parse_json(&output_stdout(output), "stdout")
}

pub fn assert_stderr_error(assert: &Assert) -> Value {
    parse_error_envelope(&assert_stderr(assert))
}

pub fn output_stderr_error(output: &Output) -> Value {
    parse_error_envelope(&output_stderr(output))
}

pub fn output_ndjson_values(output: &Output) -> Vec<Value> {
    parse_ndjson(&output_stdout(output))
}

pub fn output_ndjson_session_rows(output: &Output) -> Vec<Value> {
    output_ndjson_values(output)
        .into_iter()
        .filter(|value| value.get("id").is_some())
        .collect()
}

fn parse_json(text: &str, stream_name: &str) -> Value {
    serde_json::from_str(text.trim())
        .unwrap_or_else(|err| panic!("expected JSON {stream_name}, got {text:?}: {err}"))
}

fn parse_error_envelope(stderr: &str) -> Value {
    let line = stderr
        .lines()
        .find(|line| line.starts_with('{'))
        .unwrap_or_else(|| panic!("expected JSON envelope on stderr, got {stderr:?}"));
    parse_json(line, "error envelope")
}

fn parse_ndjson(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|err| panic!("expected NDJSON line, got {line:?}: {err}"))
        })
        .collect()
}
