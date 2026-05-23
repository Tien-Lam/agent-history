use std::io::Write as _;
use std::path::Path;

pub(in crate::health) fn check_dir_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut last_collision = None;
    for attempt in 0..16 {
        let probe = dir.join(format!(
            ".aghist-health-probe-{}-{attempt}",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
        {
            Ok(mut file) => {
                let write_result = file.write_all(b"ok");
                let remove_result = std::fs::remove_file(&probe);
                return write_result.and(remove_result);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last_collision = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_collision.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create a unique health probe file",
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_dir_writable_removes_probe_after_success() {
        let dir = tempfile::tempdir().unwrap();

        check_dir_writable(dir.path()).unwrap();

        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn check_dir_writable_preserves_existing_fixed_probe_name() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join(".aghist-health-probe");
        std::fs::write(&existing, b"user data").unwrap();

        check_dir_writable(dir.path()).unwrap();

        assert_eq!(std::fs::read(&existing).unwrap(), b"user data");
    }
}
