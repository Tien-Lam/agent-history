use std::io;
use std::path::{Path, PathBuf};

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{config, search};

const INSTALL_MARKER_METHOD: &str = "method=github-release";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallSource {
    GithubRelease,
    Cargo,
    BuildTree,
    SystemPackage,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
enum InstallOperation {
    Update,
    Uninstall,
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
    let config_path = config::Config::config_path();
    let config_dir = config_path.as_deref().and_then(|p| p.parent());

    eprintln!("This will remove:");
    eprintln!("  binary:       {}", exe.display());
    if index_dir.exists() {
        eprintln!("  search index: {}", index_dir.display());
    }
    if let Some(dir) = config_dir {
        if dir.exists() {
            eprintln!("  config:       {}", dir.display());
        }
    }

    eprint!("\nContinue? [y/N] ");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to read confirmation: {e}")))?;
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
    if let Some(dir) = config_dir {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to remove {}: {e}", dir.display()),
                )
            })?;
            eprintln!("Removed {}", dir.display());
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
        println!("Updated to v{}", status.version());
    } else {
        println!("Already up to date (v{})", status.version());
    }
    Ok(EXIT_OK)
}

#[cfg(not(feature = "self-update"))]
fn self_update_impl() -> Result<i32, ErrorEnvelope> {
    Err(ErrorEnvelope::new(
        "unsupported-install-method",
        "this aghist binary was built without self-update support",
    )
    .with_hint(
        "Use your installer to update, or install a GitHub release via install.sh/manual download.",
    ))
}

fn current_exe() -> Result<PathBuf, ErrorEnvelope> {
    std::env::current_exe()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("current_exe failed: {e}")))
}

fn ensure_self_managed_install(
    exe: &Path,
    operation: InstallOperation,
) -> Result<(), ErrorEnvelope> {
    match detect_install_source(exe) {
        InstallSource::GithubRelease | InstallSource::Unknown => Ok(()),
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

fn detect_install_source(exe: &Path) -> InstallSource {
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    detect_install_source_with(exe, cargo_home.as_deref(), home.as_deref())
}

fn detect_install_source_with(
    exe: &Path,
    cargo_home: Option<&Path>,
    home: Option<&Path>,
) -> InstallSource {
    if has_github_release_marker(exe) {
        return InstallSource::GithubRelease;
    }

    if is_build_tree_path(exe) {
        return InstallSource::BuildTree;
    }

    if is_cargo_bin_path(exe, cargo_home, home) {
        return InstallSource::Cargo;
    }

    if is_system_package_path(exe) {
        return InstallSource::SystemPackage;
    }

    InstallSource::Unknown
}

fn has_github_release_marker(exe: &Path) -> bool {
    install_marker_path(exe)
        .and_then(|marker| std::fs::read_to_string(marker).ok())
        .is_some_and(|contents| {
            contents
                .lines()
                .any(|line| line.trim() == INSTALL_MARKER_METHOD)
        })
}

fn install_marker_path(exe: &Path) -> Option<PathBuf> {
    let mut marker_name = exe.file_stem()?.to_os_string();
    marker_name.push(".install");
    Some(exe.with_file_name(marker_name))
}

fn is_build_tree_path(exe: &Path) -> bool {
    let components = path_components(exe);
    components.iter().enumerate().any(|(idx, component)| {
        *component == "target"
            && components
                .iter()
                .skip(idx + 1)
                .take(3)
                .any(|next| matches!(next.as_str(), "debug" | "release"))
    })
}

fn is_cargo_bin_path(exe: &Path, cargo_home: Option<&Path>, home: Option<&Path>) -> bool {
    cargo_home.is_some_and(|dir| exe.starts_with(dir.join("bin")))
        || home.is_some_and(|dir| exe.starts_with(dir.join(".cargo").join("bin")))
}

fn is_system_package_path(exe: &Path) -> bool {
    let path = normalized_path(exe);
    path.starts_with("/usr/bin/")
        || path.starts_with("/bin/")
        || path.starts_with("/usr/sbin/")
        || path.starts_with("/sbin/")
        || path.starts_with("/nix/store/")
        || path.starts_with("/snap/")
        || path.starts_with("/var/lib/snapd/snap/")
        || path.starts_with("/opt/homebrew/bin/")
        || path.starts_with("/opt/homebrew/Cellar/")
        || path.starts_with("/home/linuxbrew/.linuxbrew/bin/")
        || path.starts_with("/home/linuxbrew/.linuxbrew/Cellar/")
        || path.contains("/Cellar/")
        || path.contains("/scoop/apps/")
        || path.contains("/scoop/shims/")
        || path.contains("/Chocolatey/bin/")
}

fn path_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests;
