#[cfg(feature = "self-update")]
use std::io::Write as _;
use std::path::PathBuf;

use aghist::cli_error::ErrorEnvelope;

mod cleanup;
mod guard;
mod source;
mod uninstall;

use guard::{ensure_self_managed_install, InstallOperation};
pub(crate) use uninstall::uninstall;

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
    Ok(aghist::cli_error::EXIT_OK)
}

#[cfg(not(feature = "self-update"))]
fn self_update_impl() -> Result<i32, ErrorEnvelope> {
    Err(ErrorEnvelope::new(
        "unsupported-install-method",
        "this aghist binary was built without self-update support",
    )
    .with_hint("Use your installer to update, or install a GitHub release via install.sh."))
}

pub(super) fn current_exe() -> Result<PathBuf, ErrorEnvelope> {
    std::env::current_exe().map_err(|e| ErrorEnvelope::io("current_exe failed", e))
}
