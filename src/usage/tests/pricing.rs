use super::*;

#[test]
fn pricing_for_known_prefix_returns_rate() {
    let p = pricing_for("claude-sonnet-4-5-20250929").unwrap();
    assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
    assert!((p.output_per_mtok - 15.0).abs() < f64::EPSILON);
}

#[test]
fn pricing_for_longer_prefix_wins() {
    let p = pricing_for("claude-3-5-sonnet-20240620").unwrap();
    assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
}

#[test]
fn pricing_for_unknown_returns_none() {
    assert!(pricing_for("totally-imaginary-model").is_none());
    assert!(pricing_for("").is_none());
}

#[test]
fn cost_usd_includes_input_output_and_cache() {
    let p = pricing_for("claude-sonnet-4-5").unwrap();
    let usage = TokenUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        cache_read_tokens: Some(1_000_000),
        cache_write_tokens: Some(1_000_000),
    };
    let cost = p.cost_usd(&usage);
    assert!((cost - 22.05).abs() < 1e-9, "got {cost}");
}

#[test]
fn cost_usd_skips_cache_when_no_rate_available() {
    let p = pricing_for("gpt-4-turbo").unwrap();
    let usage = TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: Some(1_000_000),
        cache_write_tokens: Some(1_000_000),
    };
    assert!(p.cost_usd(&usage).abs() < f64::EPSILON);
}
