//! Measure the photo filesystem, and leave breathing room for the appliance.
use crate::BoxError;
use serde::Serialize;
use std::io::{self, Read, Write};
use std::path::Path;

pub const RESERVE_BYTES: u64 = 512 * 1024 * 1024;
const CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct SpaceReserved;
impl std::fmt::Display for SpaceReserved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("photo download paused to preserve free space for the device")
    }
}
impl std::error::Error for SpaceReserved {}

#[derive(Debug, Serialize)]
pub struct StorageReport {
    pub viewer_version: &'static str,
    pub disk_total_bytes: u64,
    pub disk_available_bytes: u64,
    pub disk_used_bytes: u64,
    pub photo_bytes: u64,
    pub photo_count: u64,
    pub photo_limit_bytes: u64,
    pub reserve_bytes: u64,
}

pub struct FilesystemSpace {
    pub total: u64,
    pub available: u64,
    pub used: u64,
}

#[cfg(unix)]
// libc's counters are 32 or 64 bits depending on the Unix target.
#[allow(clippy::unnecessary_cast)]
pub fn filesystem_space(path: &Path) -> io::Result<FilesystemSpace> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL terminated; statvfs writes one initialized structure
    // on success. Never read the output on an error.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stats = unsafe { stats.assume_init() };
    let block = stats.f_frsize as u64;
    let total = (stats.f_blocks as u64).saturating_mul(block);
    let free = (stats.f_bfree as u64).saturating_mul(block).min(total);
    Ok(FilesystemSpace {
        total,
        available: (stats.f_bavail as u64).saturating_mul(block).min(free),
        used: total.saturating_sub(free),
    })
}

#[cfg(not(unix))]
pub fn filesystem_space(_path: &Path) -> io::Result<FilesystemSpace> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "filesystem measurements require Unix",
    ))
}

/// Recheck available space before each write, including temporary downloads.
/// Other processes can still consume disk; this bounds the viewer's own writes.
pub fn copy_with_reserve(
    mut reader: impl Read,
    mut writer: impl Write,
    max_bytes: u64,
    mut available: impl FnMut() -> io::Result<u64>,
) -> Result<u64, BoxError> {
    let mut buffer = [0_u8; CHUNK_BYTES];
    let mut written = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(written);
        }
        if written.saturating_add(read as u64) > max_bytes {
            return Err("downloaded image exceeds 25 MiB".into());
        }
        if available()?.saturating_sub(RESERVE_BYTES) < read as u64 {
            return Err(SpaceReserved.into());
        }
        writer.write_all(&buffer[..read])?;
        written += read as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_disk_filling_during_a_download_stops_before_the_reserve() {
        let input = vec![1; CHUNK_BYTES * 3];
        let mut output = Vec::new();
        let mut remaining = RESERVE_BYTES + CHUNK_BYTES as u64;
        let error = copy_with_reserve(input.as_slice(), &mut output, input.len() as u64, || {
            let current = remaining;
            remaining = RESERVE_BYTES;
            Ok(current)
        })
        .unwrap_err();
        assert!(error.is::<SpaceReserved>());
        assert_eq!(output.len(), CHUNK_BYTES);
    }
    #[test]
    fn unreadable_space_and_oversized_downloads_fail_closed() {
        let mut output = Vec::new();
        assert!(
            copy_with_reserve(&b"photo"[..], &mut output, 25, || Err(io::Error::other(
                "no stats"
            )))
            .is_err()
        );
        assert!(output.is_empty());
        assert!(copy_with_reserve(&b"photo"[..], &mut output, 4, || Ok(u64::MAX)).is_err());
        assert!(output.is_empty());
        assert_eq!(
            copy_with_reserve(&b"photo"[..], &mut output, 5, || Ok(RESERVE_BYTES + 5)).unwrap(),
            5
        );
    }
    #[test]
    fn measures_the_actual_filesystem() {
        let stats = filesystem_space(&std::env::temp_dir()).unwrap();
        assert!(stats.total > 0);
        assert!(stats.available <= stats.total.saturating_sub(stats.used));
    }
}
