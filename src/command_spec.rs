/// Stable, machine-readable command registry shared by schema generation and
/// CLI-facing documentation. Command execution still lives in the binary, but
/// discoverable command names now come from one table instead of hand-written
/// arrays in each surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
}

/// Order matches the public help output and `aghist schema --list`.
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
