mod local;
mod remote;

pub(crate) use local::sources_command;
pub(crate) use remote::{
    sources_add_remote, sources_list_remote, sources_pull_remote, sources_remove_remote,
};

/// Recursive directory size in bytes. Symlinks and IO errors are skipped.
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_file() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            total = total.saturating_add(meta.len());
        } else if file_type.is_dir() {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        }
    }
    total
}

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
    use super::format_bytes;

    #[test]
    fn format_bytes_keeps_one_decimal_rounding() {
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
        assert_eq!(format_bytes(2047), "2.0K");
        assert_eq!(format_bytes(10 * 1024 * 1024 + 512 * 1024), "10.5M");
    }
}
