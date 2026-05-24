use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{config, search};

use super::cleanup::config_removal_target;
use super::current_exe;
use super::guard::{ensure_self_managed_install, InstallOperation};
use super::source::install_marker_path;

#[cfg(any(windows, test))]
const WINDOWS_REMOVE_TARGET_ENV: &str = "AGHIST_REMOVE_TARGET";
#[cfg(any(windows, test))]
const WINDOWS_DELAYED_DELETE_SCRIPT: &str =
    "timeout /t 2 /nobreak >nul & del /f /q \"%AGHIST_REMOVE_TARGET%\"";

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
        search::remove_managed_index_dir(&index_dir).map_err(|e| {
            ErrorEnvelope::new(
                "index-error",
                format!("failed to remove search index {}: {e}", index_dir.display()),
            )
            .with_hint("Unset AGHIST_INDEX_DIR or remove the directory manually after verifying it contains only aghist index files.")
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

    remove_current_exe(&exe)?;

    eprintln!("aghist has been uninstalled.");
    Ok(EXIT_OK)
}

#[cfg(windows)]
fn remove_current_exe(exe: &std::path::Path) -> Result<(), ErrorEnvelope> {
    let tmp = exe.with_extension("old");
    std::fs::rename(exe, &tmp).map_err(|e| {
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
        .args(["/C", WINDOWS_DELAYED_DELETE_SCRIPT])
        .env(WINDOWS_REMOVE_TARGET_ENV, &tmp)
        .spawn()
    {
        eprintln!(
            "warning: could not schedule cleanup of {}: {e}",
            tmp.display()
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn remove_current_exe(exe: &std::path::Path) -> Result<(), ErrorEnvelope> {
    std::fs::remove_file(exe).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to remove {}: {e}", exe.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_delayed_delete_uses_env_var_for_target_path() {
        assert_eq!(
            WINDOWS_DELAYED_DELETE_SCRIPT,
            "timeout /t 2 /nobreak >nul & del /f /q \"%AGHIST_REMOVE_TARGET%\""
        );
        assert_eq!(WINDOWS_REMOVE_TARGET_ENV, "AGHIST_REMOVE_TARGET");
    }
}
