use chrono::Utc;
use toml::Value;

#[test]
fn deny_exceptions_have_review_metadata() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let deny_toml = std::fs::read_to_string(manifest_dir.join("deny.toml"))
        .expect("deny.toml should be readable");
    let parsed = deny_toml
        .parse::<Value>()
        .expect("deny.toml should parse as TOML");
    let today = Utc::now().date_naive();

    let advisory_ignores = parsed
        .get("advisories")
        .and_then(|value| value.get("ignore"))
        .and_then(Value::as_array)
        .expect("deny.toml should define advisories.ignore");
    assert!(
        !advisory_ignores.is_empty(),
        "expected advisory ignores to be covered by this policy"
    );
    for entry in advisory_ignores {
        let id = required_str(entry, "id", "advisories.ignore");
        assert_review_metadata(id, required_str(entry, "reason", id), today);
    }

    let duplicate_skips = parsed
        .get("bans")
        .and_then(|value| value.get("skip"))
        .and_then(Value::as_array)
        .expect("deny.toml should define bans.skip");
    assert!(
        !duplicate_skips.is_empty(),
        "expected duplicate dependency skips to be covered by this policy"
    );
    for entry in duplicate_skips {
        let crate_id = required_str(entry, "crate", "bans.skip");
        assert_review_metadata(crate_id, required_str(entry, "reason", crate_id), today);
    }
}

fn required_str<'a>(value: &'a Value, key: &str, context: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{context} entry should contain string field {key:?}"))
}

fn assert_review_metadata(context: &str, reason: &str, today: chrono::NaiveDate) {
    let reviewed = field(reason, "reviewed")
        .unwrap_or_else(|| panic!("{context} reason must include `reviewed: YYYY-MM-DD`"));
    parse_date(context, "reviewed", reviewed);

    let next_review = field(reason, "next-review")
        .unwrap_or_else(|| panic!("{context} reason must include `next-review: YYYY-MM-DD`"));
    let next_review = parse_date(context, "next-review", next_review);
    assert!(
        next_review >= today,
        "{context} exception review expired on {next_review}; re-evaluate the dependency tree"
    );

    let upstream = field(reason, "upstream")
        .unwrap_or_else(|| panic!("{context} reason must include `upstream: owner/project`"));
    assert!(
        !upstream.is_empty() && upstream != "unknown",
        "{context} reason must name the upstream dependency path"
    );
}

fn field<'a>(reason: &'a str, name: &str) -> Option<&'a str> {
    reason.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once(':')?;
        (key == name).then(|| value.trim())
    })
}

fn parse_date(context: &str, field: &str, value: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .unwrap_or_else(|err| panic!("{context} {field} date {value:?} is invalid: {err}"))
}
