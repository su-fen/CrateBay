//! FFI bindings to the Swift Virtualization.framework bridge.
//!
//! These declarations match the C functions exported by `bridge/VZBridge.swift`
//! and declared in `bridge/VZBridge.h`.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

// ---------------------------------------------------------------------------
// Raw C types
// ---------------------------------------------------------------------------

/// Opaque handle to a running VM.
pub type VZVMHandle = *mut std::ffi::c_void;

/// Descriptor for a shared directory passed to the bridge.
#[repr(C)]
pub struct VZSharedDir {
    pub tag: *const c_char,
    pub host_path: *const c_char,
    pub read_only: bool,
}

/// VM configuration passed to the bridge.
#[repr(C)]
pub struct VZVMConfig {
    pub kernel_path: *const c_char,
    pub initrd_path: *const c_char,
    pub cmdline: *const c_char,
    pub disk_path: *const c_char,
    pub console_log_path: *const c_char,
    pub cpus: u32,
    pub memory_mb: u64,
    pub rosetta: bool,
    pub shared_dirs: *const VZSharedDir,
    pub shared_dirs_count: u32,
}

// ---------------------------------------------------------------------------
// Extern declarations
// ---------------------------------------------------------------------------

extern "C" {
    pub fn vz_free_string(s: *mut c_char);

    pub fn vz_rosetta_available() -> bool;

    pub fn vz_create_disk_image(
        path: *const c_char,
        size_bytes: u64,
        out_error: *mut *mut c_char,
    ) -> i32;

    pub fn vz_create_and_start_vm(
        config: *const VZVMConfig,
        out_error: *mut *mut c_char,
    ) -> VZVMHandle;

    pub fn vz_stop_vm(handle: VZVMHandle, timeout_secs: f64, out_error: *mut *mut c_char) -> i32;

    pub fn vz_destroy_vm(handle: VZVMHandle, out_error: *mut *mut c_char) -> i32;

    pub fn vz_vm_state(handle: VZVMHandle) -> i32;

    pub fn vz_read_console(
        handle: VZVMHandle,
        offset: u64,
        buffer: *mut u8,
        buffer_len: u64,
        out_bytes_read: *mut u64,
        out_error: *mut *mut c_char,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Collect a bridge error string and free it.
///
/// Returns `None` if the pointer is null.
fn take_error(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let msg = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { vz_free_string(ptr) };
    Some(msg)
}

/// VM state as reported by the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VzState {
    Stopped,
    Running,
    Paused,
    Error,
    Starting,
    Pausing,
    Resuming,
    Stopping,
    Unknown,
}

impl VzState {
    fn from_raw(v: i32) -> Self {
        match v {
            0 => Self::Stopped,
            1 => Self::Running,
            2 => Self::Paused,
            3 => Self::Error,
            4 => Self::Starting,
            5 => Self::Pausing,
            6 => Self::Resuming,
            7 => Self::Stopping,
            _ => Self::Unknown,
        }
    }
}

/// Owned handle to a bridge VM.  Calls `vz_destroy_vm` on drop.
pub struct VmHandle {
    raw: VZVMHandle,
}

// The Swift bridge is thread-safe (all VM operations go through a dispatch queue).
unsafe impl Send for VmHandle {}
unsafe impl Sync for VmHandle {}

impl Drop for VmHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            let mut err: *mut c_char = ptr::null_mut();
            unsafe { vz_destroy_vm(self.raw, &mut err) };
            if let Some(msg) = take_error(err) {
                tracing::warn!("vz_destroy_vm on drop: {}", msg);
            }
            self.raw = ptr::null_mut();
        }
    }
}

impl VmHandle {
    /// Query the VM state.
    pub fn state(&self) -> VzState {
        let raw = unsafe { vz_vm_state(self.raw) };
        VzState::from_raw(raw)
    }

    /// Stop the VM, waiting up to `timeout_secs`.
    pub fn stop(&self, timeout_secs: f64) -> Result<(), String> {
        let mut err: *mut c_char = ptr::null_mut();
        let rc = unsafe { vz_stop_vm(self.raw, timeout_secs, &mut err) };
        if rc != 0 {
            return Err(take_error(err).unwrap_or_else(|| "unknown stop error".into()));
        }
        Ok(())
    }

