const MAX_SENTENCE_BYTES: usize = 4 * 1024;

/// Split a text blob into sentences on `.`/`!`/`?`/newline boundaries.
///
/// Cheap and naive: doesn't try to handle abbreviations or ellipses, and
/// preserves the terminator with its sentence so snippets read naturally.
/// Overlong sentences are truncated to keep pathological inputs (e.g.
/// minified JSON dumped into a chat) from blowing the snippet column.
pub(super) fn split_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.len();
    for (i, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let end = i + ch.len_utf8();
            push_segment(text, start, end, &mut out);
            start = end;
        }
    }
    if start < bytes {
        push_segment(text, start, bytes, &mut out);
    }
    out
}

fn push_segment<'a>(text: &'a str, start: usize, end: usize, out: &mut Vec<&'a str>) {
    let seg = &text[start..end];
    if seg.trim().is_empty() {
        return;
    }
    let bounded = if seg.len() > MAX_SENTENCE_BYTES {
        // Find the last char boundary at or below the limit so we don't
        // panic on a multi-byte boundary.
        let mut cut = MAX_SENTENCE_BYTES;
        while cut > 0 && !seg.is_char_boundary(cut) {
            cut -= 1;
        }
        &seg[..cut]
    } else {
        seg
    };
    out.push(bounded);
}

pub(super) fn clip(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}
