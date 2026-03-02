pub mod hypervisor;
pub mod images;
pub mod k3s;
pub mod logging;
pub mod portfwd;
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
    tonic::include_proto!("cargobay");
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
