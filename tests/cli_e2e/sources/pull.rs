#[cfg(unix)]
use super::super::aghist;

#[cfg(unix)]
mod rsync;
#[cfg(unix)]
mod safety;
#[cfg(unix)]
mod validation;

/// Writes an executable shell script that imitates rsync: it parses the last
/// arg as the dest dir, creates it, and drops one stub file. Used to drive
/// `aghist sources pull` in tests without a real SSH endpoint.
#[cfg(unix)]
fn write_fake_rsync(dir: &std::path::Path, args_log: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-rsync.sh");
    let body = format!(
        "#!/bin/sh\n\
         for a in \"$@\"; do printf '%s\\n' \"$a\" >> {log:?}; done\n\
         dest=\n\
         for a in \"$@\"; do dest=\"$a\"; done\n\
         mkdir -p \"$dest\"\n\
         printf 'stub-jsonl' > \"$dest/sample.jsonl\"\n\
         exit 0\n",
        log = args_log.display().to_string(),
    );
    std::fs::write(&script, body).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

/// Same as `write_fake_rsync` but exits non-zero so we can test error paths.
#[cfg(unix)]
fn write_failing_rsync(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-rsync-fail.sh");
    std::fs::write(&script, "#!/bin/sh\necho 'fake rsync error' >&2\nexit 23\n").unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

#[cfg(unix)]
fn stderr_error_kind(output: &std::process::Output) -> String {
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    parsed["error"]["kind"].as_str().unwrap().to_string()
}
