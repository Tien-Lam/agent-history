use super::ModelPricing;

/// Hand-curated price list. Keys are *prefixes*: lookup matches the
/// longest entry that is a prefix of the model id, so undated and dated
/// model ids (`claude-sonnet-4-5` vs `claude-sonnet-4-5-20250929`)
/// resolve to the same rate without per-snapshot maintenance.
///
/// Sources: Anthropic public pricing page (claude-*), `OpenAI` pricing
/// page (gpt-*, o-series), Google Vertex/AI Studio (gemini-*) as of
/// the snapshot below. These are *list* prices; volume discounts and
/// batch-API rates are out of scope.
///
/// Keep this list small and conservative. Unknown models report
/// `cost_usd: null` rather than guessing; silent guesses become silent
/// bugs in finance reports.
pub(super) const PRICING_TABLE: &[(&str, ModelPricing)] = &[
    // Claude 4.x family (cache rates: write = base * 1.25, read = base * 0.10).
    (
        "claude-opus-4",
        ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.5),
            cache_write_per_mtok: Some(18.75),
        },
    ),
    (
        "claude-sonnet-4",
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
    ),
    (
        "claude-haiku-4",
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cache_read_per_mtok: Some(0.1),
            cache_write_per_mtok: Some(1.25),
        },
    ),
    // Claude 3.x family.
    (
        "claude-3-5-sonnet",
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
    ),
    (
        "claude-3-5-haiku",
        ModelPricing {
            input_per_mtok: 0.8,
            output_per_mtok: 4.0,
            cache_read_per_mtok: Some(0.08),
            cache_write_per_mtok: Some(1.0),
        },
    ),
    (
        "claude-3-opus",
        ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.5),
            cache_write_per_mtok: Some(18.75),
        },
    ),
    (
        "claude-3-haiku",
        ModelPricing {
            input_per_mtok: 0.25,
            output_per_mtok: 1.25,
            cache_read_per_mtok: Some(0.03),
            cache_write_per_mtok: Some(0.3),
        },
    ),
    // GPT-4o / GPT-4 family. Cache read is OpenAI's "cached input" rate.
    (
        "gpt-4o-mini",
        ModelPricing {
            input_per_mtok: 0.15,
            output_per_mtok: 0.6,
            cache_read_per_mtok: Some(0.075),
            cache_write_per_mtok: None,
        },
    ),
    (
        "gpt-4o",
        ModelPricing {
            input_per_mtok: 2.5,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(1.25),
            cache_write_per_mtok: None,
        },
    ),
    (
        "gpt-4-turbo",
        ModelPricing {
            input_per_mtok: 10.0,
            output_per_mtok: 30.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    // Gemini 1.5 / 2.x family.
    (
        "gemini-2.5-pro",
        ModelPricing {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-2.5-flash",
        ModelPricing {
            input_per_mtok: 0.3,
            output_per_mtok: 2.5,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-1.5-pro",
        ModelPricing {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
    (
        "gemini-1.5-flash",
        ModelPricing {
            input_per_mtok: 0.075,
            output_per_mtok: 0.3,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
    ),
];
