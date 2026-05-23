use chrono::Timelike;

use crate::model::{Message, Session};

pub(super) fn collect_projects(sessions: &[(Session, Vec<Message>)]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for (s, _) in sessions {
        if let Some(name) = &s.project_name {
            if !seen.iter().any(|p| p == name) {
                seen.push(name.clone());
            }
        }
    }
    seen.sort();
    seen
}

pub(super) fn time_of_day_histogram(sessions: &[(Session, Vec<Message>)]) -> [u64; 24] {
    let mut hist = [0u64; 24];
    for (_, msgs) in sessions {
        for msg in msgs {
            let hour = msg.timestamp.hour() as usize;
            if hour < 24 {
                hist[hour] = hist[hour].saturating_add(1);
            }
        }
    }
    hist
}
