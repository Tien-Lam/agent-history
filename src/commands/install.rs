use std::io;
#[cfg(feature = "self-update")]
use std::io::Write as _;
use std::path::{Path, PathBuf};

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{config, search};

mod source;

use source::{detect_install_source, InstallSource};

#[derive(Debug, Clone, Copy)]
enum InstallOperation {
    Update,
    Uninstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemovalTarget {
    Directory(PathBuf),
    File(PathBuf),
}

impl InstallOperation {
    fn verb(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Uninstall => "uninstall",
        }
    }
}

pub(crate) fn uninstall() -> Result<i32, ErrorEnvelope> {
    let exe = current_exe()?;
    ensure_self_managed_install(&exe, InstallOperation::Uninstall)?;

    let index_dir = search::SearchIndex::default_index_dir();
    let config_path = config::Config::resolved_path();
    let config_target = config_removal_target(config_path.as_deref());
    let marker = release_install_marker_path(&exe);

    eprintln!("This will remove:");
    eprintln!("  binary:       {}", exe.display());
    if let Some(marker) = &marker {
        if marker.exists() {
            eprintln!("  marker:       {}", marker.display());
        }
    }
    if index_dir.exists() {
        eprintln!("  search index: {}", index_dir.display());
    }
    if let Some(target) = &config_target {
        eprintln!("  config:       {}", target.display_path().display());
    }

    eprint!("\nContinue? [y/N] ");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ErrorEnvelope::io("failed to read confirmation", e))?;
    if !input.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        return Err(ErrorEnvelope::new("aborted", "uninstall cancelled by user"));
    }

    if index_dir.exists() {
        std::fs::remove_dir_all(&index_dir).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", index_dir.display()),
            )
        })?;
        eprintln!("Removed {}", index_dir.display());
    }
    if let Some(target) = &config_target {
        target.remove().map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", target.display_path().display()),
            )
        })?;
        eprintln!("Removed {}", target.display_path().display());
    }

    if let Some(marker) = marker {
        if marker.exists() {
            std::fs::remove_file(&marker).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to remove {}: {e}", marker.display()),
                )
            })?;
            eprintln!("Removed {}", marker.display());
        }
    }

    // On Windows, self-delete requires renaming first.
    #[cfg(windows)]
    {
        let tmp = exe.with_extension("old");
        std::fs::rename(&exe, &tmp).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!(
                    "failed to rename {} -> {}: {e}",
                    exe.display(),
                    tmp.display()
                ),
            )
        })?;
        if let Err(e) = std::process::Command::new("cmd")
            .args(["/C", "timeout", "/t", "2", "/nobreak", ">nul", "&", "del"])
            .arg(&tmp)
            .spawn()
        {
            eprintln!(
                "warning: could not schedule cleanup of {}: {e}",
                tmp.display()
            );
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(&exe).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", exe.display()),
            )
        })?;
    }

    eprintln!("aghist has been uninstalled.");
    Ok(EXIT_OK)
}

pub(crate) fn self_update() -> Result<i32, ErrorEnvelope> {
    let exe = current_exe()?;
    ensure_self_managed_install(&exe, InstallOperation::Update)?;
    self_update_impl()
}

#[cfg(feature = "self-update")]
fn self_update_impl() -> Result<i32, ErrorEnvelope> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("Tien-Lam")
        .repo_name("agent-history")
        .bin_name("aghist")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(self_update::cargo_crate_version!())
        .build()
        .map_err(|e| {
            ErrorEnvelope::new("update-failed", format!("failed to configure updater: {e}"))
        })?
        .update()
        .map_err(|e| ErrorEnvelope::new("update-failed", format!("update failed: {e}")))?;

    if status.updated() {
        writeln!(io::stdout().lock(), "Updated to v{}", status.version())
            .map_err(|e| ErrorEnvelope::io("failed to write update output", e))?;
    } else {
        writeln!(
            io::stdout().lock(),
            "Already up to date (v{})",
            status.version()
        )
        .map_err(|e| ErrorEnvelope::io("failed to write update output", e))?;
    }
    Ok(EXIT_OK)
}

#[cfg(not(feature = "self-update"))]
fn self_update_impl() -> Result<i32, ErrorEnvelope> {
    Err(ErrorEnvelope::new(
        "unsupported-install-method",
        "this aghist binary was built without self-update support",
    )
    .with_hint("Use your installer to update, or install a GitHub release via install.sh."))
}

fn current_exe() -> Result<PathBuf, ErrorEnvelope> {
    std::env::current_exe().map_err(|e| ErrorEnvelope::io("current_exe failed", e))
}

fn release_install_marker_path(exe: &Path) -> Option<PathBuf> {
    let mut marker_name = exe.file_stem()?.to_os_string();
    marker_name.push(".install");
    Some(exe.with_file_name(marker_name))
}

fn config_removal_target(config_path: Option<&Path>) -> Option<RemovalTarget> {
    let default_config_path = config::Config::config_path();
    config_removal_target_for(config_path, default_config_path.as_deref())
}

