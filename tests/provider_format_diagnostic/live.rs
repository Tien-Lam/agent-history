#[test]
fn live_data_diagnostic() {
    if std::env::var("AGHIST_LIVE_TEST").is_err() {
        eprintln!("Skipping live_data_diagnostic (set AGHIST_LIVE_TEST=1 to run)");
        return;
    }

    let providers = aghist::provider::detect_all_providers();

    for provider in &providers {
        let sessions = match provider.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[{}] DISCOVER ERROR: {e}", provider.provider());
                continue;
            }
        };

        eprintln!(
            "[{}] discovered {} sessions",
            provider.provider(),
            sessions.len()
        );

        let mut loaded = 0;
        let mut empty = 0;
        let mut errors = 0;

        for session in sessions.iter().take(5) {
            match provider.load_messages(session) {
                Ok(msgs) => {
                    if msgs.is_empty() {
                        empty += 1;
                        eprintln!(
                            "  EMPTY: session={} message_count={} source={}",
                            session.id.0,
                            session.message_count,
                            session.source_path.display()
                        );
                    } else {
                        loaded += 1;
                        let roles: Vec<_> = msgs.iter().map(|m| format!("{:?}", m.role)).collect();
                        eprintln!(
                            "  OK:    session={} loaded={} roles=[{}]",
                            session.id.0,
                            msgs.len(),
                            roles.join(", ")
                        );
                    }
                }
                Err(e) => {
                    errors += 1;
                    eprintln!("  ERROR: session={} error={e}", session.id.0);
                }
            }
        }

        eprintln!(
            "[{}] sample results: loaded={loaded}, empty={empty}, errors={errors}",
            provider.provider()
        );
    }
}
