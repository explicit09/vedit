use anyhow::{Context, Result};
use std::path::Path;

#[cfg(not(windows))]
pub(crate) fn replace_file(src: &Path, dst: &Path) -> Result<()> {
    std::fs::rename(src, dst)
        .with_context(|| format!("renaming {} to {}", src.display(), dst.display()))
}

#[cfg(windows)]
pub(crate) fn replace_file(src: &Path, dst: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let src_w: Vec<u16> = src.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst_w: Vec<u16> = dst.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;

    let ok = unsafe { MoveFileExW(src_w.as_ptr(), dst_w.as_ptr(), flags) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("renaming {} to {}", src.display(), dst.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn replace_file_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("new");
        let dst = dir.path().join("current");
        std::fs::write(&src, "new contents").unwrap();
        std::fs::write(&dst, "old contents").unwrap();

        replace_file(&src, &dst).unwrap();

        assert!(!src.exists());
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new contents");
    }
}
