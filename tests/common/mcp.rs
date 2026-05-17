use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use super::helpers::aghist_bin;

pub fn run_session(env_home: &Path, requests: &[Value]) -> Vec<Value> {
    run_session_with_config(env_home, None, requests)
}

pub fn run_session_with_config(
    env_home: &Path,
    config_path: Option<&Path>,
    requests: &[Value],
) -> Vec<Value> {
    run_session_with_config_and_sources_cache(env_home, config_path, None, requests)
}

pub fn run_session_with_config_and_sources_cache(
    env_home: &Path,
    config_path: Option<&Path>,
    sources_cache: Option<&Path>,
    requests: &[Value],
) -> Vec<Value> {
    let mut cmd = Command::new(aghist_bin());
    cmd.arg("mcp")
        .env("AGHIST_HOME", env_home)
        .env("AGHIST_INDEX_DIR", env_home.join("aghist-index"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = config_path {
        cmd.env("AGHIST_CONFIG", path);
    }
    if let Some(path) = sources_cache {
        cmd.env("AGHIST_SOURCES_CACHE_DIR", path);
    }
    let mut child = cmd.spawn().expect("spawn aghist mcp");

    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for request in requests {
            let line = serde_json::to_string(request).expect("request serializes as JSON");
            stdin.write_all(line.as_bytes()).expect("write request");
            stdin.write_all(b"\n").expect("write newline");
        }
    }

    let output = child.wait_with_output().expect("wait_with_output");
    assert!(
        output.status.success(),
        "aghist mcp exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");

    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("response is valid JSON"))
        .collect()
}

pub fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    walk_into(root, root, &mut out);
    out
}

fn walk_into(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).expect("read provider dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let ty = entry.file_type().expect("file type");
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            walk_into(root, &path, out);
        } else if ty.is_file() {
            let rel = path.strip_prefix(root).expect("relative provider path");
            let bytes = std::fs::read(&path).expect("read provider file");
            out.insert(rel.to_path_buf(), bytes);
        }
    }
}
