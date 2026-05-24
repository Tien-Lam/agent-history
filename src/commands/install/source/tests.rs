use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::*;

fn exe_path(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) {
        "aghist.exe"
    } else {
        "aghist"
    })
}

#[test]
fn detects_build_tree_binaries() {
    let root = tempfile::tempdir().unwrap();
    let exe = exe_path(&root.path().join("target").join("debug"));

    assert_eq!(
        detect_install_source_with(&exe, None, Some(root.path())),
        InstallSource::BuildTree
    );
}

#[test]
fn detects_cargo_bin_binaries() {
    let root = tempfile::tempdir().unwrap();
    let cargo_home = root.path().join("cargo-home");
    let exe = exe_path(&cargo_home.join("bin"));

    assert_eq!(
        detect_install_source_with(&exe, Some(&cargo_home), Some(root.path())),
        InstallSource::Cargo
    );
}

#[test]
fn release_marker_overrides_cargo_bin_path() {
    let root = tempfile::tempdir().unwrap();
    let cargo_home = root.path().join("cargo-home");
    let exe = exe_path(&cargo_home.join("bin"));
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(install_marker_path(&exe).unwrap(), INSTALL_MARKER_METHOD).unwrap();

    assert_eq!(
        detect_install_source_with(&exe, Some(&cargo_home), Some(root.path())),
        InstallSource::GithubRelease
    );
}

#[test]
fn release_marker_does_not_override_build_tree_path() {
    let root = tempfile::tempdir().unwrap();
    let exe = exe_path(&root.path().join("target").join("debug"));
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(install_marker_path(&exe).unwrap(), INSTALL_MARKER_METHOD).unwrap();

    assert_eq!(
        detect_install_source_with(&exe, None, Some(root.path())),
        InstallSource::BuildTree
    );
}

#[test]
fn release_marker_does_not_override_system_package_path() {
    let root = tempfile::tempdir().unwrap();
    let exe = exe_path(
        &root
            .path()
            .join("scoop")
            .join("apps")
            .join("aghist")
            .join("current"),
    );
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(install_marker_path(&exe).unwrap(), INSTALL_MARKER_METHOD).unwrap();

    assert_eq!(
        detect_install_source_with(&exe, None, None),
        InstallSource::SystemPackage
    );
}

#[test]
fn path_from_env_value_ignores_empty_override() {
    assert_eq!(path_from_env_value(None), None);
    assert_eq!(path_from_env_value(Some(OsString::new())), None);
    assert_eq!(
        path_from_env_value(Some(OsString::from("/tmp/cargo-home"))),
        Some(PathBuf::from("/tmp/cargo-home"))
    );
}
