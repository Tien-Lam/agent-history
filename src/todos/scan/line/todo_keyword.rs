/// Word-bounded uppercase `TODO` match. Rejects `Todo`, `todo`, and any
/// occurrence inside `TodoWrite` (Claude Code tool name) or `TodoCreate`.
pub(super) fn contains_todo_keyword(line: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = b"TODO";
    for (i, window) in bytes.windows(needle.len()).enumerate() {
        if window == needle {
            let before_ok = i == 0 || bytes.get(i - 1).is_none_or(|byte| !is_ident_char(*byte));
            let after = i + needle.len();
            let after_ok = bytes.get(after).is_none_or(|byte| !is_ident_char(*byte));
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
