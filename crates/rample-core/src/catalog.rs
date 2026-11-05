//! Rample Kit folder names and cheap file change stamps.
use std::{fs, path::Path, time::UNIX_EPOCH};

/// A canonical Kit folder name: a Bank letter and a Kit number, such as `A0` or `Z99`.
pub fn is_kit_name(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() == 2 || b.len() == 3)
        && b[0].is_ascii_uppercase()
        && b[1..].iter().all(u8::is_ascii_digit)
}
/// Size, modification time and identity of a file, to notice changes cheaply.
pub fn fingerprint(path: &Path) -> String {
    let Ok(m) = fs::symlink_metadata(path) else {
        return String::new();
    };
    let time = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|t| t.as_nanos())
        .unwrap_or(0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}-{time}-{}-{}-{}",
            m.len(),
            m.ino(),
            m.ctime(),
            m.ctime_nsec()
        )
    }
    #[cfg(not(unix))]
    {
        format!("{}-{time}-{}", m.len(), m.permissions().readonly())
    }
}
