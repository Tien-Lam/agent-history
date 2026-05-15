use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{config, search};

pub(crate) fn uninstall() -> Result<i32, ErrorEnvelope> {
    let exe = std::env::current_exe()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("current_exe failed: {e}")))?;
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
