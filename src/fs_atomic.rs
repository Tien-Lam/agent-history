use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::Path;

pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    write_via_temp(path, bytes.as_ref())
}

pub fn is_temp_file_for(name: &str, target_name: &str) -> bool {
    name.starts_with(&format!(".{target_name}."))
        && Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
        && name.len() > target_name.len() + ".tmp".len() + 2
}

fn write_via_temp(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic-write target has no parent directory",
        ));
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };

    let prefix = temp_prefix(path)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)?;

    tmp.write_all(bytes)?;
    tmp.as_file_mut().sync_all()?;
    tmp.into_temp_path().persist(path).map_err(io::Error::from)
}

fn temp_prefix(path: &Path) -> io::Result<OsString> {
    let Some(file_name) = path.file_name() else {
        return Err(invalid_target_path());
    };
    let mut tmp_name = OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(".");
    Ok(tmp_name)
}

fn invalid_target_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "atomic-write target has no file name",
    )
}

#[cfg(test)]
fn any_temp_files_for(dir: &Path, target_name: &std::ffi::OsStr) -> io::Result<bool> {
    let Some(target_name) = target_name.to_str() else {
        return Ok(false);
    };
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| is_temp_file_for(name, target_name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_creates_parent_dirs_and_replaces_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");

        write(&path, b"first").unwrap();
        write(&path, b"second").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn write_skips_existing_temp_file_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let collision = dir.path().join(".manifest.json.existing.tmp");
        std::fs::write(&collision, b"existing").unwrap();

        write(&path, b"replacement").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        assert_eq!(std::fs::read(&collision).unwrap(), b"existing");
    }

    #[test]
    fn write_cleans_temp_file_when_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target");
        std::fs::create_dir(&path).unwrap();

        let _err = write(&path, b"replacement").unwrap_err();

        assert!(!any_temp_files_for(dir.path(), path.file_name().unwrap()).unwrap());
    }

    #[test]
    fn temp_file_matcher_is_target_specific() {
        assert!(is_temp_file_for(
            ".manifest.json.123-0.tmp",
            "manifest.json"
        ));
        assert!(!is_temp_file_for(
            ".embeddings.bin.123-0.tmp",
            "manifest.json"
        ));
        assert!(!is_temp_file_for("manifest.json.tmp", "manifest.json"));
    }
}
