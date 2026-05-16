use std::path::PathBuf;

use tempfile::TempDir;

/// Holds a temp directory and the base path to pass to a provider constructor.
/// The `TempDir` must be kept alive for the duration of the test.
pub struct FixtureDir {
    pub dir: TempDir,
    pub base_path: PathBuf,
}
