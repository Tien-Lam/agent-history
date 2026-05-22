use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

const MAX_TEMP_COLLISIONS: u32 = 16;

pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let bytes = bytes.as_ref();
    let mut last_collision = None;
    for attempt in 0..MAX_TEMP_COLLISIONS {
        let tmp = temp_path(path, attempt)?;
        match write_via_temp(path, &tmp, bytes) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(e);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }

    Err(last_collision.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not create a unique atomic-write temp file",
        )
    }))
}

pub fn is_temp_file_for(name: &str, target_name: &str) -> bool {
    name.starts_with(&format!(".{target_name}."))
        && Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"))
        && name.len() > target_name.len() + ".tmp".len() + 2
}

fn write_via_temp(path: &Path, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp)?;
    file.write_all(bytes)?;
    drop(file);
    std::fs::rename(tmp, path)
}

fn temp_path(path: &Path, attempt: u32) -> io::Result<PathBuf> {
    let Some(file_name) = path.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic-write target has no file name",
        ));
    };

    let mut tmp_name = OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(".{}-{attempt}.tmp", std::process::id()));
    Ok(path.with_file_name(tmp_name))
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
        let collision = temp_path(&path, 0).unwrap();
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

        let err = write(&path, b"replacement").unwrap_err();

        assert!(err.kind() == io::ErrorKind::IsADirectory || err.kind() == io::ErrorKind::Other);
        let tmp = temp_path(&path, 0).unwrap();
        assert!(!tmp.exists());
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
