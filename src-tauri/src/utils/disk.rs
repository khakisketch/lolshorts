use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskSpaceSnapshot {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl DiskSpaceSnapshot {
    pub fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

/// Query the volume that owns `path`. Failure is explicit: recording safety
/// callers must never replace an unknown measurement with fabricated capacity.
pub fn query_disk_space(path: &Path) -> io::Result<DiskSpaceSnapshot> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut available_bytes = 0_u64;
        let mut total_bytes = 0_u64;
        let mut total_free_bytes = 0_u64;
        unsafe {
            GetDiskFreeSpaceExW(
                windows::core::PCWSTR(wide_path.as_ptr()),
                Some(&mut available_bytes),
                Some(&mut total_bytes),
                Some(&mut total_free_bytes),
            )
        }
        .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(DiskSpaceSnapshot {
            total_bytes,
            available_bytes,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        use nix::sys::statvfs::statvfs;
        let stats = statvfs(path).map_err(|error| io::Error::other(error.to_string()))?;
        let fragment_size = stats.fragment_size();
        Ok(DiskSpaceSnapshot {
            total_bytes: fragment_size.saturating_mul(stats.blocks()),
            available_bytes: fragment_size.saturating_mul(stats.blocks_available()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_query_reports_real_capacity_for_an_existing_directory() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = query_disk_space(directory.path()).unwrap();
        assert!(snapshot.total_bytes > 0);
        assert!(snapshot.available_bytes <= snapshot.total_bytes);
        assert_eq!(
            snapshot.used_bytes(),
            snapshot.total_bytes - snapshot.available_bytes
        );
    }
}
