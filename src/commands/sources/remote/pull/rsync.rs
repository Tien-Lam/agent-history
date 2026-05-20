use std::path::Path;

use aghist::cli_error::ErrorEnvelope;
use aghist::config;

pub(super) fn run_rsync_pull(
    src: &config::RemoteSource,
    data_dir: &Path,
    dry_run: bool,
) -> Result<(), ErrorEnvelope> {
    let rsync_bin = std::env::var("AGHIST_RSYNC_BIN").unwrap_or_else(|_| "rsync".to_string());
    let remote = build_rsync_remote_url(src);
    let mut local = data_dir.display().to_string();
    if !local.ends_with('/') {
        local.push('/');
    }

    let mut cmd = std::process::Command::new(&rsync_bin);
    cmd.arg("-a").arg("--delete");
    if dry_run {
        cmd.arg("--dry-run");
    }
    if matches!(src.transport, config::Transport::Ssh) {
        cmd.arg("-e").arg("ssh -o BatchMode=yes");
    }
    cmd.arg("--").arg(&remote).arg(&local);

    let output = cmd.output().map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to invoke rsync ('{rsync_bin}'): {e}"),
        )
        .with_hint("Install rsync, or set AGHIST_RSYNC_BIN to a working binary.")
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output
        .status
        .code()
        .map_or_else(|| String::from("?"), |c| c.to_string());
    Err(ErrorEnvelope::new(
        "rsync-failed",
        format!("rsync exited {code} for source '{}'", src.name),
    )
    .with_hint(format!(
        "remote: {remote} - stderr: {}",
        stderr.lines().last().unwrap_or("").trim()
    )))
}

fn build_rsync_remote_url(src: &config::RemoteSource) -> String {
    let path = src.path.trim_end_matches('/');
    match src.transport {
        config::Transport::Ssh => format!("{}:{}/", src.host, path),
        config::Transport::Rsync => {
            let path = path.trim_start_matches('/');
            format!("rsync://{}/{}/", src.host, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(transport: config::Transport, path: &str) -> config::RemoteSource {
        config::RemoteSource {
            name: "box".to_string(),
            host: "example.test".to_string(),
            path: path.to_string(),
            transport,
        }
    }

    #[test]
    fn ssh_remote_uses_colon_path() {
        let src = source(config::Transport::Ssh, "/home/me/.aghist/");

        assert_eq!(
            build_rsync_remote_url(&src),
            "example.test:/home/me/.aghist/"
        );
    }

    #[test]
    fn rsync_remote_uses_daemon_url() {
        let src = source(config::Transport::Rsync, "/module/path/");

        assert_eq!(
            build_rsync_remote_url(&src),
            "rsync://example.test/module/path/"
        );
    }
}
