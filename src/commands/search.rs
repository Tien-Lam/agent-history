use anyhow::Result;

use crate::cli::SearchArgs;
use crate::filter::SessionFilter;
use crate::model::MessageRole;
use crate::output::{self, SearchMatch, SearchResult};
use crate::provider::registry::Registry;

pub fn run(args: &SearchArgs, registry: &Registry) -> Result<()> {
    let filter = SessionFilter {
        provider: args.tool.clone(),
        project: args.project.clone(),
        since: parse_date_filter(args.since.as_deref())?,
        limit: None,
    };

    let sessions = registry.discover_sessions(&filter)?;
    let query_lower = args.query.to_lowercase();
    let mut results = Vec::new();

    for session in &sessions {
        let messages = match registry.load_messages(session) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("warning: skipping {}: {e}", session.id.short());
                continue;
            }
        };

        let mut matches = Vec::new();
        let lines: Vec<(MessageRole, String)> = messages
            .iter()
            .flat_map(|m| {
                m.content
                    .lines()
                    .map(|l| (m.role, l.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect();

        for (i, (role, line)) in lines.iter().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                let context_start = i.saturating_sub(args.context);
                let context_end = (i + args.context + 1).min(lines.len());

                matches.push(SearchMatch {
                    message_index: i,
                    role: *role,
                    line: line.clone(),
                    context_before: lines[context_start..i]
                        .iter()
                        .map(|(_, l)| l.clone())
                        .collect(),
                    context_after: lines[i + 1..context_end]
                        .iter()
                        .map(|(_, l)| l.clone())
                        .collect(),
                });
            }
        }

        if !matches.is_empty() {
            results.push(SearchResult { session, matches });
        }
    }

    let formatted = output::format_search_results(&results);
    print!("{formatted}");
    Ok(())
}

fn parse_date_filter(s: Option<&str>) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(s) = s else { return Ok(None) };

    let naive = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid date '{s}': {e}"))?;
    let dt = naive.and_hms_opt(0, 0, 0).expect("valid time").and_utc();
    Ok(Some(dt))
}
