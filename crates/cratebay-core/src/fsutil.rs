use std::path::Path;

/// Copy a file, using copy-on-write cloning when available.
///
/// This speeds up copying large VM disk images on APFS (macOS) and helps keep
/// first-run latency low when installing bundled runtime assets.
pub fn copy_file_fast(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(target_os = "macos")]
    {
        if !dest.exists() {
            if try_clonefile(src, dest).is_ok() {
                return Ok(());
            }
        }
    }

    std::fs::copy(src, dest)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn try_clonefile(src: &Path, dest: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let src = CString::new(src.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "src contains NUL"))?;
    let dest = CString::new(dest.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "dest contains NUL"))?;

    let rc = unsafe { libc::clonefile(src.as_ptr(), dest.as_ptr(), 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
