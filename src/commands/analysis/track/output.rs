use std::io::{self, IsTerminal};

use aghist::cli_error::ErrorEnvelope;
use aghist::llm::TrackEvent;
use aghist::output::write_json_line;

use crate::commands::text::truncate;

pub(super) fn emit_track_output(
    topic: &str,
    sessions_scanned: usize,
    events: &[TrackEvent],
    force_json: bool,
) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let want_json = force_json || !stdout.is_terminal();
    let mut out = stdout.lock();
    if want_json {
        write_track_json(&mut out, topic, sessions_scanned, events)
    } else {
        write_track_human(&mut out, topic, sessions_scanned, events)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write track output", e))
}

fn write_track_json<W: io::Write>(
    out: &mut W,
    topic: &str,
    sessions_scanned: usize,
    events: &[TrackEvent],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "topic": topic,
        "sessions_scanned": sessions_scanned,
        "timeline": events,
    });
    write_json_line(out, &payload)
}

fn write_track_human<W: io::Write>(
    out: &mut W,
    topic: &str,
    sessions_scanned: usize,
    events: &[TrackEvent],
) -> io::Result<()> {
    writeln!(out, "Topic: {topic}")?;
    writeln!(out, "Sessions scanned: {sessions_scanned}")?;
    writeln!(out)?;
    writeln!(
        out,
        "{:<10}  {:<42}  {:<12}  EVENT",
        "DATE", "REF", "DIRECTION"
    )?;
    for ev in events {
        let ref_short = truncate(&ev.session_ref, 42);
        let event_short = truncate(&ev.event, 80);
        writeln!(
            out,
            "{:<10}  {:<42}  {:<12}  {}",
            ev.date, ref_short, ev.direction, event_short
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} event(s)", events.len())
}
