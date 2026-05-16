/// Stable, machine-readable registry for commands that expose JSON schemas.
///
/// Command execution still lives in the binary. This table is the schema
/// discovery contract, not a complete mirror of clap's top-level help:
/// `list` is exposed as the legacy `--list` flag, while lifecycle commands
/// such as `update` and `uninstall` deliberately have no schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
}

/// Order is the stable `aghist schema --list` discovery order.
pub const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "list",
        summary: "List sessions without opening the TUI.",
    },
    CommandSpec {
        name: "search",
        summary: "Search indexed sessions for a query.",
    },
    CommandSpec {
        name: "show",
        summary: "Resolve a citation ref to one message.",
    },
    CommandSpec {
        name: "export",
        summary: "Export a session to Markdown, JSON, or HTML.",
    },
    CommandSpec {
        name: "diff",
        summary: "Compare two sessions turn-by-turn.",
    },
    CommandSpec {
        name: "index",
        summary: "Build or refresh the search index.",
    },
    CommandSpec {
        name: "sources",
        summary: "Inspect or manage local and remote provider sources.",
    },
    CommandSpec {
        name: "health",
        summary: "Validate index, manifest, and provider state.",
    },
    CommandSpec {
        name: "mcp",
        summary: "Run a stdio MCP server.",
    },
    CommandSpec {
        name: "schema",
        summary: "Emit JSON-Schema for an aghist command.",
    },
    CommandSpec {
        name: "decisions",
        summary: "Extract candidate architectural decisions.",
    },
    CommandSpec {
        name: "todos",
        summary: "Surface unresolved TODOs and follow-ups.",
    },
    CommandSpec {
        name: "threads",
        summary: "Cluster sessions into related work threads.",
    },
    CommandSpec {
        name: "track",
        summary: "Track how a topic evolved across sessions.",
    },
    CommandSpec {
        name: "note",
        summary: "Manage per-user notes.",
    },
    CommandSpec {
        name: "tag",
        summary: "Manage per-user tags.",
    },
    CommandSpec {
        name: "star",
        summary: "Mark a session or turn as starred.",
    },
    CommandSpec {
        name: "unstar",
        summary: "Remove a star.",
    },
    CommandSpec {
        name: "stars",
        summary: "List starred sessions and turns.",
    },
    CommandSpec {
        name: "usage",
        summary: "Aggregate token usage and cost.",
    },
    CommandSpec {
        name: "project",
        summary: "Build a per-project productivity dashboard.",
    },
    CommandSpec {
        name: "report",
        summary: "Build a cross-project activity report.",
    },
];

pub fn command_names() -> impl Iterator<Item = &'static str> {
    COMMAND_SPECS.iter().map(|spec| spec.name)
}

pub fn command_spec(name: &str) -> Option<&'static CommandSpec> {
    COMMAND_SPECS.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_unique() {
        let mut names: Vec<&str> = command_names().collect();
        let len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), len);
    }
}
