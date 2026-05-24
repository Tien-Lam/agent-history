mod common;

use std::fmt::Write as _;

use assert_cmd::Command;

fn aghist() -> Command {
    let mut cmd = common::helpers::isolated_aghist("cli-help");
    cmd.env("COLUMNS", "120");
    cmd
}

#[test]
fn cli_help_contract_snapshot() {
    let mut snapshot = String::new();
    for (label, args) in HELP_COMMANDS {
        let output = aghist().args(*args).assert().success();
        let stdout = normalize_help(&common::cli::assert_stdout(&output));
        writeln!(snapshot, "===== {label} =====").unwrap();
        snapshot.push_str(&stdout);
        if !snapshot.ends_with('\n') {
            snapshot.push('\n');
        }
        snapshot.push('\n');
    }

    insta::assert_snapshot!("cli_help_contract", snapshot);
}

const HELP_COMMANDS: &[(&str, &[&str])] = &[
    ("root", &["--help"]),
    ("export", &["export", "--help"]),
    ("index", &["index", "--help"]),
    ("search", &["search", "--help"]),
    ("health", &["health", "--help"]),
    ("sources", &["sources", "--help"]),
    ("sources pull", &["sources", "pull", "--help"]),
    ("show", &["show", "--help"]),
    ("diff", &["diff", "--help"]),
    ("track", &["track", "--help"]),
    ("decisions", &["decisions", "--help"]),
    ("todos", &["todos", "--help"]),
    ("threads", &["threads", "--help"]),
    ("mcp", &["mcp", "--help"]),
    ("schema", &["schema", "--help"]),
    ("note", &["note", "--help"]),
    ("note add", &["note", "add", "--help"]),
    ("tag", &["tag", "--help"]),
    ("tag add", &["tag", "add", "--help"]),
    ("stars", &["stars", "--help"]),
    ("usage", &["usage", "--help"]),
    ("project", &["project", "--help"]),
    ("report", &["report", "--help"]),
    ("update", &["update", "--help"]),
    ("uninstall", &["uninstall", "--help"]),
];

fn normalize_help(text: &str) -> String {
    text.replace("\r\n", "\n")
}
