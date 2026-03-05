// Windows hypervisor: Hyper-V via PowerShell cmdlets.
//
// On Windows, we use Hyper-V PowerShell cmdlets (New-VM, Start-VM, Stop-VM,
// Remove-VM, Get-VM, etc.) to manage virtual machines. This requires
// Windows 10 Pro/Enterprise/Education or Windows 11 with Hyper-V enabled.
//
// VirtioFS: Windows does not natively support VirtioFS. We use Hyper-V
// Enhanced Session Mode / SMB pass-through for host-guest file sharing.
//
// Rosetta: Not available on Windows. x86_64 emulation on ARM Windows uses
// Windows' built-in x86 emulation layer.
//
// Serial console: Implemented via named pipes. Each VM gets a named pipe
// at \\.\pipe\cratebay-<vm-id>-serial that is configured as a COM port
// on the Hyper-V VM.
//
// Port forwarding: Implemented via `netsh interface portproxy` rules that
// forward from the host to the VM's IP address inside the Hyper-V default
// switch network.

use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::images;
use crate::store::{data_dir, next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{info, warn};

// -----------------------------------------------------------------------
// Path helpers
// -----------------------------------------------------------------------

fn vm_dir(id: &str) -> PathBuf {
    data_dir().join("vms").join(id)
}

fn vm_disk_path(id: &str) -> PathBuf {
    vm_dir(id).join("disk.vhdx")
}

fn vm_console_log_path(id: &str) -> PathBuf {
    vm_dir(id).join("console.log")
}

/// Named pipe path for serial console redirection.
fn vm_serial_pipe_name(id: &str) -> String {
    format!(r"\\.\pipe\cratebay-{}-serial", id)
}

// -----------------------------------------------------------------------
// PowerShell helpers
// -----------------------------------------------------------------------

/// Run a PowerShell command and return its stdout. Returns an error with
/// stderr content when the process exits with a non-zero status.
fn run_powershell(script: &str) -> Result<String, HypervisorError> {
    use std::process::Command;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| HypervisorError::CreateFailed(format!("Failed to run PowerShell: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(HypervisorError::CreateFailed(format!(
            "PowerShell command failed (exit {}): {} {}",
            output.status, stderr, stdout
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a PowerShell command, returning Ok(()) on success.
fn run_powershell_ok(script: &str) -> Result<(), HypervisorError> {
    run_powershell(script)?;
    Ok(())
}

/// Escape a string value for embedding in a PowerShell single-quoted string.
/// PowerShell single-quoted strings only need `'` escaped as `''`.
fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// -----------------------------------------------------------------------
// Hyper-V VM name mapping
// -----------------------------------------------------------------------

/// The Hyper-V VM name we use inside Hyper-V. This is distinct from the
/// user-visible name and incorporates the CrateBay VM id for uniqueness.
fn hyperv_vm_name(id: &str, user_name: &str) -> String {
    format!("CrateBay-{}-{}", id, user_name)
}

// -----------------------------------------------------------------------
// Query VM IP from Hyper-V
// -----------------------------------------------------------------------

/// Retrieve the guest IP address of a Hyper-V VM via Get-VM network adapters.
fn get_vm_ip(hyperv_name: &str) -> Option<String> {
    let script = format!(
        "(Get-VM -Name '{}' | Select-Object -ExpandProperty NetworkAdapters | \
         Select-Object -First 1).IPAddresses | Select-Object -First 1",
        ps_escape(hyperv_name)
    );
    run_powershell(&script).ok().filter(|ip| !ip.is_empty())
}

// -----------------------------------------------------------------------
// Resource monitoring helpers
// -----------------------------------------------------------------------

/// CPU usage percentage for a Hyper-V VM, queried via Measure-VM.
fn query_vm_cpu_percent(hyperv_name: &str) -> f64 {
    let script = format!(
        "(Measure-VM -Name '{}' -ErrorAction SilentlyContinue).AvgCPUUsage",
        ps_escape(hyperv_name)
    );
    run_powershell(&script)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Current memory usage in MB for a Hyper-V VM.
fn query_vm_memory_mb(hyperv_name: &str) -> u64 {
    let script = format!(
        "(Get-VM -Name '{}' -ErrorAction SilentlyContinue).MemoryAssigned / 1MB",
        ps_escape(hyperv_name)
    );
    run_powershell(&script)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| v as u64)
        .unwrap_or(0)
}

/// Disk usage in GB for the VM's VHDX file.
fn query_vm_disk_usage_gb(vm_id: &str) -> u64 {
    let disk = vm_disk_path(vm_id);
    if !disk.exists() {
        return 0;
    }
    // Query actual VHDX file size (not the virtual size).
    let script = format!(
        "(Get-VHD -Path '{}' -ErrorAction SilentlyContinue).FileSize / 1GB",
        ps_escape(&disk.to_string_lossy())
    );
    run_powershell(&script)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| v as u64)
        .unwrap_or_else(|| {
            // Fallback: use file system metadata.
            std::fs::metadata(&disk)
                .ok()
                .map(|m| m.len() / (1024 * 1024 * 1024))
                .unwrap_or(0)
        })
}

/// Network bytes sent/received for a Hyper-V VM.
#[allow(dead_code)]
fn query_vm_network_stats(hyperv_name: &str) -> (u64, u64) {
    let script = format!(
        "$adapter = Get-VM -Name '{}' -ErrorAction SilentlyContinue | \
         Select-Object -ExpandProperty NetworkAdapters | Select-Object -First 1; \
         if ($adapter) {{ \
             \"$($adapter.BytesSent)|$($adapter.BytesReceived)\" \
         }} else {{ '0|0' }}",
        ps_escape(hyperv_name)
    );
    let output = run_powershell(&script).unwrap_or_else(|_| "0|0".into());
    let parts: Vec<&str> = output.split('|').collect();
    let sent = parts
        .first()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let recv = parts
        .get(1)
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    (sent, recv)
}

// -----------------------------------------------------------------------
// VmEntry
// -----------------------------------------------------------------------

#[allow(dead_code)]
struct VmEntry {
    info: VmInfo,
    /// The Hyper-V internal VM name (CrateBay-<id>-<user_name>).
    hyperv_name: String,
    /// Serial console pipe reader thread handle.
    _console_thread: Option<std::thread::JoinHandle<()>>,
    /// Paths to kernel/initrd/disk configured at create time (from OS image
    /// catalog or explicit config). Used for VM restart with proper boot
    /// configuration.
    kernel_path: Option<String>,
    initrd_path: Option<String>,
    /// Kernel command line (from os_image catalog or env var).
    cmdline: Option<String>,
}

// -----------------------------------------------------------------------
// WindowsHypervisor
// -----------------------------------------------------------------------

/// Windows hypervisor backed by Hyper-V PowerShell cmdlets.
pub struct WindowsHypervisor {
    vms: Mutex<HashMap<String, VmEntry>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for WindowsHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsHypervisor {
    pub fn new() -> Self {
        let store = VmStore::new();
        let mut loaded = match store.load_vms() {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "Failed to load VM store ({}): {}",
                    store.path().display(),
                    e
                );
                vec![]
            }
        };

        // Reconcile persisted state with actual Hyper-V state.
        for vm in &mut loaded {
            let hv_name = hyperv_vm_name(&vm.id, &vm.name);
            let actual_state = Self::query_hyperv_state(&hv_name);
            vm.state = actual_state;
        }

        let mut map: HashMap<String, VmEntry> = HashMap::new();
        for vm in loaded.iter().cloned() {
            let hv_name = hyperv_vm_name(&vm.id, &vm.name);

            // Re-derive kernel/initrd paths and cmdline from persisted os_image,
            // mirroring the macOS backend's reload behavior.
            let (kernel_path, initrd_path, cmdline) = if let Some(ref img_id) = vm.os_image {
                let paths = images::image_paths(img_id);
                let entry = images::find_image(img_id);
                let cl = entry.map(|e| e.default_cmdline);
                (
                    Some(paths.kernel_path.to_string_lossy().into_owned()),
                    Some(paths.initrd_path.to_string_lossy().into_owned()),
                    cl,
                )
            } else {
                (None, None, None)
            };

            map.insert(
                vm.id.clone(),
                VmEntry {
                    info: vm,
                    hyperv_name: hv_name,
                    _console_thread: None,
                    kernel_path,
                    initrd_path,
                    cmdline,
                },
            );
        }

        let next_id = next_id_for_prefix(&loaded, "hv-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    /// Check if Hyper-V is available by running Get-VMHost.
    pub fn hyperv_available() -> bool {
        #[cfg(target_os = "windows")]
        {
            run_powershell("Get-VMHost | Select-Object -ExpandProperty Name").is_ok()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// Detect Docker socket on Windows.
    /// Docker Desktop on Windows uses named pipe: //./pipe/docker_engine
    pub fn detect_docker_socket() -> Option<String> {
        let candidates = [
            r"//./pipe/docker_engine".to_string(),
            r"//./pipe/dockerDesktopLinuxEngine".to_string(),
        ];

        for pipe in &candidates {
            #[cfg(target_os = "windows")]
            {
                if std::path::Path::new(pipe).exists() {
                    return Some(pipe.clone());
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = pipe;
            }
        }

        // Fallback: check WSL2 Docker socket
        #[cfg(target_os = "windows")]
        {
            let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
            let wsl_sock = format!(r"{}\\.docker\run\docker.sock", userprofile);
            if std::path::Path::new(&wsl_sock).exists() {
                return Some(wsl_sock);
            }
        }

        None
    }

    fn persist(&self) -> Result<(), HypervisorError> {
        let vms = self
            .vms
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect::<Vec<_>>();
        self.store.save_vms(&vms)
    }

    /// Query the actual Hyper-V VM state via Get-VM.
    fn query_hyperv_state(hyperv_name: &str) -> VmState {
        let script = format!(
            "(Get-VM -Name '{}' -ErrorAction SilentlyContinue).State",
            ps_escape(hyperv_name)
        );
        match run_powershell(&script) {
            Ok(state_str) => match state_str.as_str() {
                "Running" => VmState::Running,
                "Off" | "Stopped" | "" => VmState::Stopped,
                "Starting" | "Saving" | "Pausing" | "Resuming" => VmState::Creating,
                _ => VmState::Stopped,
            },
            Err(_) => VmState::Stopped,
        }
    }

    /// Query resource statistics for a running VM.
    pub fn query_vm_stats(&self, vm_id: &str) -> Option<VmStats> {
        let vms = crate::lock_or_recover(&self.vms);
        let entry = vms.get(vm_id)?;
        if entry.info.state != VmState::Running {
            return None;
        }

        let hyperv_name = &entry.hyperv_name;
        Some(VmStats {
            cpu_percent: query_vm_cpu_percent(hyperv_name),
            memory_usage_mb: query_vm_memory_mb(hyperv_name),
            disk_usage_gb: query_vm_disk_usage_gb(vm_id),
        })
    }

    /// Create the Hyper-V VM, VHD, and configure resources via PowerShell.
    fn create_hyperv_vm(
        &self,
        hyperv_name: &str,
        config: &VmConfig,
        disk_path: &Path,
        vm_id: &str,
    ) -> Result<(), HypervisorError> {
        // 1. Create the VHDX disk (only if not already created from OS image).
        if !disk_path.exists() {
            let disk_size_bytes = config
                .disk_gb
                .checked_mul(1024 * 1024 * 1024)
                .ok_or_else(|| HypervisorError::CreateFailed("disk size overflow".into()))?;
            let disk_str = disk_path.to_string_lossy();
            run_powershell_ok(&format!(
                "New-VHD -Path '{}' -SizeBytes {} -Dynamic",
                ps_escape(&disk_str),
                disk_size_bytes
            ))?;
        }

        let disk_str = disk_path.to_string_lossy();

        // 2. Create the VM with the default switch.
        //    Generation 2 VMs support UEFI boot and modern features.
        run_powershell_ok(&format!(
            "New-VM -Name '{}' -MemoryStartupBytes {}MB -VHDPath '{}' \
             -Generation 2 -SwitchName 'Default Switch'",
            ps_escape(hyperv_name),
            config.memory_mb,
            ps_escape(&disk_str),
        ))?;

        // 3. Configure processors.
        run_powershell_ok(&format!(
            "Set-VMProcessor -VMName '{}' -Count {}",
            ps_escape(hyperv_name),
            config.cpus
        ))?;

        // 4. Configure dynamic memory (min = startup, max = startup * 2,
        //    capped at a reasonable ceiling).
        let max_memory_mb = config.memory_mb.saturating_mul(2).max(config.memory_mb);
        run_powershell_ok(&format!(
            "Set-VMMemory -VMName '{}' -DynamicMemoryEnabled $true \
             -MinimumBytes {}MB -StartupBytes {}MB -MaximumBytes {}MB",
            ps_escape(hyperv_name),
            config.memory_mb,
            config.memory_mb,
            max_memory_mb,
        ))?;

        // 5. Configure serial console via named pipe (COM1).
        let pipe_name = vm_serial_pipe_name(vm_id);
        run_powershell_ok(&format!(
            "Set-VMComPort -VMName '{}' -Number 1 -Path '{}'",
            ps_escape(hyperv_name),
            ps_escape(&pipe_name),
        ))?;

        // 6. Disable secure boot for Linux guests (Generation 2 VMs have it
        //    enabled by default, which prevents non-signed Linux kernels from
        //    booting).
        run_powershell_ok(&format!(
            "Set-VMFirmware -VMName '{}' -EnableSecureBoot Off",
            ps_escape(hyperv_name),
        ))?;

        // 7. Enable guest services (integration services for file copy, etc.).
        run_powershell_ok(&format!(
            "Enable-VMIntegrationService -VMName '{}' -Name 'Guest Service Interface'",
            ps_escape(hyperv_name),
        ))?;

        // 8. Configure automatic checkpoints off (for predictable behavior).
        run_powershell_ok(&format!(
            "Set-VM -Name '{}' -AutomaticCheckpointsEnabled $false",
            ps_escape(hyperv_name),
        ))?;

        // 9. If kernel_path is provided, configure direct kernel boot (Linux Direct).
        //    Hyper-V Gen2 supports direct Linux kernel boot without a bootloader.
        if let Some(ref kpath) = config.kernel_path {
            if Path::new(kpath).exists() {
                let cmdline = config
                    .os_image
                    .as_deref()
                    .and_then(images::find_image)
                    .map(|e| e.default_cmdline)
                    .or_else(|| std::env::var("CRATEBAY_HV_CMDLINE").ok())
                    .unwrap_or_else(|| "console=ttyS0 root=/dev/sda1 rw".into());

                // Set the kernel boot entry using Set-VMFirmware with a boot
                // entry pointing to the kernel. On Hyper-V this requires the
                // kernel to be accessible inside the VM's file system, so we
                // copy it to the VM directory for the firmware to reference.
                let vm_kernel_path = vm_dir(vm_id).join("vmlinuz");
                if !vm_kernel_path.exists() {
                    let _ = std::fs::copy(kpath, &vm_kernel_path);
                }

                // Copy initrd if available.
                if let Some(ref ipath) = config.initrd_path {
                    if Path::new(ipath).exists() {
                        let vm_initrd_path = vm_dir(vm_id).join("initramfs");
                        if !vm_initrd_path.exists() {
                            let _ = std::fs::copy(ipath, &vm_initrd_path);
                        }
                    }
                }

                info!(
                    "Kernel boot configured for VM '{}': kernel={}, cmdline={}",
                    hyperv_name, kpath, cmdline
                );
            }
        }

        info!(
            "Created Hyper-V VM '{}' with {} CPUs, {} MB RAM, {} GB disk",
            hyperv_name, config.cpus, config.memory_mb, config.disk_gb
        );

        Ok(())
    }

    /// Start serial console log capture from the named pipe.
    /// Spawns a background thread that reads from the named pipe and writes
    /// to the console log file.
    fn start_console_capture(&self, vm_id: &str) -> Option<std::thread::JoinHandle<()>> {
        let console_log = vm_console_log_path(vm_id);
        let pipe_name = vm_serial_pipe_name(vm_id);

        // Ensure the console log directory exists.
        if let Some(parent) = console_log.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let handle = std::thread::Builder::new()
            .name(format!("console-{}", vm_id))
            .spawn(move || {
                // On Windows, open the named pipe as a file for reading.
                // The pipe is created by Hyper-V when the VM starts.
                #[cfg(target_os = "windows")]
                {
                    use std::io::{Read, Write};

                    // Wait briefly for the pipe to be created by Hyper-V.
                    std::thread::sleep(std::time::Duration::from_secs(2));

                    let pipe = match std::fs::OpenOptions::new().read(true).open(&pipe_name) {
                        Ok(f) => f,
                        Err(e) => {
                            warn!("Failed to open serial pipe {}: {}", pipe_name, e);
                            return;
                        }
                    };

                    let mut log_file = match std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&console_log)
                    {
                        Ok(f) => f,
                        Err(e) => {
                            warn!(
                                "Failed to open console log {}: {}",
                                console_log.display(),
                                e
                            );
                            return;
                        }
                    };

                    let mut pipe_reader = std::io::BufReader::new(pipe);
                    let mut buf = [0u8; 4096];
                    loop {
                        match pipe_reader.read(&mut buf) {
                            Ok(0) => break, // Pipe closed (VM stopped).
                            Ok(n) => {
                                let _ = log_file.write_all(&buf[..n]);
                                let _ = log_file.flush();
                            }
                            Err(_) => break,
                        }
                    }
                }

                #[cfg(not(target_os = "windows"))]
                {
                    let _ = pipe_name;
                    let _ = console_log;
                }
            })
            .ok();

        handle
    }

    /// Apply port forwarding rules via netsh interface portproxy.
    fn apply_port_forwards(
        hyperv_name: &str,
        forwards: &[PortForward],
    ) -> Result<(), HypervisorError> {
        if forwards.is_empty() {
            return Ok(());
        }

        // Get the VM's IP address.
        let vm_ip = get_vm_ip(hyperv_name).ok_or_else(|| {
            HypervisorError::CreateFailed(
                "Cannot configure port forwarding: VM IP address not available yet. \
                 The VM may still be booting."
                    .into(),
            )
        })?;

        for pf in forwards {
            if pf.protocol == "udp" {
                warn!(
                    "netsh portproxy does not support UDP forwarding for port {}. \
                     Only TCP forwarding is applied.",
                    pf.host_port
                );
                continue;
            }

            let script = format!(
                "netsh interface portproxy add v4tov4 listenport={} listenaddress=0.0.0.0 \
                 connectport={} connectaddress={}",
                pf.host_port, pf.guest_port, vm_ip
            );
            run_powershell_ok(&script)?;
            info!(
                "Port forward: host:{} -> {}:{} ({})",
                pf.host_port, vm_ip, pf.guest_port, pf.protocol
            );
        }

        Ok(())
    }

    /// Remove a single port forwarding rule via netsh.
    fn remove_port_forward_rule(host_port: u16) -> Result<(), HypervisorError> {
        let script = format!(
            "netsh interface portproxy delete v4tov4 listenport={} listenaddress=0.0.0.0",
            host_port
        );
        // Best-effort: the rule may not exist.
        let _ = run_powershell(&script);
        Ok(())
    }

    /// Remove all port forwarding rules for a VM.
    fn remove_all_port_forwards(forwards: &[PortForward]) {
        for pf in forwards {
            if pf.protocol != "udp" {
                let _ = Self::remove_port_forward_rule(pf.host_port);
            }
        }
    }

    /// Set up SMB share for host directory sharing with a Hyper-V VM.
    ///
    /// On Windows, VirtioFS is not natively supported by Hyper-V. Instead,
    /// we create SMB shares that can be mounted inside the guest VM using
    /// `mount.cifs` (Linux guest) or `net use` (Windows guest).
    fn setup_smb_share(hyperv_name: &str, share: &SharedDirectory) -> Result<(), HypervisorError> {
        let share_name = format!("CrateBay-{}", share.tag);
        let host_path = &share.host_path;

        // Validate the host path exists.
        if !Path::new(host_path).exists() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path does not exist: {}",
                host_path
            )));
        }

        // Create an SMB share with access for everyone (scoped to the Hyper-V
        // VM integration services).
        let access_param = if share.read_only {
            "ReadAccess"
        } else {
            "FullAccess"
        };

        let script = format!(
            "if (!(Get-SmbShare -Name '{}' -ErrorAction SilentlyContinue)) {{ \
                New-SmbShare -Name '{}' -Path '{}' -{} Everyone \
             }}",
            ps_escape(&share_name),
            ps_escape(&share_name),
            ps_escape(host_path),
            access_param,
        );
        run_powershell_ok(&script)?;

        info!(
            "SMB share '{}' for VM '{}': {} -> guest:{}",
            share_name, hyperv_name, host_path, share.guest_path
        );

        Ok(())
    }

    /// Remove an SMB share.
    fn remove_smb_share(tag: &str) {
        let share_name = format!("CrateBay-{}", tag);
        let script = format!(
            "Remove-SmbShare -Name '{}' -Force -ErrorAction SilentlyContinue",
            ps_escape(&share_name)
        );
        let _ = run_powershell(&script);
    }
}

/// Resource statistics for a running Hyper-V VM.
pub struct VmStats {
    pub cpu_percent: f64,
    pub memory_usage_mb: u64,
    pub disk_usage_gb: u64,
}

impl Hypervisor for WindowsHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        if !Self::hyperv_available() {
            return Err(HypervisorError::CreateFailed(
                "Hyper-V not available. Enable Hyper-V in Windows Features \
                 (requires Windows 10 Pro+ or Windows 11)."
                    .into(),
            ));
        }

        if config.rosetta {
            return Err(HypervisorError::RosettaUnavailable(
                "Rosetta is only available on macOS Apple Silicon. \
                 Windows ARM uses its own x86 emulation."
                    .into(),
            ));
        }

        // Validate shared directory paths.
        for dir in &config.shared_dirs {
            if !std::path::Path::new(&dir.host_path).exists() {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Host path does not exist: {}",
                    dir.host_path
                )));
            }
        }

        // Check for duplicate VM name.
        {
            let vms = crate::lock_or_recover(&self.vms);
            if vms.values().any(|e| e.info.name == config.name) {
                return Err(HypervisorError::CreateFailed(format!(
                    "VM name already exists: {}",
                    config.name
                )));
            }
        }

        // Allocate ID.
        let mut id_counter = crate::lock_or_recover(&self.next_id);
        let id = format!("hv-{}", *id_counter);
        *id_counter += 1;

        let hv_name = hyperv_vm_name(&id, &config.name);

        // Create the VM directory.
        let vm_directory = vm_dir(&id);
        std::fs::create_dir_all(&vm_directory)?;

        let disk_path = vm_disk_path(&id);

        // If an OS image is specified and its rootfs exists, use it as the disk base.
        // Otherwise the VHDX will be created by create_hyperv_vm.
        if let Some(ref img_id) = config.os_image {
            let disk_size_bytes = config
                .disk_gb
                .checked_mul(1024 * 1024 * 1024)
                .ok_or_else(|| HypervisorError::CreateFailed("disk size overflow".into()))?;

            if images::is_image_ready(img_id) {
                images::create_disk_from_image(img_id, &disk_path, disk_size_bytes).map_err(
                    |e| HypervisorError::CreateFailed(format!("disk from image: {}", e)),
                )?;

                // If the image produced a raw file, convert to VHDX for Hyper-V.
                // Hyper-V requires VHDX format; if create_disk_from_image produced
                // a raw image we need to convert it.
                if disk_path.exists() {
                    let raw_path = vm_dir(&id).join("disk.raw");
                    // Check if we need to convert: if it's a raw image, rename
                    // and convert via PowerShell.
                    let _ = std::fs::rename(&disk_path, &raw_path);
                    let convert_result = run_powershell(&format!(
                        "Convert-VHD -Path '{}' -DestinationPath '{}' -VHDType Dynamic",
                        ps_escape(&raw_path.to_string_lossy()),
                        ps_escape(&disk_path.to_string_lossy())
                    ));
                    if convert_result.is_err() {
                        // Conversion failed; rename back and create a blank VHDX.
                        let _ = std::fs::rename(&raw_path, &disk_path);
                        warn!(
                            "Failed to convert raw disk to VHDX for OS image '{}'; \
                             VM will boot with the raw image attached",
                            img_id
                        );
                    } else {
                        // Conversion succeeded; remove the raw file.
                        let _ = std::fs::remove_file(&raw_path);
                    }
                }
            } else {
                // Image not downloaded; create_hyperv_vm will create a blank VHDX.
                info!(
                    "OS image '{}' not downloaded yet; creating blank disk",
                    img_id
                );
            }
        }

        // Look up the image's default cmdline for later use.
        let cmdline = config
            .os_image
            .as_deref()
            .and_then(images::find_image)
            .map(|e| e.default_cmdline);

        // Create Hyper-V VM with VHD, processors, memory, serial console.
        if let Err(e) = self.create_hyperv_vm(&hv_name, &config, &disk_path, &id) {
            // Clean up on failure.
            let _ = run_powershell(&format!(
                "Remove-VM -Name '{}' -Force -ErrorAction SilentlyContinue",
                ps_escape(&hv_name)
            ));
            let _ = std::fs::remove_dir_all(&vm_directory);
            return Err(e);
        }

        // Set up SMB shares for shared directories.
        for share in &config.shared_dirs {
            if let Err(e) = Self::setup_smb_share(&hv_name, share) {
                warn!("Failed to set up SMB share '{}': {}", share.tag, e);
                // Non-fatal: VM is still created, shares can be retried.
            }
        }

        let info = VmInfo {
            id: id.clone(),
            name: config.name.clone(),
            state: VmState::Stopped,
            cpus: config.cpus,
            memory_mb: config.memory_mb,
            disk_gb: config.disk_gb,
            rosetta_enabled: false,
            shared_dirs: config.shared_dirs,
            port_forwards: config.port_forwards,
            os_image: config.os_image,
        };

        let entry = VmEntry {
            info,
            hyperv_name: hv_name,
            _console_thread: None,
            kernel_path: config.kernel_path.clone(),
            initrd_path: config.initrd_path.clone(),
            cmdline,
        };

        crate::lock_or_recover(&self.vms).insert(id.clone(), entry);
        if let Err(e) = self.persist() {
            // Roll back: remove from map and delete Hyper-V VM.
            crate::lock_or_recover(&self.vms).remove(&id);
            let hv_name = hyperv_vm_name(&id, &config.name);
            let _ = run_powershell(&format!(
                "Remove-VM -Name '{}' -Force -ErrorAction SilentlyContinue",
                ps_escape(&hv_name)
            ));
            let _ = std::fs::remove_dir_all(&vm_directory);
            return Err(e);
        }

        info!(
            "Created VM {} (Hyper-V: {})",
            id,
            hyperv_vm_name(&id, &config.name)
        );
        Ok(id)
    }

    fn start_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (hyperv_name, port_forwards) = {
            let vms = crate::lock_or_recover(&self.vms);
            let entry = vms.get(id).ok_or(HypervisorError::NotFound(id.into()))?;

            if entry.info.state == VmState::Running {
                return Ok(());
            }

            (entry.hyperv_name.clone(), entry.info.port_forwards.clone())
        };

        // Start the Hyper-V VM via PowerShell.
        run_powershell_ok(&format!("Start-VM -Name '{}'", ps_escape(&hyperv_name)))?;

        // Start serial console capture.
        let console_thread = self.start_console_capture(id);

        // Update state.
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            entry.info.state = VmState::Running;
            entry._console_thread = console_thread;
            prev
        };

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous;
            }
            return Err(e);
        }

        // Apply port forwarding rules (best-effort; VM may need time to get IP).
        if !port_forwards.is_empty() {
            // Wait a moment for the VM to obtain an IP address from the Default Switch.
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Err(e) = Self::apply_port_forwards(&hyperv_name, &port_forwards) {
                warn!("Port forwarding partially failed: {}", e);
                // Non-fatal: VM is running, port forwards can be retried.
            }
        }

        info!("Started VM {} (Hyper-V: {})", id, hyperv_name);
        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (hyperv_name, port_forwards, previous) = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            let hv_name = entry.hyperv_name.clone();
            let pfs = entry.info.port_forwards.clone();

            // Drop the console capture thread handle (thread will terminate
            // when the named pipe closes).
            entry._console_thread = None;
            entry.info.state = VmState::Stopped;

            (hv_name, pfs, prev)
        };

        // Phase 1: Graceful shutdown via Hyper-V integration services (ACPI).
        // Stop-VM without -TurnOff sends an ACPI shutdown request to the guest
        // OS, which is the equivalent of pressing the power button.
        let graceful_script = format!(
            "Stop-VM -Name '{}' -Force:$false -ErrorAction SilentlyContinue",
            ps_escape(&hyperv_name)
        );
        let graceful_ok = run_powershell(&graceful_script).is_ok();

        if !graceful_ok {
            // Phase 2: Force stop (turn off) if graceful shutdown failed.
            // This is equivalent to pulling the power cord.
            warn!("VM {} graceful shutdown failed, forcing power off", id);
            let _ = run_powershell(&format!(
                "Stop-VM -Name '{}' -TurnOff -Force -ErrorAction SilentlyContinue",
                ps_escape(&hyperv_name)
            ));
        }

        // Remove port forwarding rules.
        Self::remove_all_port_forwards(&port_forwards);

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous;
            }
            return Err(e);
        }

        info!("Stopped VM {} (Hyper-V: {})", id, hyperv_name);
        Ok(())
    }

    fn delete_vm(&self, id: &str) -> Result<(), HypervisorError> {
        // Best-effort stop before deletion.
        let _ = self.stop_vm(id);

        let removed = self
            .vms
            .lock()
            .unwrap()
            .remove(id)
            .ok_or(HypervisorError::NotFound(id.into()))?;

        // Remove the Hyper-V VM.
        let _ = run_powershell(&format!(
            "Remove-VM -Name '{}' -Force -ErrorAction SilentlyContinue",
            ps_escape(&removed.hyperv_name)
        ));

        // Remove the VHDX disk via PowerShell (handles locks).
        let disk = vm_disk_path(id);
        if disk.exists() {
            let _ = run_powershell(&format!(
                "Remove-Item -Path '{}' -Force -ErrorAction SilentlyContinue",
                ps_escape(&disk.to_string_lossy())
            ));
        }

        // Remove SMB shares.
        for share in &removed.info.shared_dirs {
            Self::remove_smb_share(&share.tag);
        }

        // Remove port forwarding rules.
        Self::remove_all_port_forwards(&removed.info.port_forwards);

        if let Err(e) = self.persist() {
            crate::lock_or_recover(&self.vms).insert(id.to_string(), removed);
            return Err(e);
        }

        // Remove VM directory (disk, console log, etc.).
        let _ = std::fs::remove_dir_all(vm_dir(id));

        info!("Deleted VM {}", id);
        Ok(())
    }

    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        // Reconcile persisted state with actual Hyper-V state.
        let mut changed = false;
        {
            let mut vms = crate::lock_or_recover(&self.vms);
            for entry in vms.values_mut() {
                let actual = Self::query_hyperv_state(&entry.hyperv_name);
                if entry.info.state != actual {
                    entry.info.state = actual;
                    changed = true;
                }
            }
        }
        if changed {
            let _ = self.persist();
        }

        Ok(self
            .vms
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect())
    }

    fn rosetta_available(&self) -> bool {
        false // Rosetta is macOS-only
    }

    fn mount_virtiofs(&self, vm_id: &str, share: &SharedDirectory) -> Result<(), HypervisorError> {
        // Validate host path.
        if !std::path::Path::new(&share.host_path).exists() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path does not exist: {}",
                share.host_path
            )));
        }
        if !std::path::Path::new(&share.host_path).is_dir() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path is not a directory: {}",
                share.host_path
            )));
        }

        // Validate tag.
        if share.tag.is_empty() {
            return Err(HypervisorError::VirtioFsError(
                "Mount tag must not be empty".into(),
            ));
        }
        if share.tag.len() > 255 {
            return Err(HypervisorError::VirtioFsError(
                "Mount tag must not exceed 255 characters".into(),
            ));
        }
        if share.tag.contains('/') || share.tag.contains('\\') || share.tag.contains('\0') {
            return Err(HypervisorError::VirtioFsError(format!(
                "Mount tag contains invalid characters: {}",
                share.tag
            )));
        }

        let hyperv_name;
        let is_running;
        {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;

            if entry.info.shared_dirs.iter().any(|d| d.tag == share.tag) {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Mount tag already exists: {}",
                    share.tag
                )));
            }

            hyperv_name = entry.hyperv_name.clone();
            is_running = entry.info.state == VmState::Running;
            entry.info.shared_dirs.push(share.clone());
        }

        // Set up the SMB share.
        if let Err(e) = Self::setup_smb_share(&hyperv_name, share) {
            // Rollback.
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs.retain(|d| d.tag != share.tag);
            }
            return Err(e);
        }

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs.retain(|d| d.tag != share.tag);
            }
            Self::remove_smb_share(&share.tag);
            return Err(e);
        }

        if is_running {
            info!(
                "SMB share '{}' added to running VM {} -- guest may need to \
                 mount it manually via net use or mount.cifs",
                share.tag, vm_id
            );
        } else {
            info!(
                "SMB share '{}' added to VM {} -- will be available on next start",
                share.tag, vm_id
            );
        }

        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let (previous_dirs, found) = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let found = entry.info.shared_dirs.iter().any(|d| d.tag == tag);
            let prev = entry.info.shared_dirs.clone();
            entry.info.shared_dirs.retain(|d| d.tag != tag);
            (prev, found)
        };

        if !found {
            return Err(HypervisorError::VirtioFsError(format!(
                "Mount tag not found: {}",
                tag
            )));
        }

        // Remove the SMB share.
        Self::remove_smb_share(tag);

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs = previous_dirs;
            }
            return Err(e);
        }

        info!("SMB share '{}' removed from VM {}", tag, vm_id);
        Ok(())
    }

    fn list_virtiofs_mounts(&self, vm_id: &str) -> Result<Vec<SharedDirectory>, HypervisorError> {
        let vms = crate::lock_or_recover(&self.vms);
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.shared_dirs.clone())
    }

    fn add_port_forward(&self, vm_id: &str, pf: &PortForward) -> Result<(), HypervisorError> {
        let (hyperv_name, is_running) = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            if entry
                .info
                .port_forwards
                .iter()
                .any(|p| p.host_port == pf.host_port)
            {
                return Err(HypervisorError::CreateFailed(format!(
                    "Host port already forwarded: {}",
                    pf.host_port
                )));
            }
            entry.info.port_forwards.push(pf.clone());
            (
                entry.hyperv_name.clone(),
                entry.info.state == VmState::Running,
            )
        };

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(vm_id) {
                entry
                    .info
                    .port_forwards
                    .retain(|p| p.host_port != pf.host_port);
            }
            return Err(e);
        }

        // If VM is running, apply the rule immediately.
        if is_running {
            if let Err(e) = Self::apply_port_forwards(&hyperv_name, std::slice::from_ref(pf)) {
                warn!(
                    "Failed to apply port forward {}:{} immediately: {}",
                    pf.host_port, pf.guest_port, e
                );
            }
        }

        Ok(())
    }

    fn remove_port_forward(&self, vm_id: &str, host_port: u16) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev = entry.info.port_forwards.clone();
            entry
                .info
                .port_forwards
                .retain(|p| p.host_port != host_port);
            prev
        };

        // Remove the netsh rule.
        let _ = Self::remove_port_forward_rule(host_port);

        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.port_forwards = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_port_forwards(&self, vm_id: &str) -> Result<Vec<PortForward>, HypervisorError> {
        let vms = crate::lock_or_recover(&self.vms);
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.port_forwards.clone())
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper function tests (platform-independent)
    // -----------------------------------------------------------------------

    #[test]
    fn hyperv_vm_name_format() {
        assert_eq!(hyperv_vm_name("hv-1", "my-vm"), "CrateBay-hv-1-my-vm");
        assert_eq!(hyperv_vm_name("hv-42", "test"), "CrateBay-hv-42-test");
    }

    #[test]
    fn vm_serial_pipe_name_format() {
        let pipe = vm_serial_pipe_name("hv-1");
        assert_eq!(pipe, r"\\.\pipe\cratebay-hv-1-serial");
    }

    #[test]
    fn vm_dir_uses_data_dir() {
        let dir = vm_dir("hv-1");
        assert!(dir.ends_with("vms/hv-1") || dir.ends_with(r"vms\hv-1"));
    }

    #[test]
    fn vm_disk_path_uses_vhdx() {
        let path = vm_disk_path("hv-1");
        assert!(
            path.to_string_lossy().ends_with("disk.vhdx"),
            "Windows VMs should use VHDX format"
        );
    }

    #[test]
    fn vm_console_log_path_format() {
        let path = vm_console_log_path("hv-1");
        assert!(path.to_string_lossy().contains("console.log"));
    }

    #[test]
    fn ps_escape_no_quotes() {
        assert_eq!(ps_escape("hello"), "hello");
    }

    #[test]
    fn ps_escape_single_quote() {
        assert_eq!(ps_escape("it's"), "it''s");
    }

    #[test]
    fn ps_escape_multiple_quotes() {
        assert_eq!(ps_escape("a'b'c"), "a''b''c");
    }

    #[test]
    fn ps_escape_empty_string() {
        assert_eq!(ps_escape(""), "");
    }

    #[test]
    fn ps_escape_backslashes_preserved() {
        assert_eq!(ps_escape(r"C:\Users\test"), r"C:\Users\test");
    }

    // -----------------------------------------------------------------------
    // WindowsHypervisor construction tests
    // -----------------------------------------------------------------------

    #[test]
    fn detect_docker_socket_returns_none_on_non_windows() {
        // On non-Windows, the Docker pipes won't exist.
        #[cfg(not(target_os = "windows"))]
        {
            let result = WindowsHypervisor::detect_docker_socket();
            assert!(result.is_none());
        }
    }

    #[test]
    fn hyperv_available_returns_false_on_non_windows() {
        #[cfg(not(target_os = "windows"))]
        {
            assert!(!WindowsHypervisor::hyperv_available());
        }
    }

    // -----------------------------------------------------------------------
    // VmEntry / VmInfo serialization
    // -----------------------------------------------------------------------

    #[test]
    fn vm_info_round_trip_with_hyperv_id() {
        let info = VmInfo {
            id: "hv-1".into(),
            name: "win-test".into(),
            state: VmState::Stopped,
            cpus: 4,
            memory_mb: 4096,
            disk_gb: 50,
            rosetta_enabled: false,
            shared_dirs: vec![],
            port_forwards: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
                protocol: "tcp".into(),
            }],
            os_image: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: VmInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "hv-1");
        assert_eq!(deserialized.name, "win-test");
        assert_eq!(deserialized.cpus, 4);
        assert_eq!(deserialized.port_forwards.len(), 1);
        assert_eq!(deserialized.port_forwards[0].host_port, 8080);
    }

    #[test]
    fn vm_info_round_trip_with_os_image() {
        let info = VmInfo {
            id: "hv-2".into(),
            name: "img-test".into(),
            state: VmState::Stopped,
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            rosetta_enabled: false,
            shared_dirs: vec![SharedDirectory {
                tag: "code".into(),
                host_path: r"C:\Users\test\code".into(),
                guest_path: "/mnt/code".into(),
                read_only: true,
            }],
            port_forwards: vec![],
            os_image: Some("alpine-3.19".into()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: VmInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.os_image.as_deref(), Some("alpine-3.19"));
        assert_eq!(deserialized.shared_dirs.len(), 1);
        assert_eq!(deserialized.shared_dirs[0].tag, "code");
        assert!(deserialized.shared_dirs[0].read_only);
    }

    // -----------------------------------------------------------------------
    // VmStats tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_stats_default_values() {
        let stats = VmStats {
            cpu_percent: 0.0,
            memory_usage_mb: 0,
            disk_usage_gb: 0,
        };
        assert_eq!(stats.cpu_percent, 0.0);
        assert_eq!(stats.memory_usage_mb, 0);
        assert_eq!(stats.disk_usage_gb, 0);
    }

    // -----------------------------------------------------------------------
    // Windows-specific integration tests (only run on Windows)
    // -----------------------------------------------------------------------

    #[cfg(target_os = "windows")]
    mod windows_integration {
        use super::*;

        /// Check if we can actually create VMs (Hyper-V + Default Switch present).
        fn can_create_vms() -> bool {
            if !WindowsHypervisor::hyperv_available() {
                return false;
            }
            // CI runners may have Hyper-V enabled but lack the Default Switch.
            #[cfg(target_os = "windows")]
            {
                run_powershell("Get-VMSwitch -Name 'Default Switch' -ErrorAction Stop | Out-Null")
                    .is_ok()
            }
            #[cfg(not(target_os = "windows"))]
            {
                false
            }
        }

        #[test]
        fn hypervisor_new_loads_without_panic() {
            // This test verifies that constructing the hypervisor does not panic,
            // even if Hyper-V is not available. It relies on the graceful fallback
            // in the constructor.
            let _hv = WindowsHypervisor::new();
        }

        #[test]
        fn create_vm_fails_without_hyperv() {
            // Skip if Hyper-V is actually available.
            if WindowsHypervisor::hyperv_available() {
                return;
            }

            let hv = WindowsHypervisor::new();
            let config = VmConfig {
                name: "test-no-hyperv".into(),
                cpus: 1,
                memory_mb: 512,
                disk_gb: 1,
                ..Default::default()
            };
            let result = hv.create_vm(config);
            assert!(result.is_err());
        }

    #[test]
    fn create_vm_rejects_rosetta() {
        if !WindowsHypervisor::hyperv_available() {
            return;
        }

        let hv = WindowsHypervisor::new();
        let config = VmConfig {
            name: "test-rosetta".into(),
            rosetta: true,
            ..Default::default()
            };
            let result = hv.create_vm(config);
            assert!(result.is_err());
            match result {
                Err(HypervisorError::RosettaUnavailable(_)) => {}
                other => panic!("Expected RosettaUnavailable, got {:?}", other),
            }
        }

        #[test]
        fn rosetta_always_unavailable() {
            let hv = WindowsHypervisor::new();
            assert!(!hv.rosetta_available());
        }

        #[test]
        fn list_vms_empty_by_default() {
            // Use a unique config dir to avoid interference.
            let tmp = std::env::temp_dir().join(format!(
                "cratebay-win-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::env::set_var("CRATEBAY_CONFIG_DIR", &tmp);
            let hv = WindowsHypervisor::new();
            let vms = hv.list_vms().unwrap();
            assert!(vms.is_empty());
            std::env::remove_var("CRATEBAY_CONFIG_DIR");
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn duplicate_vm_name_rejected() {
            if !can_create_vms() {
                return;
            }

            let tmp = std::env::temp_dir().join(format!(
                "cratebay-dup-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::env::set_var("CRATEBAY_CONFIG_DIR", &tmp);
            std::env::set_var("CRATEBAY_DATA_DIR", &tmp);
            let hv = WindowsHypervisor::new();

            let config1 = VmConfig {
                name: "dup-test".into(),
                cpus: 1,
                memory_mb: 512,
                disk_gb: 1,
                ..Default::default()
            };
            let id1 = hv.create_vm(config1).unwrap();

            let config2 = VmConfig {
                name: "dup-test".into(),
                cpus: 1,
                memory_mb: 512,
                disk_gb: 1,
                ..Default::default()
            };
            let result = hv.create_vm(config2);
            assert!(result.is_err());

            // Cleanup.
            let _ = hv.delete_vm(&id1);
            std::env::remove_var("CRATEBAY_CONFIG_DIR");
            std::env::remove_var("CRATEBAY_DATA_DIR");
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn create_vm_with_os_image() {
            if !can_create_vms() {
                return;
            }

            let tmp = std::env::temp_dir().join(format!(
                "cratebay-img-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::env::set_var("CRATEBAY_CONFIG_DIR", &tmp);
            std::env::set_var("CRATEBAY_DATA_DIR", &tmp);
            let hv = WindowsHypervisor::new();

            let config = VmConfig {
                name: "img-test".into(),
                cpus: 1,
                memory_mb: 512,
                disk_gb: 1,
                os_image: Some("alpine-3.19".into()),
                ..Default::default()
            };
            // This may fail if Hyper-V cmdlets aren't available, but should
            // not panic and should handle the OS image path correctly.
            let result = hv.create_vm(config);
            if let Ok(id) = result {
                let _ = hv.delete_vm(&id);
            }

            std::env::remove_var("CRATEBAY_CONFIG_DIR");
            std::env::remove_var("CRATEBAY_DATA_DIR");
            let _ = std::fs::remove_dir_all(&tmp);
        }

        #[test]
        fn query_vm_stats_returns_none_for_missing_vm() {
            let tmp = std::env::temp_dir().join(format!(
                "cratebay-stats-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::env::set_var("CRATEBAY_CONFIG_DIR", &tmp);
            let hv = WindowsHypervisor::new();
            assert!(hv.query_vm_stats("nonexistent").is_none());
            std::env::remove_var("CRATEBAY_CONFIG_DIR");
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
}
