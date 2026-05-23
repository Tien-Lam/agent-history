/// (lowercase needle, weight, display label).
///
/// Needles are matched as case-insensitive substrings. Order is irrelevant:
/// we sum every match. Where a needle includes a trailing space, that's
/// a deliberate word-boundary guard ("we will " avoids matching "we willingly").
const PATTERNS: &[(&str, f32, &str)] = &[
    // Explicit decision language (high signal).
    ("we decided", 5.0, "we decided"),
    ("decided to", 5.0, "decided to"),
    ("decision:", 5.0, "decision:"),
    ("the decision is", 5.0, "the decision is"),
    ("agreed to", 5.0, "agreed to"),
    ("agreed that", 5.0, "agreed that"),
    ("chose to", 4.0, "chose to"),
    ("going with", 4.0, "going with"),
    ("settled on", 4.0, "settled on"),
    // Plans / commitments.
    ("we will ", 3.0, "we will"),
    ("we won't", 3.0, "we won't"),
    ("we won\u{2019}t", 3.0, "we won't"),
    ("we'll ", 3.0, "we'll"),
    ("we\u{2019}ll ", 3.0, "we'll"),
    ("we shall", 3.0, "we shall"),
    ("we should ", 2.0, "we should"),
    // Comparative choice.
    ("instead of", 3.0, "instead of"),
    ("rather than", 3.0, "rather than"),
    // Soft markers only push borderline cases over the threshold.
    ("let's ", 2.0, "let's"),
    ("let us ", 2.0, "let us"),
    ("should ", 1.0, "should"),
    ("because ", 1.0, "because"),
];

pub(super) struct Scored {
    pub(super) score: f32,
    pub(super) markers: Vec<String>,
}

pub(super) fn score_sentence(sentence: &str) -> Scored {
    let lower = sentence.to_lowercase();
    let mut score = 0.0;
    let mut markers: Vec<String> = Vec::new();
    for (needle, weight, label) in PATTERNS {
        if lower.contains(needle) {
            score += weight;
            let label = (*label).to_string();
            if !markers.contains(&label) {
                markers.push(label);
            }
        }
    }
    Scored { score, markers }
}
