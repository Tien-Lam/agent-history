use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::{Message, Session};
use crate::usage::{pricing_for, round_cents_4};

/// One row per project in the top-projects table. Counts are scoped to the
/// window's bundles. Cost follows the same convention as `aghist usage`:
/// `None` if any constituent session uses an unpriced model.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectActivity {
    pub project: String,
    pub session_count: usize,
    pub message_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
}

/// Group bundles by `project_name`, returning `(total_distinct_projects,
/// top_n)`. Sessions without a project name bucket under `(unknown)`.
/// Rows are sorted by `message_count` desc, ties broken by `session_count`
/// desc and project name asc; deterministic.
pub(super) fn top_projects(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
) -> (usize, Vec<ProjectActivity>) {
    let mut buckets: BTreeMap<String, Vec<&(Session, Vec<Message>)>> = BTreeMap::new();
    for bundle in sessions {
        let name = bundle
            .0
            .project_name
            .clone()
            .unwrap_or_else(|| "(unknown)".to_string());
        buckets.entry(name).or_default().push(bundle);
    }
    let total = buckets.len();
    let mut rows: Vec<ProjectActivity> = buckets
        .into_iter()
        .map(|(project, group)| activity_for(project, &group))
        .collect();
    rows.sort_by(|a, b| {
        b.message_count
            .cmp(&a.message_count)
            .then_with(|| b.session_count.cmp(&a.session_count))
            .then_with(|| a.project.cmp(&b.project))
    });
    if limit > 0 && rows.len() > limit {
        rows.truncate(limit);
    }
    (total, rows)
}

fn activity_for(project: String, group: &[&(Session, Vec<Message>)]) -> ProjectActivity {
    let session_count = group.len();
    let message_count: usize = group.iter().map(|(_, m)| m.len()).sum();
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_r = 0u64;
    let mut cache_w = 0u64;
    let mut cost = 0.0f64;
    let mut has_unpriced = false;
    for (s, _) in group {
        let Some(usage) = s.token_usage.as_ref() else {
            continue;
        };
        input = input.saturating_add(usage.input_tokens);
        output = output.saturating_add(usage.output_tokens);
        cache_r = cache_r.saturating_add(usage.cache_read_tokens.unwrap_or(0));
        cache_w = cache_w.saturating_add(usage.cache_write_tokens.unwrap_or(0));
        match s.model.as_deref().and_then(pricing_for) {
            Some(p) => cost += p.cost_usd(usage),
            None => has_unpriced = true,
        }
    }
    let total_tokens = input
        .saturating_add(output)
        .saturating_add(cache_r)
        .saturating_add(cache_w);
    ProjectActivity {
        project,
        session_count,
        message_count,
        input_tokens: input,
        output_tokens: output,
        total_tokens,
        cost_usd: if has_unpriced {
            None
        } else {
            Some(round_cents_4(cost))
        },
    }
}
