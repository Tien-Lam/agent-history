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
        let before_ok = i == 0 || !is_word_char(bytes[i - 1]);
        if !before_ok || !bytes[i].is_ascii_lowercase() {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i].is_ascii_lowercase() {
            i += 1;
        }
        let prefix_len = i - prefix_start;
        if prefix_len < 2 || i >= bytes.len() || bytes[i] != b'-' {
            continue;
        }
        let suffix_start = i + 1;
        let mut j = suffix_start;
        while j < bytes.len() {
            let c = bytes[j];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' {
                j += 1;
            } else {
                break;
            }
        }
        while j > suffix_start && bytes[j - 1] == b'.' {
            j -= 1;
        }
        let after_ok = j == bytes.len() || !is_word_char(bytes[j]);
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
