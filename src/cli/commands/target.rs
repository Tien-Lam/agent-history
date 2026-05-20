use super::analysis::{DecisionsCommand, ThreadsCommand, TodosCommand, TrackCommand};
use super::lookup::{DiffCommand, ExportCommand, IndexCommand, SearchCommand, ShowCommand};
use super::metadata::{NoteCommand, SourcesCommand, TagCommand};
use super::reports::{ProjectCommand, ReportCommand, UsageCommand};
use super::Command;

pub(crate) enum CommandTarget {
    ContextFree(ContextFreeCommand),
    Mcp,
    Context(ContextCommand),
}

pub(crate) enum ContextFreeCommand {
    Schema {
        subcommand: Option<String>,
        list: bool,
        all: bool,
    },
    Update,
    Uninstall,
}

pub(crate) enum ContextCommand {
    Lookup(LookupCommand),
    Analysis(AnalysisCommand),
    Metadata(MetadataCommand),
    Reports(ReportsCommand),
}

pub(crate) enum LookupCommand {
    Export(ExportCommand),
    Index(IndexCommand),
    Search(SearchCommand),
    Show(ShowCommand),
    Diff(DiffCommand),
}

pub(crate) enum AnalysisCommand {
    Track(TrackCommand),
    Decisions(DecisionsCommand),
    Todos(TodosCommand),
    Threads(ThreadsCommand),
}

pub(crate) enum MetadataCommand {
    Sources {
        command: Option<SourcesCommand>,
    },
    Health,
    Note {
        command: NoteCommand,
    },
    Tag {
        command: TagCommand,
    },
    Star {
        reference: String,
    },
    Unstar {
        reference: String,
    },
    Stars {
        reference: Option<String>,
        json: bool,
    },
}

pub(crate) enum ReportsCommand {
    Usage(UsageCommand),
    Project(ProjectCommand),
    Report(ReportCommand),
}

impl From<Command> for CommandTarget {
    fn from(command: Command) -> Self {
        match command {
            Command::Export(args) => {
                Self::Context(ContextCommand::Lookup(LookupCommand::Export(args)))
            }
            Command::Index(args) => {
                Self::Context(ContextCommand::Lookup(LookupCommand::Index(args)))
            }
            Command::Search(args) => {
                Self::Context(ContextCommand::Lookup(LookupCommand::Search(args)))
            }
            Command::Show(args) => Self::Context(ContextCommand::Lookup(LookupCommand::Show(args))),
            Command::Diff(args) => Self::Context(ContextCommand::Lookup(LookupCommand::Diff(args))),
            Command::Track(args) => {
                Self::Context(ContextCommand::Analysis(AnalysisCommand::Track(args)))
            }
            Command::Decisions(args) => {
                Self::Context(ContextCommand::Analysis(AnalysisCommand::Decisions(args)))
            }
            Command::Todos(args) => {
                Self::Context(ContextCommand::Analysis(AnalysisCommand::Todos(args)))
            }
            Command::Threads(args) => {
                Self::Context(ContextCommand::Analysis(AnalysisCommand::Threads(args)))
            }
            Command::Sources { command } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Sources {
                    command,
                }))
            }
            Command::Health => Self::Context(ContextCommand::Metadata(MetadataCommand::Health)),
            Command::Note { command } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Note { command }))
            }
            Command::Tag { command } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Tag { command }))
            }
            Command::Star { reference } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Star {
                    reference,
                }))
            }
            Command::Unstar { reference } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Unstar {
                    reference,
                }))
            }
            Command::Stars { reference, json } => {
                Self::Context(ContextCommand::Metadata(MetadataCommand::Stars {
                    reference,
                    json,
                }))
            }
            Command::Usage(args) => {
                Self::Context(ContextCommand::Reports(ReportsCommand::Usage(args)))
            }
            Command::Project(args) => {
                Self::Context(ContextCommand::Reports(ReportsCommand::Project(args)))
            }
            Command::Report(args) => {
                Self::Context(ContextCommand::Reports(ReportsCommand::Report(args)))
            }
            Command::Mcp => Self::Mcp,
            Command::Schema {
                subcommand,
                list,
                all,
            } => Self::ContextFree(ContextFreeCommand::Schema {
                subcommand,
                list,
                all,
            }),
            Command::Update => Self::ContextFree(ContextFreeCommand::Update),
            Command::Uninstall => Self::ContextFree(ContextFreeCommand::Uninstall),
        }
    }
}
