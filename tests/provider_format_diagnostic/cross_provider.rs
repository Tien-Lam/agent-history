use super::fixture_provider_set;

#[test]
fn all_fixture_providers_roundtrip() {
    let providers = fixture_provider_set();

    for (label, provider) in &providers {
        let sessions = provider
            .discover_sessions()
            .unwrap_or_else(|e| panic!("{label}: discover_sessions failed: {e}"));

        for session in &sessions {
            let messages = provider.load_messages(session).unwrap_or_else(|e| {
                panic!("{label}: load_messages failed for {}: {e}", session.id.0)
            });

            eprintln!(
                "[{label}] session={} discovered_count={} loaded_count={}",
                session.id.0,
                session.message_count,
                messages.len()
            );

            if session.message_count > 0 {
                assert!(
                    !messages.is_empty(),
                    "[{label}] session {} has message_count={} but load_messages returned 0 - \
                     this indicates a format mismatch between discover and load",
                    session.id.0,
                    session.message_count,
                );
            }
        }
    }
}
