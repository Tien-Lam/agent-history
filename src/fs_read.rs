use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub(crate) fn read_to_string_limited(path: &Path, max_bytes: usize) -> io::Result<String> {
    let bytes = read_limited(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub(crate) fn read_limited(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    read_from_limited(file, max_bytes)
}

fn read_from_limited<R: Read>(reader: R, max_bytes: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut reader = reader.take(max_bytes.saturating_add(1) as u64);
    reader.read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file exceeds {max_bytes} byte limit"),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limited_read_accepts_exact_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, "abcde").unwrap();

        assert_eq!(read_to_string_limited(&path, 5).unwrap(), "abcde");
    }

    #[test]
    fn limited_read_rejects_oversized_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, "abcdef").unwrap();

        let err = read_to_string_limited(&path, 5).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("5 byte limit"));
    }

    #[test]
    fn limited_read_rejects_invalid_utf8_for_strings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, [0xff]).unwrap();

        let err = read_to_string_limited(&path, 5).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
