/// Word-bounded uppercase `TODO` match. Rejects `Todo`, `todo`, and any
/// occurrence inside `TodoWrite` (Claude Code tool name) or `TodoCreate`.
pub(super) fn contains_todo_keyword(line: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = b"TODO";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after == bytes.len() || !is_ident_char(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