fn config_removal_target_for(
    config_path: Option<&Path>,
    default_config_path: Option<&Path>,
) -> Option<RemovalTarget> {
    let path = config_path?;
    if Some(path) == default_config_path {
        let dir = path.parent()?;
        if dir.exists() {
            return Some(RemovalTarget::Directory(dir.to_path_buf()));
        }
        return None;
    }
    if path.exists() {
        Some(RemovalTarget::File(path.to_path_buf()))
    } else {
        None
    }
}

impl RemovalTarget {
    fn display_path(&self) -> &Path {
        match self {
            Self::Directory(path) | Self::File(path) => path,
        }
    }

    fn remove(&self) -> std::io::Result<()> {
        match self {
            Self::Directory(path) => std::fs::remove_dir_all(path),
            Self::File(path) => std::fs::remove_file(path),
        }
    }
}

fn ensure_self_managed_install(
    exe: &Path,
    operation: InstallOperation,
) -> Result<(), ErrorEnvelope> {
    match detect_install_source(exe) {
        InstallSource::GithubRelease => Ok(()),
        InstallSource::Unknown => Err(unsupported_install_method(
            operation,
            "this aghist binary does not have aghist's release install marker",
            "Reinstall with `install.sh`, or use the installer/package manager that owns this binary.",
        )),
        InstallSource::Cargo => Err(unsupported_install_method(
            operation,
            "this aghist binary is installed under Cargo's bin directory",
            "Use `cargo binstall aghist --force`, `cargo install --git https://github.com/Tien-Lam/agent-history.git --force`, or your Cargo package manager.",
        )),
        InstallSource::BuildTree => Err(unsupported_install_method(
            operation,
            "this aghist binary is running from a Cargo build directory",
            "Run the command against an installed release binary, or rebuild from source.",
        )),
        InstallSource::SystemPackage => Err(unsupported_install_method(
            operation,
            "this aghist binary appears to be managed by a system package manager",
            "Use the package manager that installed aghist; self-managed commands only operate on release binaries.",
        )),
    }
}

fn unsupported_install_method(
    operation: InstallOperation,
    reason: &'static str,
    hint: &'static str,
) -> ErrorEnvelope {
    ErrorEnvelope::new(
        "unsupported-install-method",
        format!("cannot {} aghist: {reason}", operation.verb()),
    )
    .with_hint(hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe_path(dir: &Path) -> PathBuf {
        dir.join(if cfg!(windows) {
            "aghist.exe"
        } else {
            "aghist"
        })
    }

    #[test]
    fn self_managed_operations_require_release_marker() {
        let root = tempfile::tempdir().unwrap();
        let exe = exe_path(root.path());

        let err = ensure_self_managed_install(&exe, InstallOperation::Update).unwrap_err();

        assert_eq!(err.kind, "unsupported-install-method");
        assert!(
            err.message
                .contains("does not have aghist's release install marker"),
            "{}",
            err.message
        );
    }

    #[test]
    fn release_marker_allows_self_managed_operations() {
        let root = tempfile::tempdir().unwrap();
        let exe = exe_path(root.path());
        let marker = release_install_marker_path(&exe).unwrap();
        std::fs::write(marker, "method=github-release\n").unwrap();

        ensure_self_managed_install(&exe, InstallOperation::Update).unwrap();
        ensure_self_managed_install(&exe, InstallOperation::Uninstall).unwrap();
    }

    #[test]
    fn release_marker_path_uses_binary_stem() {
        let root = tempfile::tempdir().unwrap();
        let exe = exe_path(root.path());

        assert_eq!(
            release_install_marker_path(&exe).unwrap(),
            root.path().join("aghist.install")
        );
    }

    #[test]
    fn default_config_target_removes_aghist_config_directory() {
        let root = tempfile::tempdir().unwrap();
        let config_dir = root.path().join("aghist");
        let config_path = config_dir.join("config.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&config_path, "cache_size = 20\n").unwrap();

        let target = config_removal_target_for(Some(&config_path), Some(&config_path))
            .expect("default config dir should be targeted");

        assert_eq!(target, RemovalTarget::Directory(config_dir));
    }

    #[test]
    fn override_config_target_removes_only_config_file() {
        let root = tempfile::tempdir().unwrap();
        let shared_dir = root.path().join("shared");
        let config_path = shared_dir.join("aghist.toml");
        let keep_path = shared_dir.join("keep.txt");
        let default_config_path = root.path().join("default").join("config.toml");
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(&config_path, "cache_size = 20\n").unwrap();
        std::fs::write(&keep_path, "keep\n").unwrap();

        let target = config_removal_target_for(Some(&config_path), Some(&default_config_path))
            .expect("existing override config file should be targeted");

        assert_eq!(target, RemovalTarget::File(config_path.clone()));
        target.remove().unwrap();
        assert!(!config_path.exists());
        assert!(shared_dir.exists());
        assert!(keep_path.exists());
    }

    #[test]
    fn missing_override_config_has_no_removal_target() {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("shared").join("aghist.toml");
        let default_config_path = root.path().join("default").join("config.toml");

        let target = config_removal_target_for(Some(&config_path), Some(&default_config_path));

        assert_eq!(target, None);
    }
}
