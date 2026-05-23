use std::collections::BTreeMap;
use std::path::Path;

use aghist::model::Provider;

const REPO_URL: &str = "https://github.com/Tien-Lam/agent-history";

fn repo_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|err| panic!("failed to read {path}: {err}"))
}

#[test]
fn changelog_release_links_match_version_history() {
    let changelog = repo_file("CHANGELOG.md");
    let versions = changelog_versions(&changelog);
    let links = changelog_link_defs(&changelog);

    assert!(!versions.is_empty(), "CHANGELOG.md has no release headings");

    for version in &versions {
        assert!(
            links.contains_key(version),
            "CHANGELOG.md heading [{version}] is missing a link definition",
        );
    }

    for version in links.keys() {
        assert!(
            versions.contains(version),
            "CHANGELOG.md link definition [{version}] has no matching heading",
        );
    }

    for pair in versions.windows(2) {
        let current = &pair[0];
        let previous = &pair[1];
        let expected = format!("{REPO_URL}/compare/v{previous}...v{current}");
        assert_eq!(
            links.get(current).map(String::as_str),
            Some(expected.as_str()),
            "CHANGELOG.md link for [{current}] should compare against the previous release",
        );
    }

    let oldest = versions.last().expect("checked non-empty versions");
    let expected = format!("{REPO_URL}/releases/tag/v{oldest}");
    assert_eq!(
        links.get(oldest).map(String::as_str),
        Some(expected.as_str()),
        "CHANGELOG.md link for the first release should point to its tag",
    );
}

#[test]
fn changelog_top_release_matches_cargo_version() {
    let changelog = repo_file("CHANGELOG.md");
    let cargo_toml = repo_file("Cargo.toml");
    let versions = changelog_versions(&changelog);
    let cargo_version = cargo_package_version(&cargo_toml);

    assert_eq!(
        versions.first().map(String::as_str),
        Some(cargo_version.as_str()),
        "top CHANGELOG.md release should match Cargo.toml package.version",
    );
}

#[test]
fn readme_supported_providers_match_registered_providers() {
    let readme = repo_file("README.md");

    for provider in Provider::all() {
        let display_name = provider.as_str();
        assert!(
            readme.contains(&format!("**{display_name}**")),
            "README.md Supported Providers is missing {display_name}",
        );
    }
}

#[test]
fn markdown_local_links_resolve() {
    for path in [
        "README.md",
        "CLAUDE.md",
        "CHANGELOG.md",
        "docs/ARCHITECTURE.md",
    ] {
        let markdown = repo_file(path);
        for (line_idx, line) in markdown.lines().enumerate() {
            for target in markdown_link_targets(line) {
                if is_external_link(&target) {
                    continue;
                }

                let (path_part, anchor) = split_link_target(&target);
                let linked_path = if path_part.is_empty() {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
                } else {
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(path)
                        .parent()
                        .expect("markdown path has a parent")
                        .join(path_part)
                };

                assert!(
                    linked_path.exists(),
                    "{path}:{} links to missing local target {target:?}",
                    line_idx + 1,
                );

                if let Some(anchor) = anchor {
                    let linked_markdown =
                        std::fs::read_to_string(&linked_path).unwrap_or_else(|err| {
                            panic!(
                                "{path}:{} failed to read linked markdown {}: {err}",
                                line_idx + 1,
                                linked_path.display()
                            )
                        });
                    assert!(
                        markdown_anchors(&linked_markdown)
                            .iter()
                            .any(|candidate| candidate == anchor),
                        "{path}:{} links to missing anchor #{anchor} in {}",
                        line_idx + 1,
                        linked_path.display(),
                    );
                }
            }
        }
    }
}

fn changelog_versions(changelog: &str) -> Vec<String> {
    changelog
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("## [")?;
            let (version, _) = rest.split_once(']')?;
            Some(version.to_string())
        })
        .collect()
}

fn markdown_link_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        targets.push(rest[..end].trim().to_string());
        rest = &rest[end + 1..];
    }
    targets
}

fn is_external_link(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.contains("://")
}

fn split_link_target(target: &str) -> (&str, Option<&str>) {
    let bare = target.split_whitespace().next().unwrap_or(target);
    match bare.split_once('#') {
        Some((path, anchor)) if !anchor.is_empty() => (path, Some(anchor)),
        Some((path, _)) => (path, None),
        None => (bare, None),
    }
}

fn markdown_anchors(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(heading_anchor)
        .collect::<Vec<_>>()
}

fn heading_anchor(line: &str) -> Option<String> {
    let heading = line.trim_start().strip_prefix('#')?;
    let heading = heading.trim_start_matches('#').trim();
    if heading.is_empty() {
        return None;
    }

    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in heading.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if (ch.is_ascii_whitespace() || ch == '-') && !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    Some(slug)
}

fn changelog_link_defs(changelog: &str) -> BTreeMap<String, String> {
    changelog
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let (version, url) = rest.split_once("]: ")?;
            Some((version.to_string(), url.to_string()))
        })
        .collect()
}

fn cargo_package_version(cargo_toml: &str) -> String {
    cargo_toml
        .lines()
        .skip_while(|line| line.trim() != "[package]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| {
            line.trim()
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string)
        })
        .expect("Cargo.toml [package] section has no version")
}
