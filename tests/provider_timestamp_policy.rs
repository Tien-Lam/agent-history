use std::path::Path;

#[test]
fn provider_parsers_do_not_use_wall_clock_timestamp_fallbacks() {
    let provider_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provider");
    let mut offenders = Vec::new();
    collect_wall_clock_fallbacks(&provider_root, &provider_root, &mut offenders);

    assert!(
        offenders.is_empty(),
        "provider parsers must use source-derived or deterministic fallback timestamps, not wall-clock time:\n{}",
        offenders.join("\n")
    );
}

fn collect_wall_clock_fallbacks(root: &Path, path: &Path, offenders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_wall_clock_fallbacks(root, &path, offenders);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        scan_file(root, &path, offenders);
    }
}

fn scan_file(root: &Path, path: &Path, offenders: &mut Vec<String>) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for (line_idx, line) in contents.lines().enumerate() {
        if line.contains("Utc::now()")
            || line.contains("chrono::Utc::now()")
            || line.contains("unwrap_or_else(Utc::now)")
            || line.contains("unwrap_or_else(chrono::Utc::now)")
        {
            let rel = path.strip_prefix(root).unwrap_or(path);
            offenders.push(format!("{}:{}", rel.display(), line_idx + 1));
        }
    }
}
