/// Find beads-style refs in a line.
///
/// Pattern: `[a-z]{2,}-[a-z0-9.]+` with at least one digit in the suffix.
/// Word-bounded: must not be preceded or followed by another word/dash
/// character. The digit requirement filters out `follow-up`, `come-back-to`,
/// and similar prose hyphenates while still catching `ahist-y3o.7.2`,
/// `gt-abc1`, `pr-1234`, etc.
pub(super) fn find_bd_refs(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let prefix_start = i;
        let before_ok = i == 0 || bytes.get(i - 1).is_none_or(|byte| !is_word_char(*byte));
        let Some(&current) = bytes.get(i) else {
            break;
        };
        if !before_ok || !current.is_ascii_lowercase() {
            i += 1;
            continue;
        }
        while bytes.get(i).is_some_and(u8::is_ascii_lowercase) {
            i += 1;
        }
        let prefix_len = i - prefix_start;
        if prefix_len < 2 || bytes.get(i) != Some(&b'-') {
            continue;
        }
        let suffix_start = i + 1;
        let mut j = suffix_start;
        while let Some(&c) = bytes.get(j) {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' {
                j += 1;
            } else {
                break;
            }
        }
        while j > suffix_start && bytes.get(j - 1) == Some(&b'.') {
            j -= 1;
        }
        let after_ok = bytes.get(j).is_none_or(|byte| !is_word_char(*byte));
        let suffix = &line[suffix_start..j];
        let has_digit = suffix.bytes().any(|c| c.is_ascii_digit());
        if after_ok && !suffix.is_empty() && has_digit {
            out.push(line[prefix_start..j].to_string());
        }
        i = j.max(i + 1);
    }
    out
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}
