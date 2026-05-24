pub(super) fn best_snippet(
    content: &str,
    tool_output: &str,
    query: &str,
    max_len: usize,
) -> String {
    // Prefer whichever stored field actually contains the query, so a hit on
    // tool_output doesn't return an empty/unrelated snippet from content.
    let q_lower = query.to_lowercase();
    if tool_output.to_lowercase().contains(&q_lower) {
        make_snippet(tool_output, query, max_len)
    } else if !content.is_empty() {
        make_snippet(content, query, max_len)
    } else {
        make_snippet(tool_output, query, max_len)
    }
}

fn make_snippet(content: &str, query: &str, max_len: usize) -> String {
    let pos = find_case_insensitive(content, query).unwrap_or(0);
    let mut start = pos.saturating_sub(max_len / 2);
    let mut end = (start + max_len).min(content.len());

    while start > 0 && !content.is_char_boundary(start) {
        start -= 1;
    }
    while end < content.len() && !content.is_char_boundary(end) {
        end += 1;
    }

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(&content[start..end]);
    if end < content.len() {
        snippet.push_str("...");
    }
    snippet.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_case_insensitive(content: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    if content.is_ascii() && query.is_ascii() {
        return content
            .to_ascii_lowercase()
            .find(&query.to_ascii_lowercase());
    }

    let query = query.to_lowercase();
    let mut folded = String::with_capacity(content.len());
    let mut folded_to_original = Vec::with_capacity(content.len());
    for (original_index, ch) in content.char_indices() {
        for lower in ch.to_lowercase() {
            let mut buf = [0u8; 4];
            let encoded = lower.encode_utf8(&mut buf);
            folded_to_original.extend(std::iter::repeat_n(original_index, encoded.len()));
            folded.push(lower);
        }
    }

    folded
        .find(&query)
        .and_then(|index| folded_to_original.get(index).copied())
}

#[cfg(test)]
mod tests {
    use super::{find_case_insensitive, make_snippet};

    #[test]
    fn finds_ascii_case_insensitive_offsets() {
        assert_eq!(find_case_insensitive("alpha Needle", "needle"), Some(6));
    }

    #[test]
    fn maps_unicode_case_fold_expansion_back_to_original_offsets() {
        let content = "İstanbul needle";

        assert_eq!(
            find_case_insensitive(content, "needle"),
            Some("İstanbul ".len())
        );
    }

    #[test]
    fn snippet_does_not_panic_after_case_fold_expansion() {
        let content = "İİİİİİ needle";

        let snippet = make_snippet(content, "needle", 12);

        assert!(snippet.contains("needle"));
    }
}
