use std::io;
#[cfg(feature = "self-update")]
use std::io::Write as _;
use std::path::PathBuf;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{config, search};

mod cleanup;
mod guard;
mod source;

use cleanup::config_removal_target;
use guard::{ensure_self_managed_install, InstallOperation};
use source::install_marker_path;

pub(crate) fn uninstall() -> Result<i32, ErrorEnvelope> {
    let exe = current_exe()?;
    ensure_self_managed_install(&exe, InstallOperation::Uninstall)?;

    let index_dir = search::SearchIndex::default_index_dir();
    let config_path = config::Config::resolved_path();
    let config_target = config_removal_target(config_path.as_deref());
    let marker = install_marker_path(&exe);

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
