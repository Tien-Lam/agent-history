use std::ffi::OsString;
use std::path::{Path, PathBuf};

const INSTALL_MARKER_METHOD: &str = "method=github-release";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallSource {
    GithubRelease,
    Cargo,
    BuildTree,
    SystemPackage,
    Unknown,
}

pub(super) fn detect_install_source(exe: &Path) -> InstallSource {
    let cargo_home = path_from_env_value(std::env::var_os("CARGO_HOME"));
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    detect_install_source_with(exe, cargo_home.as_deref(), home.as_deref())
}

fn path_from_env_value(value: Option<OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !os_string_is_blank(value))
        .map(PathBuf::from)
}

fn os_string_is_blank(value: &OsString) -> bool {
    value.is_empty() || value.to_string_lossy().trim().is_empty()
}

fn detect_install_source_with(
    exe: &Path,
    cargo_home: Option<&Path>,
    home: Option<&Path>,
) -> InstallSource {
    if is_build_tree_path(exe) {
        return InstallSource::BuildTree;
    }

    if is_system_package_path(exe) {
        return InstallSource::SystemPackage;
    }

    if has_github_release_marker(exe) {
        return InstallSource::GithubRelease;
    }

    if is_cargo_bin_path(exe, cargo_home, home) {
        return InstallSource::Cargo;
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

pub(super) fn install_marker_path(exe: &Path) -> Option<PathBuf> {
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
