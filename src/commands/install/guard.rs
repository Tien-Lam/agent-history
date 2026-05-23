use std::path::Path;

use aghist::cli_error::ErrorEnvelope;

use super::source::{detect_install_source, InstallSource};

#[derive(Debug, Clone, Copy)]
pub(super) enum InstallOperation {
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

pub(super) fn ensure_self_managed_install(
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
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::commands::install::source::install_marker_path;

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
        let marker = install_marker_path(&exe).unwrap();
        std::fs::write(marker, "method=github-release\n").unwrap();

        ensure_self_managed_install(&exe, InstallOperation::Update).unwrap();
        ensure_self_managed_install(&exe, InstallOperation::Uninstall).unwrap();
    }

    #[test]
    fn release_marker_path_uses_binary_stem() {
        let root = tempfile::tempdir().unwrap();
        let exe = exe_path(root.path());

        assert_eq!(
            install_marker_path(&exe).unwrap(),
            root.path().join("aghist.install")
        );
    }
}
