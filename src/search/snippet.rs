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
    let lower = content.to_lowercase();
    let q = query.to_lowercase();

    let pos = lower.find(&q).unwrap_or(0);
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
