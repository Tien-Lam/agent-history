/// Parse a 1-based inclusive turn range against a session of `total` messages.
///
/// Accepts `A:B`, `:B`, `A:`, or a bare `A`. Empty halves default to the
/// session bounds (`1` and `total`). Bounds are clamped to the available
/// range so callers can do `--turn-range :999` without failing.
///
/// Returns `(start, end)` with `1 <= start <= end <= total`, ready to be
/// converted to a 0-based half-open slice via `start-1 .. end`.
pub(super) fn parse_turn_range(spec: &str, total: usize) -> Result<(usize, usize), String> {
    if total == 0 {
        return Err("session has no messages to slice".to_string());
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(format!("empty turn range '{spec}'"));
    }

    let parse_bound = |s: &str, label: &str| -> Result<Option<usize>, String> {
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<usize>()
            .map(Some)
            .map_err(|_| format!("invalid {label} '{s}' in turn range '{spec}'"))
    };

    let (start_raw, end_raw) = if let Some((a, b)) = trimmed.split_once(':') {
        (parse_bound(a, "start")?, parse_bound(b, "end")?)
    } else {
        let n =
            parse_bound(trimmed, "turn")?.ok_or_else(|| format!("empty turn range '{spec}'"))?;
        (Some(n), Some(n))
    };

    if start_raw == Some(0) || end_raw == Some(0) {
        return Err(format!("turn range '{spec}' uses 0 (turns are 1-based)"));
    }

    let start = start_raw.unwrap_or(1);
    let end = end_raw.unwrap_or(total).min(total);

    if start > total {
        return Err(format!(
            "turn range '{spec}' starts at {start} but session only has {total} message(s)"
        ));
    }
    if end < start {
        return Err(format!(
            "turn range '{spec}' has end ({end}) before start ({start})"
        ));
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::parse_turn_range;

    #[test]
    fn full_range() {
        assert_eq!(parse_turn_range("3:7", 10).unwrap(), (3, 7));
    }

    #[test]
    fn open_start_defaults_to_one() {
        assert_eq!(parse_turn_range(":5", 10).unwrap(), (1, 5));
    }

    #[test]
    fn open_end_defaults_to_total() {
        assert_eq!(parse_turn_range("4:", 10).unwrap(), (4, 10));
    }

    #[test]
    fn single_turn() {
        assert_eq!(parse_turn_range("7", 10).unwrap(), (7, 7));
    }

    #[test]
    fn end_clamps_to_total() {
        assert_eq!(parse_turn_range("3:999", 10).unwrap(), (3, 10));
    }

    #[test]
    fn empty_session_rejects() {
        assert!(parse_turn_range("1:1", 0).is_err());
    }

    #[test]
    fn zero_rejected() {
        assert!(parse_turn_range("0:5", 10).is_err());
        assert!(parse_turn_range("3:0", 10).is_err());
        assert!(parse_turn_range("0", 10).is_err());
    }

    #[test]
    fn start_past_end_rejects() {
        assert!(parse_turn_range("8:3", 10).is_err());
    }

    #[test]
    fn start_past_session_rejects() {
        assert!(parse_turn_range("99:100", 10).is_err());
    }

    #[test]
    fn non_numeric_rejects() {
        assert!(parse_turn_range("a:b", 10).is_err());
        assert!(parse_turn_range("abc", 10).is_err());
    }

    #[test]
    fn empty_spec_rejects() {
        assert!(parse_turn_range("", 10).is_err());
    }
}
