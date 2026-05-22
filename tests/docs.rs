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
