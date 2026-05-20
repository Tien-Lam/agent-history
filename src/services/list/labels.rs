use crate::federated::LOCAL_SOURCE;
use crate::model::Provider;

use super::ListedSession;

pub fn source_provider_label(source: &str, provider: Provider) -> String {
    if source == LOCAL_SOURCE {
        provider.to_string()
    } else {
        format!("{source}/{provider}")
    }
}

pub fn source_provider_counts(sessions: &[ListedSession]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for listed in sessions {
        let label = source_provider_label(&listed.source, listed.session.provider);
        if let Some((_, count)) = counts.iter_mut().find(|(existing, _)| existing == &label) {
            *count += 1;
        } else {
            counts.push((label, 1));
        }
    }
    counts
}
