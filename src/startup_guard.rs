use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// Clear only the marker for this boot after the widget has stayed alive.
pub fn clear_stable_marker() -> io::Result<bool> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "HOME is unavailable"))?;
    let marker = PathBuf::from(home).join(".cache/zdesk/startup-pending");
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    clear_matching_marker(&marker, boot_id.trim())
}

fn clear_matching_marker(marker: &Path, boot_id: &str) -> io::Result<bool> {
    match fs::read_to_string(marker) {
        Ok(marked_boot) if marked_boot.trim() == boot_id => {
            fs::remove_file(marker)?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::clear_matching_marker;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clears_only_a_matching_boot_marker() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let marker =
            std::env::temp_dir().join(format!("zdesk-guard-test-{}-{nonce}", std::process::id()));
        fs::write(&marker, "old-boot\n").unwrap();
        assert!(!clear_matching_marker(&marker, "current-boot").unwrap());
        assert!(marker.exists());
        fs::write(&marker, "current-boot\n").unwrap();
        assert!(clear_matching_marker(&marker, "current-boot").unwrap());
        assert!(!marker.exists());
        assert!(!clear_matching_marker(&marker, "current-boot").unwrap());
    }
}
