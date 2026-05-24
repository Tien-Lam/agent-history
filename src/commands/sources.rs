mod dir_accounting;
mod local;
mod remote;

pub(crate) use local::sources_command;
pub(crate) use remote::{
    sources_add_remote, sources_list_remote, sources_pull_remote, sources_remove_remote,
};

fn format_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b >= GB {
        format_fixed_unit(b, GB, "G")
    } else if b >= MB {
        format_fixed_unit(b, MB, "M")
    } else if b >= KB {
        format_fixed_unit(b, KB, "K")
    } else {
        format!("{b}B")
    }
}

fn format_fixed_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let tenths_total = ((u128::from(bytes) * 10) + (u128::from(unit) / 2)) / u128::from(unit);
    let whole = tenths_total / 10;
    let tenths = tenths_total % 10;
    format!("{whole}.{tenths}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::dir_accounting::dir_size_bytes;
    use super::format_bytes;

    #[test]
    fn format_bytes_keeps_one_decimal_rounding() {
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
        assert_eq!(format_bytes(2047), "2.0K");
        assert_eq!(format_bytes(10 * 1024 * 1024 + 512 * 1024), "10.5M");
    }

    #[test]
    fn dir_size_reports_non_directory_roots() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "content").unwrap();

        let err = dir_size_bytes(&file).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotADirectory);
    }
}
