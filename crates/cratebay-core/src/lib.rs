/// Acquire a mutex lock, recovering from poisoning.
///
/// If a thread panics while holding a mutex, the mutex becomes "poisoned".
/// Rather than propagating the panic via `.unwrap()`, this function recovers
/// the inner data so the rest of the application can continue.
pub fn lock_or_recover<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub mod docker_auth;
pub mod fsutil;
pub mod hypervisor;
pub mod images;
pub mod k3s;
pub mod logging;
pub mod plugin;
pub mod portfwd;
pub mod runtime;
pub mod store;
pub mod validation;
pub mod vm;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub mod proto {
    tonic::include_proto!("cratebay");
}

/// Create the platform-appropriate hypervisor implementation.
///
/// - macOS: Apple Virtualization.framework (Rosetta + VirtioFS)
/// - Linux: KVM via rust-vmm (VirtioFS via virtiofsd)
/// - Windows: Hyper-V / Windows Hypervisor Platform (Plan 9 / SMB sharing)
pub fn create_hypervisor() -> Box<dyn hypervisor::Hypervisor> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSHypervisor::new())
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxHypervisor::new())
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsHypervisor::new())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Box::new(vm::StubHypervisor::new())
    }
}

/// Get platform information string.
pub fn platform_info() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS (Virtualization.framework)"
    }

    #[cfg(target_os = "linux")]
    {
        "Linux (KVM)"
    }

    #[cfg(target_os = "windows")]
    {
        "Windows (Hyper-V)"
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "Unknown (Stub)"
    }
}

pub fn config_dir() -> std::path::PathBuf {
    store::config_dir()
}

pub fn data_dir() -> std::path::PathBuf {
    store::data_dir()
}

pub fn log_dir() -> std::path::PathBuf {
    store::log_dir()
}

pub fn vm_console_log_path(vm_id: &str) -> std::path::PathBuf {
    store::vm_console_log_path(vm_id)
}