    /// Read console output starting at `offset`. Returns bytes read.
    /// The console output comes from the serial port log file configured
    /// when the VM was created.
    pub fn read_console(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let mut bytes_read: u64 = 0;
        let mut err: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            vz_read_console(
                self.raw,
                offset,
                buf.as_mut_ptr(),
                buf.len() as u64,
                &mut bytes_read,
                &mut err,
            )
        };
        if rc != 0 {
            return Err(take_error(err).unwrap_or_else(|| "unknown console read error".into()));
        }
        Ok(bytes_read as usize)
    }
}

/// Descriptor for shared directories, holding owned CStrings so the
/// pointers remain valid for the duration of the FFI call.
pub struct SharedDirFFI {
    pub tag: CString,
    pub host_path: CString,
    pub read_only: bool,
}

/// Configuration for creating a VM via the bridge.
pub struct VmCreateConfig {
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub cmdline: String,
    pub disk_path: String,
    pub console_log_path: Option<String>,
    pub cpus: u32,
    pub memory_mb: u64,
    pub rosetta: bool,
    pub shared_dirs: Vec<SharedDirFFI>,
}

/// Create and start a VM through the Swift bridge.
pub fn create_and_start_vm(cfg: &VmCreateConfig) -> Result<VmHandle, String> {
    let kernel = CString::new(cfg.kernel_path.as_str())
        .map_err(|e| format!("invalid kernel_path: {}", e))?;
    let initrd = cfg
        .initrd_path
        .as_ref()
        .map(|s| CString::new(s.as_str()))
        .transpose()
        .map_err(|e| format!("invalid initrd_path: {}", e))?;
    let cmdline =
        CString::new(cfg.cmdline.as_str()).map_err(|e| format!("invalid cmdline: {}", e))?;
    let disk =
        CString::new(cfg.disk_path.as_str()).map_err(|e| format!("invalid disk_path: {}", e))?;
    let console_log = cfg
        .console_log_path
        .as_ref()
        .map(|s| CString::new(s.as_str()))
        .transpose()
        .map_err(|e| format!("invalid console_log_path: {}", e))?;

    // Build C-level shared dir array.
    let c_dirs: Vec<VZSharedDir> = cfg
        .shared_dirs
        .iter()
        .map(|d| VZSharedDir {
            tag: d.tag.as_ptr(),
            host_path: d.host_path.as_ptr(),
            read_only: d.read_only,
        })
        .collect();

    let c_config = VZVMConfig {
        kernel_path: kernel.as_ptr(),
        initrd_path: initrd.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
        cmdline: cmdline.as_ptr(),
        disk_path: disk.as_ptr(),
        console_log_path: console_log.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
        cpus: cfg.cpus,
        memory_mb: cfg.memory_mb,
        rosetta: cfg.rosetta,
        shared_dirs: if c_dirs.is_empty() {
            ptr::null()
        } else {
            c_dirs.as_ptr()
        },
        shared_dirs_count: c_dirs.len() as u32,
    };

    let mut err: *mut c_char = ptr::null_mut();
    let handle = unsafe { vz_create_and_start_vm(&c_config, &mut err) };
    if handle.is_null() {
        return Err(take_error(err).unwrap_or_else(|| "unknown create error".into()));
    }
    Ok(VmHandle { raw: handle })
}

/// Check whether Rosetta is available via the bridge.
pub fn rosetta_available() -> bool {
    unsafe { vz_rosetta_available() }
}

/// Create a disk image via the bridge.
pub fn create_disk_image(path: &str, size_bytes: u64) -> Result<(), String> {
    let c_path = CString::new(path).map_err(|e| format!("invalid path: {}", e))?;
    let mut err: *mut c_char = ptr::null_mut();
    let rc = unsafe { vz_create_disk_image(c_path.as_ptr(), size_bytes, &mut err) };
    if rc != 0 {
        return Err(take_error(err).unwrap_or_else(|| "unknown disk error".into()));
    }
    Ok(())
}
