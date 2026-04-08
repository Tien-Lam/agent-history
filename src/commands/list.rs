use anyhow::Result;

use crate::cli::ListArgs;
use crate::filter::SessionFilter;
use crate::output;
use crate::provider::registry::Registry;

pub fn run(args: &ListArgs, registry: &Registry) -> Result<()> {
    let filter = SessionFilter {
        provider: args.tool.clone(),
        project: args.project.clone(),
        since: parse_date_filter(args.since.as_deref())?,
        limit: Some(args.limit),
    };

    let sessions = registry.discover_sessions(&filter)?;
    let formatted = output::format_sessions(&sessions, args.format);
    println!("{formatted}");
    Ok(())
}

fn parse_date_filter(s: Option<&str>) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(s) = s else { return Ok(None) };

    let naive = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid date '{s}': {e}"))?;
    let dt = naive.and_hms_opt(0, 0, 0).expect("valid time").and_utc();
    Ok(Some(dt))
}
