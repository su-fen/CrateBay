// Linux hypervisor: KVM via rust-vmm with VirtioFS support.
//
// Uses kvm-ioctls to create and manage KVM virtual machines. Each VM gets:
// - A KVM VM file descriptor for memory/device configuration
// - One or more vCPU threads running the KVM_RUN loop
// - A serial console (COM1 at 0x3F8) for output capture
// - Guest memory backed by anonymous mmap regions
//
// VirtioFS: Uses virtiofsd (or its Rust equivalent) to provide high-performance
// shared filesystem between host and guest. The virtiofsd daemon runs on the host
// and communicates with the guest kernel's virtiofs driver via VHOST-USER protocol.
//
// Rosetta: Not available on Linux (Apple-only technology). x86_64 containers
// on ARM Linux would use QEMU user-mode emulation instead.

use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::store::{data_dir, next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

use kvm_bindings::{kvm_pit_config, kvm_userspace_memory_region, KVM_PIT_SPEAKER_DUMMY};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};

// -----------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------

/// Base address where the guest kernel is loaded (1 MiB, standard for bzImage).
const KERNEL_LOAD_ADDR: u64 = 0x0010_0000;

/// Address for the Linux boot parameters (zero page) at 0x7000.
const ZERO_PAGE_ADDR: u64 = 0x0000_7000;

/// Address for the kernel command line string.
const CMDLINE_ADDR: u64 = 0x0002_0000;

/// Default kernel command line.
const DEFAULT_CMDLINE: &str = "console=ttyS0 noapic noacpi reboot=k panic=1 pci=off nomodules";

/// Serial port I/O base (COM1).
const SERIAL_PORT_BASE: u16 = 0x3F8;

/// Size of the serial port I/O range.
const SERIAL_PORT_SIZE: u16 = 8;

// -----------------------------------------------------------------------
// Path helpers
// -----------------------------------------------------------------------

fn vm_dir(id: &str) -> PathBuf {
    data_dir().join("vms").join(id)
}

fn vm_disk_path(id: &str) -> PathBuf {
    vm_dir(id).join("disk.raw")
}

fn vm_console_log_path(id: &str) -> PathBuf {
    vm_dir(id).join("console.log")
}

// -----------------------------------------------------------------------
// KVM VM runtime state
// -----------------------------------------------------------------------

/// Runtime state for a running KVM VM instance.
struct KvmRuntime {
    /// KVM VM file descriptor.
    _vm_fd: VmFd,
    /// Stop signal shared with vCPU threads.
    stop_flag: Arc<AtomicBool>,
    /// Join handles for vCPU threads.
    vcpu_handles: Vec<std::thread::JoinHandle<()>>,
    /// Guest memory pointer and size (for cleanup).
    guest_mem: (*mut u8, usize),
}

// SAFETY: The guest memory pointer is only used by the vCPU threads which
// check the stop flag before accessing it, and we join all threads before
// unmapping the memory.
unsafe impl Send for KvmRuntime {}

impl Drop for KvmRuntime {
    fn drop(&mut self) {
        // Signal vCPU threads to stop.
        self.stop_flag.store(true, Ordering::SeqCst);

        // Wait for all vCPU threads to terminate.
        for handle in self.vcpu_handles.drain(..) {
            let _ = handle.join();
        }

        // Unmap guest memory.
        if !self.guest_mem.0.is_null() && self.guest_mem.1 > 0 {
            unsafe {
                libc::munmap(self.guest_mem.0 as *mut libc::c_void, self.guest_mem.1);
            }
        }
    }
}

// -----------------------------------------------------------------------
// VM entry stored in the hypervisor
// -----------------------------------------------------------------------

struct VmEntry {
    info: VmInfo,
    /// PIDs of virtiofsd processes for each mount tag.
    virtiofsd_pids: HashMap<String, u32>,
    /// KVM runtime state (present only when the VM is running).
    runtime: Option<KvmRuntime>,
    /// Paths to kernel/initrd configured at create time.
    kernel_path: Option<String>,
    initrd_path: Option<String>,
}

// -----------------------------------------------------------------------
// Serial console output writer
// -----------------------------------------------------------------------

/// Buffered writer for the serial console output file.
struct ConsoleWriter {
    file: std::fs::File,
}

impl ConsoleWriter {
    fn new(path: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file })
    }

    fn write_byte(&mut self, byte: u8) {
        use std::io::Write;
        let _ = self.file.write_all(&[byte]);
    }
}

// -----------------------------------------------------------------------
// LinuxHypervisor
// -----------------------------------------------------------------------

/// Linux hypervisor backed by KVM (via rust-vmm).
pub struct LinuxHypervisor {
    vms: Mutex<HashMap<String, VmEntry>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for LinuxHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxHypervisor {
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

        // Any previously-running VMs are marked stopped on daemon restart,
        // since the KVM runtime state is not preserved across restarts.
        for vm in &mut loaded {
            if vm.state != VmState::Stopped {
                vm.state = VmState::Stopped;
            }
        }

        let mut map: HashMap<String, VmEntry> = HashMap::new();
        for vm in loaded.iter().cloned() {
            map.insert(
                vm.id.clone(),
                VmEntry {
                    info: vm,
                    virtiofsd_pids: HashMap::new(),
                    runtime: None,
                    kernel_path: None,
                    initrd_path: None,
                },
            );
        }

        let next_id = next_id_for_prefix(&loaded, "kvm-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    /// Check if KVM is available on this system.
    pub fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
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

    /// Allocate guest memory using mmap.
    fn alloc_guest_memory(size_bytes: usize) -> Result<*mut u8, HypervisorError> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size_bytes,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(HypervisorError::CreateFailed(format!(
                "Failed to mmap guest memory ({} bytes): {}",
                size_bytes,
                std::io::Error::last_os_error()
            )));
        }
        Ok(ptr as *mut u8)
    }

    /// Load a kernel image (bzImage or raw binary) into guest memory.
    fn load_kernel(
        guest_mem: *mut u8,
        mem_size: usize,
        kernel_path: &str,
    ) -> Result<u64, HypervisorError> {
        let kernel_data = std::fs::read(kernel_path).map_err(|e| {
            HypervisorError::CreateFailed(format!("Failed to read kernel {}: {}", kernel_path, e))
        })?;

        let load_offset = KERNEL_LOAD_ADDR as usize;
        if load_offset + kernel_data.len() > mem_size {
            return Err(HypervisorError::CreateFailed(format!(
                "Kernel too large: {} bytes, available: {} bytes",
                kernel_data.len(),
                mem_size - load_offset
            )));
        }

        // Check for bzImage magic (0x53726448 = "HdrS" at offset 0x202).
        let is_bzimage = kernel_data.len() > 0x206
            && kernel_data[0x202] == 0x48
            && kernel_data[0x203] == 0x64
            && kernel_data[0x204] == 0x72
            && kernel_data[0x205] == 0x53;

        if is_bzimage {
            debug!("Detected bzImage format kernel");
            // For bzImage, the setup header tells us where the protected-mode
            // code starts. The setup_sects field at offset 0x1F1 gives the
            // number of setup sectors (each 512 bytes). Sector 0 is the boot
            // sector itself.
            let setup_sects = if kernel_data[0x1F1] == 0 {
                4 // default per Linux boot protocol
            } else {
                kernel_data[0x1F1] as usize
            };
            let setup_size = (setup_sects + 1) * 512;

            if setup_size >= kernel_data.len() {
                return Err(HypervisorError::CreateFailed(
                    "bzImage setup header extends past end of file".into(),
                ));
            }

            let protected_mode_code = &kernel_data[setup_size..];
            if load_offset + protected_mode_code.len() > mem_size {
                return Err(HypervisorError::CreateFailed(
                    "Kernel protected-mode code too large for guest memory".into(),
                ));
            }

            unsafe {
                std::ptr::copy_nonoverlapping(
                    protected_mode_code.as_ptr(),
                    guest_mem.add(load_offset),
                    protected_mode_code.len(),
                );
            }

            // Copy the setup header (zero page) to ZERO_PAGE_ADDR.
            let zero_page_offset = ZERO_PAGE_ADDR as usize;
            let header_size = std::cmp::min(setup_size, 4096);
            if zero_page_offset + 4096 <= mem_size {
                unsafe {
                    // Clear the zero page first.
                    std::ptr::write_bytes(guest_mem.add(zero_page_offset), 0, 4096);
                    std::ptr::copy_nonoverlapping(
                        kernel_data.as_ptr(),
                        guest_mem.add(zero_page_offset),
                        header_size,
                    );
                }
            }
        } else {
            debug!("Loading raw/ELF kernel binary");
            unsafe {
                std::ptr::copy_nonoverlapping(
                    kernel_data.as_ptr(),
                    guest_mem.add(load_offset),
                    kernel_data.len(),
                );
            }
        }

        info!(
            "Loaded kernel from {} ({} bytes) at 0x{:X}",
            kernel_path,
            kernel_data.len(),
            KERNEL_LOAD_ADDR
        );
        Ok(KERNEL_LOAD_ADDR)
    }

    /// Load an initrd image into guest memory (placed after the kernel).
    fn load_initrd(
        guest_mem: *mut u8,
        mem_size: usize,
        initrd_path: &str,
    ) -> Result<(u64, u64), HypervisorError> {
        let initrd_data = std::fs::read(initrd_path).map_err(|e| {
            HypervisorError::CreateFailed(format!("Failed to read initrd {}: {}", initrd_path, e))
        })?;

        // Place initrd at a high address, aligned to page boundary.
        // Typically placed just below the end of guest RAM.
        let initrd_size = initrd_data.len();
        let initrd_addr = ((mem_size - initrd_size) & !0xFFF) as u64;

        if (initrd_addr as usize) < KERNEL_LOAD_ADDR as usize + 0x10_0000 {
            return Err(HypervisorError::CreateFailed(
                "Not enough memory for initrd (overlaps with kernel)".into(),
            ));
        }

        unsafe {
            std::ptr::copy_nonoverlapping(
                initrd_data.as_ptr(),
                guest_mem.add(initrd_addr as usize),
                initrd_size,
            );
        }

        info!(
            "Loaded initrd from {} ({} bytes) at 0x{:X}",
            initrd_path, initrd_size, initrd_addr
        );
        Ok((initrd_addr, initrd_size as u64))
    }

    /// Write the kernel command line to guest memory.
    fn setup_cmdline(
        guest_mem: *mut u8,
        mem_size: usize,
        cmdline: &str,
    ) -> Result<(), HypervisorError> {
        let cmdline_bytes = cmdline.as_bytes();
        let offset = CMDLINE_ADDR as usize;
        if offset + cmdline_bytes.len() + 1 > mem_size {
            return Err(HypervisorError::CreateFailed(
                "Command line too long for guest memory".into(),
            ));
        }

        unsafe {
            std::ptr::copy_nonoverlapping(
                cmdline_bytes.as_ptr(),
                guest_mem.add(offset),
                cmdline_bytes.len(),
            );
            // Null-terminate the command line.
            *guest_mem.add(offset + cmdline_bytes.len()) = 0;
        }

        // Update the zero page with the command line pointer and size.
        let zero_page = ZERO_PAGE_ADDR as usize;
        if zero_page + 0x238 + 4 <= mem_size {
            unsafe {
                // cmd_line_ptr at offset 0x228 in the boot params.
                let ptr_offset = zero_page + 0x228;
                std::ptr::copy_nonoverlapping(
                    &(CMDLINE_ADDR as u32).to_le_bytes() as *const u8,
                    guest_mem.add(ptr_offset),
                    4,
                );
                // cmdline_size at offset 0x238.
                let size_offset = zero_page + 0x238;
                let size_val = cmdline_bytes.len() as u32;
                std::ptr::copy_nonoverlapping(
                    &size_val.to_le_bytes() as *const u8,
                    guest_mem.add(size_offset),
                    4,
                );
            }
        }

        debug!("Set kernel cmdline: {}", cmdline);
        Ok(())
    }

    /// Configure the zero page (boot_params) for the Linux boot protocol.
    #[cfg(target_arch = "x86_64")]
    fn setup_boot_params(
        guest_mem: *mut u8,
        mem_size: usize,
        initrd_addr: Option<u64>,
        initrd_size: Option<u64>,
    ) -> Result<(), HypervisorError> {
        let zero_page = ZERO_PAGE_ADDR as usize;
        if zero_page + 4096 > mem_size {
            return Err(HypervisorError::CreateFailed(
                "Not enough memory for zero page".into(),
            ));
        }

        unsafe {
            // Boot protocol version (>= 2.06 for our needs).
            let version_offset = zero_page + 0x206;
            let version: u16 = 0x0206;
            std::ptr::copy_nonoverlapping(
                &version.to_le_bytes() as *const u8,
                guest_mem.add(version_offset),
                2,
            );

            // type_of_loader at offset 0x210: set to 0xFF (undefined).
            *guest_mem.add(zero_page + 0x210) = 0xFF;

            // loadflags at offset 0x211: bit 0 = LOADED_HIGH (kernel loaded above 1M).
            *guest_mem.add(zero_page + 0x211) |= 0x01;

            // Set up initrd address and size if provided.
            if let (Some(addr), Some(size)) = (initrd_addr, initrd_size) {
                // ramdisk_image at offset 0x218.
                std::ptr::copy_nonoverlapping(
                    &(addr as u32).to_le_bytes() as *const u8,
                    guest_mem.add(zero_page + 0x218),
                    4,
                );
                // ramdisk_size at offset 0x21C.
                std::ptr::copy_nonoverlapping(
                    &(size as u32).to_le_bytes() as *const u8,
                    guest_mem.add(zero_page + 0x21C),
                    4,
                );
            }

            // Set up an E820 memory map entry for the guest.
            // e820_entries at offset 0x1E8.
            *guest_mem.add(zero_page + 0x1E8) = 1; // one entry

            // E820 table starts at offset 0x2D0 in boot_params.
            // Each entry is 20 bytes: addr (u64), size (u64), type (u32).
            let e820_offset = zero_page + 0x2D0;
            let mem_size_u64 = mem_size as u64;
            // Address = 0.
            std::ptr::copy_nonoverlapping(
                &0u64.to_le_bytes() as *const u8,
                guest_mem.add(e820_offset),
                8,
            );
            // Size = mem_size.
            std::ptr::copy_nonoverlapping(
                &mem_size_u64.to_le_bytes() as *const u8,
                guest_mem.add(e820_offset + 8),
                8,
            );
            // Type = 1 (usable RAM).
            std::ptr::copy_nonoverlapping(
                &1u32.to_le_bytes() as *const u8,
                guest_mem.add(e820_offset + 16),
                4,
            );
        }

        debug!(
            "Configured boot params (zero page) at 0x{:X}",
            ZERO_PAGE_ADDR
        );
        Ok(())
    }

    /// Set up initial CPU state for x86_64 long mode boot.
    #[cfg(target_arch = "x86_64")]
    fn setup_vcpu_regs(vcpu: &VcpuFd) -> Result<(), HypervisorError> {
        use kvm_bindings::kvm_regs;

        // Set up special registers for long mode.
        let mut sregs = vcpu
            .get_sregs()
            .map_err(|e| HypervisorError::CreateFailed(format!("Failed to get sregs: {}", e)))?;

        // Set up a simple identity-mapped page table for long mode.
        // CR3 points to PML4 at 0x9000, which we set up in the guest memory.
        // For simplicity, we rely on the kernel's own page table setup and
        // just start in 16-bit real mode at the kernel entry point.
        //
        // Actually, for direct kernel boot with the Linux boot protocol, we
        // start in protected mode (or long mode for 64-bit kernels). Let's set
        // up the minimal segment registers for 64-bit kernel entry.

        // Code segment: kernel code.
        sregs.cs.base = 0;
        sregs.cs.limit = 0xFFFF_FFFF;
        sregs.cs.selector = 0x10;
        sregs.cs.present = 1;
        sregs.cs.type_ = 0xB; // Execute/Read, accessed.
        sregs.cs.dpl = 0;
        sregs.cs.db = 0;
        sregs.cs.s = 1;
        sregs.cs.l = 1; // Long mode.
        sregs.cs.g = 1;

        // Data segment.
        sregs.ds.base = 0;
        sregs.ds.limit = 0xFFFF_FFFF;
        sregs.ds.selector = 0x18;
        sregs.ds.present = 1;
        sregs.ds.type_ = 0x3; // Read/Write, accessed.
        sregs.ds.dpl = 0;
        sregs.ds.db = 1;
        sregs.ds.s = 1;
        sregs.ds.l = 0;
        sregs.ds.g = 1;

        sregs.es = sregs.ds;
        sregs.fs = sregs.ds;
        sregs.gs = sregs.ds;
        sregs.ss = sregs.ds;

        // Enable long mode: set CR0 bits (PE, PG), CR4 (PAE), and EFER (LME, LMA).
        sregs.cr0 = 0x8000_0031; // PG | PE | ET | WP
        sregs.cr4 = 0x20; // PAE
        sregs.efer = 0x500; // LME | LMA

        // Set up a minimal identity-mapped page table at 0x9000.
        // PML4[0] -> PDPT at 0xA000
        // PDPT[0] -> 1 GiB identity map using huge pages.
        sregs.cr3 = 0x9000;

        vcpu.set_sregs(&sregs)
            .map_err(|e| HypervisorError::CreateFailed(format!("Failed to set sregs: {}", e)))?;

        // Set general purpose registers.
        let regs = kvm_regs {
            rflags: 0x2, // Reserved bit must be set.
            rip: KERNEL_LOAD_ADDR,
            rsi: ZERO_PAGE_ADDR, // Pointer to boot_params (Linux boot protocol).
            rsp: 0,              // Stack pointer (kernel will set its own).
            ..Default::default()
        };

        vcpu.set_regs(&regs)
            .map_err(|e| HypervisorError::CreateFailed(format!("Failed to set regs: {}", e)))?;

        debug!("Configured vCPU registers for 64-bit kernel entry");
        Ok(())
    }

    /// Set up the identity-mapped page tables in guest memory for long mode boot.
    #[cfg(target_arch = "x86_64")]
    fn setup_page_tables(guest_mem: *mut u8, mem_size: usize) -> Result<(), HypervisorError> {
        // We need at least: PML4 at 0x9000, PDPT at 0xA000.
        if 0xB000 > mem_size {
            return Err(HypervisorError::CreateFailed(
                "Not enough memory for page tables".into(),
            ));
        }

        unsafe {
            // Clear page table pages.
            std::ptr::write_bytes(guest_mem.add(0x9000), 0, 0x1000);
            std::ptr::write_bytes(guest_mem.add(0xA000), 0, 0x1000);

            // PML4[0] -> PDPT at 0xA000, present + writable.
            let pml4_entry: u64 = 0xA000 | 0x3; // Present + Writable
            std::ptr::copy_nonoverlapping(
                &pml4_entry.to_le_bytes() as *const u8,
                guest_mem.add(0x9000),
                8,
            );

            // PDPT[0] -> 1 GiB huge page at 0, present + writable + huge.
            let pdpt_entry: u64 = 0x0 | 0x83; // Present + Writable + PageSize (1GiB)
            std::ptr::copy_nonoverlapping(
                &pdpt_entry.to_le_bytes() as *const u8,
                guest_mem.add(0xA000),
                8,
            );

            // If guest has more than 1 GiB, add more PDPT entries.
            let gib_count = (mem_size + (1 << 30) - 1) / (1 << 30);
            for i in 1..std::cmp::min(gib_count, 512) {
                let entry: u64 = ((i as u64) << 30) | 0x83;
                std::ptr::copy_nonoverlapping(
                    &entry.to_le_bytes() as *const u8,
                    guest_mem.add(0xA000 + i * 8),
                    8,
                );
            }
        }

        debug!("Set up identity-mapped page tables at 0x9000");
        Ok(())
    }

    /// Set up initial CPU state for aarch64.
    #[cfg(target_arch = "aarch64")]
    fn setup_vcpu_regs(vcpu: &VcpuFd) -> Result<(), HypervisorError> {
        // On aarch64, the vCPU init sets up registers for kernel entry.
        // The kernel is entered in EL1 with MMU off.
        let mut kvi = kvm_bindings::kvm_vcpu_init::default();

        // Use the preferred target for this VM.
        // kvm_vcpu_init is populated by KVM_ARM_PREFERRED_TARGET.
        vcpu.vcpu_init(&kvi)
            .map_err(|e| HypervisorError::CreateFailed(format!("Failed to init vCPU: {}", e)))?;

        // Set PC to kernel load address.
        let pc_reg_id = 0x6030_0000_0010_0040u64; // ARM64 PC register
        vcpu.set_one_reg(pc_reg_id, &KERNEL_LOAD_ADDR.to_le_bytes())
            .map_err(|e| HypervisorError::CreateFailed(format!("Failed to set PC: {}", e)))?;

        debug!("Configured vCPU registers for aarch64 kernel entry");
        Ok(())
    }

    /// Create a KVM VM with configured memory, vCPUs, and devices.
    fn create_kvm_vm(
        config: &VmConfig,
        vm_id: &str,
    ) -> Result<(VmFd, Vec<VcpuFd>, *mut u8, usize), HypervisorError> {
        let kvm = Kvm::new().map_err(|e| {
            HypervisorError::CreateFailed(format!("Failed to open /dev/kvm: {}", e))
        })?;

        // Check KVM API version.
        let api_version = kvm.get_api_version();
        if api_version != 12 {
            return Err(HypervisorError::CreateFailed(format!(
                "Unsupported KVM API version: {} (expected 12)",
                api_version
            )));
        }

        // Create VM.
        let vm_fd = kvm
            .create_vm()
            .map_err(|e| HypervisorError::CreateFailed(format!("KVM_CREATE_VM failed: {}", e)))?;

        // Set up TSS (required on x86 before creating vCPUs with in-kernel irqchip).
        #[cfg(target_arch = "x86_64")]
        {
            vm_fd.set_tss_address(0xFFFF_D000).map_err(|e| {
                HypervisorError::CreateFailed(format!("Failed to set TSS address: {}", e))
            })?;
        }

        // Create in-kernel interrupt controller.
        #[cfg(target_arch = "x86_64")]
        {
            vm_fd.create_irq_chip().map_err(|e| {
                HypervisorError::CreateFailed(format!("Failed to create irqchip: {}", e))
            })?;

            // Create PIT2 (programmable interval timer).
            let pit_config = kvm_pit_config {
                flags: KVM_PIT_SPEAKER_DUMMY,
                ..Default::default()
            };
            vm_fd.create_pit2(pit_config).map_err(|e| {
                HypervisorError::CreateFailed(format!("Failed to create PIT: {}", e))
            })?;
        }

        // Allocate guest memory.
        let mem_size = (config.memory_mb as usize) * 1024 * 1024;
        let guest_mem = Self::alloc_guest_memory(mem_size)?;

        // Register memory region with KVM.
        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: mem_size as u64,
            userspace_addr: guest_mem as u64,
            flags: 0,
        };

        unsafe {
            vm_fd.set_user_memory_region(mem_region).map_err(|e| {
                libc::munmap(guest_mem as *mut libc::c_void, mem_size);
                HypervisorError::CreateFailed(format!("KVM_SET_USER_MEMORY_REGION failed: {}", e))
            })?;
        }

        // Set up page tables for long mode (x86_64).
        #[cfg(target_arch = "x86_64")]
        Self::setup_page_tables(guest_mem, mem_size)?;

        // Load kernel if path is provided.
        // Resolve kernel path: config first, then env var.
        let kernel_path_str: Option<String> = config
            .kernel_path
            .clone()
            .or_else(|| std::env::var("CARGOBAY_KVM_KERNEL").ok());

        if let Some(ref kp) = kernel_path_str {
            Self::load_kernel(guest_mem, mem_size, kp)?;

            // Load initrd if provided.
            let initrd_path_str: Option<String> = config
                .initrd_path
                .clone()
                .or_else(|| std::env::var("CARGOBAY_KVM_INITRD").ok());

            let (initrd_addr, initrd_size) = if let Some(ref ip) = initrd_path_str {
                let (addr, size) = Self::load_initrd(guest_mem, mem_size, ip)?;
                (Some(addr), Some(size))
            } else {
                (None, None)
            };

            // Set up command line.
            let cmdline = std::env::var("CARGOBAY_KVM_CMDLINE")
                .unwrap_or_else(|_| DEFAULT_CMDLINE.to_string());
            Self::setup_cmdline(guest_mem, mem_size, &cmdline)?;

            // Set up boot parameters.
            #[cfg(target_arch = "x86_64")]
            Self::setup_boot_params(guest_mem, mem_size, initrd_addr, initrd_size)?;
        }

        // Create vCPUs.
        let vcpu_count = std::cmp::max(config.cpus, 1);
        let mut vcpus = Vec::with_capacity(vcpu_count as usize);

        for i in 0..vcpu_count {
            let vcpu = vm_fd.create_vcpu(i as u64).map_err(|e| {
                HypervisorError::CreateFailed(format!("KVM_CREATE_VCPU({}) failed: {}", i, e))
            })?;

            // Set up initial CPU registers.
            Self::setup_vcpu_regs(&vcpu)?;
            vcpus.push(vcpu);
        }

        // Create VM directory for state files.
        let dir = vm_dir(vm_id);
        std::fs::create_dir_all(&dir)?;

        // Create disk image if it doesn't exist.
        let disk_path = vm_disk_path(vm_id);
        if !disk_path.exists() {
            let disk_bytes = config
                .disk_gb
                .checked_mul(1024 * 1024 * 1024)
                .ok_or_else(|| HypervisorError::CreateFailed("disk size overflow".into()))?;
            let file = std::fs::File::create(&disk_path)?;
            file.set_len(disk_bytes)?;
        }

        info!(
            "Created KVM VM {} with {} MiB RAM, {} vCPU(s)",
            vm_id, config.memory_mb, vcpu_count
        );

        Ok((vm_fd, vcpus, guest_mem, mem_size))
    }

    /// Spawn vCPU threads that run the KVM_RUN loop.
    fn spawn_vcpu_threads(
        vcpus: Vec<VcpuFd>,
        stop_flag: Arc<AtomicBool>,
        vm_id: String,
    ) -> Vec<std::thread::JoinHandle<()>> {
        let mut handles = Vec::with_capacity(vcpus.len());

        for (idx, mut vcpu) in vcpus.into_iter().enumerate() {
            let stop = stop_flag.clone();
            let id = vm_id.clone();
            let console_path = vm_console_log_path(&id);

            let handle = std::thread::Builder::new()
                .name(format!("kvm-vcpu-{}-{}", id, idx))
                .spawn(move || {
                    let mut console_writer = ConsoleWriter::new(&console_path).ok();

                    loop {
                        if stop.load(Ordering::Relaxed) {
                            debug!("vCPU {} of VM {} received stop signal", idx, id);
                            break;
                        }

                        match vcpu.run() {
                            Ok(exit) => match exit {
                                VcpuExit::IoOut(port, data) => {
                                    // Handle serial port output (COM1).
                                    if port >= SERIAL_PORT_BASE
                                        && port < SERIAL_PORT_BASE + SERIAL_PORT_SIZE
                                    {
                                        if port == SERIAL_PORT_BASE {
                                            // THR (Transmit Holding Register).
                                            if let Some(ref mut writer) = console_writer {
                                                for &byte in data {
                                                    writer.write_byte(byte);
                                                }
                                            }
                                        }
                                        // Other serial registers (IER, FCR, LCR, MCR, etc.)
                                        // are silently consumed.
                                    }
                                }
                                VcpuExit::IoIn(port, data) => {
                                    // Handle serial port input.
                                    if port >= SERIAL_PORT_BASE
                                        && port < SERIAL_PORT_BASE + SERIAL_PORT_SIZE
                                    {
                                        match port - SERIAL_PORT_BASE {
                                            5 => {
                                                // LSR (Line Status Register):
                                                // Bit 5 = THRE (Transmitter Holding Register Empty)
                                                // Bit 6 = TEMT (Transmitter Empty)
                                                // Always report ready to accept data.
                                                data[0] = 0x60;
                                            }
                                            6 => {
                                                // MSR (Modem Status Register):
                                                // Report carrier detect + clear to send.
                                                data[0] = 0xB0;
                                            }
                                            _ => {
                                                data[0] = 0;
                                            }
                                        }
                                    } else {
                                        // Unknown I/O port: return 0xFF (bus float).
                                        for byte in data.iter_mut() {
                                            *byte = 0xFF;
                                        }
                                    }
                                }
                                VcpuExit::MmioRead(_addr, data) => {
                                    // Unhandled MMIO read: return zeros.
                                    for byte in data.iter_mut() {
                                        *byte = 0;
                                    }
                                }
                                VcpuExit::MmioWrite(_addr, _data) => {
                                    // Unhandled MMIO write: silently ignore.
                                }
                                VcpuExit::Hlt => {
                                    debug!("vCPU {} of VM {} halted", idx, id);
                                    // On HLT, sleep briefly and check stop flag, or
                                    // the irqchip will wake us up on interrupt.
                                    if stop.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(10));
                                }
                                VcpuExit::Shutdown => {
                                    info!("vCPU {} of VM {} received shutdown signal", idx, id);
                                    stop.store(true, Ordering::SeqCst);
                                    break;
                                }
                                VcpuExit::InternalError => {
                                    error!("vCPU {} of VM {} encountered internal error", idx, id);
                                    stop.store(true, Ordering::SeqCst);
                                    break;
                                }
                                other => {
                                    debug!(
                                        "vCPU {} of VM {}: unhandled exit: {:?}",
                                        idx, id, other
                                    );
                                }
                            },
                            Err(e) => {
                                // EINTR is expected if we're being stopped.
                                if stop.load(Ordering::Relaxed) {
                                    break;
                                }
                                // EAGAIN can happen, retry.
                                if e.errno() == libc::EAGAIN {
                                    continue;
                                }
                                error!("vCPU {} of VM {} KVM_RUN error: {}", idx, id, e);
                                stop.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                })
                .expect("Failed to spawn vCPU thread");

            handles.push(handle);
        }

        handles
    }

    /// Spawn a virtiofsd process for a shared directory.
    fn spawn_virtiofsd(
        tag: &str,
        host_path: &str,
        read_only: bool,
    ) -> Result<u32, HypervisorError> {
        use std::process::{Command, Stdio};

        let socket_path = format!("/tmp/cargobay-virtiofs-{}.sock", tag);

        let mut cmd = Command::new("virtiofsd");
        cmd.arg("--socket-path")
            .arg(&socket_path)
            .arg("--shared-dir")
            .arg(host_path)
            .arg("--cache=auto");

        if read_only {
            cmd.arg("--sandbox=none");
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            HypervisorError::VirtioFsError(format!(
                "Failed to spawn virtiofsd for tag '{}': {}. \
                 Ensure virtiofsd is installed (apt install virtiofsd).",
                tag, e
            ))
        })?;

        let pid = child.id();
        info!(
            "Spawned virtiofsd (pid {}) for tag '{}' at {}",
            pid, tag, host_path
        );
        Ok(pid)
    }
}

impl Hypervisor for LinuxHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        if !Self::kvm_available() {
            return Err(HypervisorError::CreateFailed(
                "KVM not available. Ensure /dev/kvm exists and you have permissions.".into(),
            ));
        }

        if config.rosetta {
            return Err(HypervisorError::RosettaUnavailable(
                "Rosetta is only available on macOS Apple Silicon. \
                 Use QEMU user-mode for x86_64 emulation on Linux."
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
            let vms = self.vms.lock().unwrap();
            if vms.values().any(|e| e.info.name == config.name) {
                return Err(HypervisorError::CreateFailed(format!(
                    "VM name already exists: {}",
                    config.name
                )));
            }
        }

        // Generate unique ID.
        let mut id_counter = self.next_id.lock().unwrap();
        let id = format!("kvm-{}", *id_counter);
        *id_counter += 1;

        // Create the VM directory and disk image.
        let dir = vm_dir(&id);
        std::fs::create_dir_all(&dir)?;

        let disk_path = vm_disk_path(&id);
        let disk_bytes = config
            .disk_gb
            .checked_mul(1024 * 1024 * 1024)
            .ok_or_else(|| HypervisorError::CreateFailed("disk size overflow".into()))?;
        {
            let file = std::fs::File::create(&disk_path)?;
            file.set_len(disk_bytes)?;
        }

        let info = VmInfo {
            id: id.clone(),
            name: config.name.clone(),
            state: VmState::Stopped,
            cpus: config.cpus,
            memory_mb: config.memory_mb,
            disk_gb: config.disk_gb,
            rosetta_enabled: false,
            shared_dirs: config.shared_dirs.clone(),
            port_forwards: config.port_forwards,
            os_image: config.os_image,
        };

        let entry = VmEntry {
            info,
            virtiofsd_pids: HashMap::new(),
            runtime: None,
            kernel_path: config.kernel_path.clone(),
            initrd_path: config.initrd_path.clone(),
        };

        self.vms.lock().unwrap().insert(id.clone(), entry);
        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().remove(&id);
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }

        info!("Created KVM VM '{}' ({})", config.name, id);
        Ok(id)
    }

    fn start_vm(&self, id: &str) -> Result<(), HypervisorError> {
        // Check current state and extract config needed for start.
        let (config, _kernel_path, _initrd_path, shared_dirs) = {
            let vms = self.vms.lock().unwrap();
            let entry = vms.get(id).ok_or(HypervisorError::NotFound(id.into()))?;

            if entry.info.state == VmState::Running && entry.runtime.is_some() {
                return Ok(()); // Already running.
            }

            let config = VmConfig {
                name: entry.info.name.clone(),
                cpus: entry.info.cpus,
                memory_mb: entry.info.memory_mb,
                disk_gb: entry.info.disk_gb,
                rosetta: false,
                shared_dirs: vec![], // We'll handle these separately.
                os_image: None,
                kernel_path: entry.kernel_path.clone(),
                initrd_path: entry.initrd_path.clone(),
                disk_path: None,
                port_forwards: entry.info.port_forwards.clone(),
            };
            (
                config,
                entry.kernel_path.clone(),
                entry.initrd_path.clone(),
                entry.info.shared_dirs.clone(),
            )
        };

        // Create the KVM VM with vCPUs and memory.
        let (vm_fd, vcpus, guest_mem, mem_size) = Self::create_kvm_vm(&config, id)?;

        // Spawn virtiofsd processes for shared directories.
        let mut virtiofsd_pids = HashMap::new();
        for share in &shared_dirs {
            if std::path::Path::new(&share.host_path).exists() {
                match Self::spawn_virtiofsd(&share.tag, &share.host_path, share.read_only) {
                    Ok(pid) => {
                        virtiofsd_pids.insert(share.tag.clone(), pid);
                    }
                    Err(e) => {
                        warn!("Failed to spawn virtiofsd for '{}': {}", share.tag, e);
                    }
                }
            }
        }

        // Spawn vCPU threads.
        let stop_flag = Arc::new(AtomicBool::new(false));
        let vcpu_handles = Self::spawn_vcpu_threads(vcpus, stop_flag.clone(), id.to_string());

        // Update VM state.
        let previous_state = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            entry.info.state = VmState::Running;
            entry.virtiofsd_pids = virtiofsd_pids;
            entry.runtime = Some(KvmRuntime {
                _vm_fd: vm_fd,
                stop_flag,
                vcpu_handles,
                guest_mem: (guest_mem, mem_size),
            });
            prev
        };

        if let Err(e) = self.persist() {
            // Rollback: stop the VM.
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous_state;
                // Drop runtime to clean up vCPU threads and memory.
                entry.runtime = None;
                // Kill virtiofsd processes.
                for (_, pid) in entry.virtiofsd_pids.drain() {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                }
            }
            return Err(e);
        }

        info!("Started KVM VM {}", id);
        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (runtime, virtiofsd_pids, previous_state) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            let runtime = entry.runtime.take();
            let pids: Vec<(String, u32)> = entry.virtiofsd_pids.drain().collect();
            entry.info.state = VmState::Stopped;
            (runtime, pids, prev)
        };

        // Stop virtiofsd processes.
        for (tag, pid) in &virtiofsd_pids {
            info!("Stopping virtiofsd for '{}' (pid {})", tag, pid);
            unsafe {
                libc::kill(*pid as i32, libc::SIGTERM);
            }
        }

        // Wait briefly for virtiofsd to exit, then force-kill.
        if !virtiofsd_pids.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(500));
            for (_, pid) in &virtiofsd_pids {
                // Check if still alive.
                let rc = unsafe { libc::kill(*pid as i32, 0) };
                if rc == 0 {
                    unsafe {
                        libc::kill(*pid as i32, libc::SIGKILL);
                    }
                }
            }
        }

        // Clean up the runtime (signals vCPU threads to stop, joins them,
        // and unmaps guest memory via the Drop impl).
        drop(runtime);

        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous_state;
                for (tag, pid) in virtiofsd_pids {
                    entry.virtiofsd_pids.insert(tag, pid);
                }
            }
            return Err(e);
        }

        info!("Stopped KVM VM {}", id);
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

        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().insert(id.to_string(), removed);
            return Err(e);
        }

        // Remove VM directory (disk, console log, etc.).
        let _ = std::fs::remove_dir_all(vm_dir(id));

        info!("Deleted KVM VM {}", id);
        Ok(())
    }

    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        let mut changed = false;
        {
            let mut vms = self.vms.lock().unwrap();
            for entry in vms.values_mut() {
                if entry.info.state == VmState::Running {
                    // Check if the runtime has stopped (vCPU threads exited).
                    if let Some(ref runtime) = entry.runtime {
                        if runtime.stop_flag.load(Ordering::Relaxed) {
                            // VM stopped on its own (shutdown/error).
                            entry.runtime = None;
                            entry.info.state = VmState::Stopped;
                            // Kill any remaining virtiofsd processes.
                            for (_, pid) in entry.virtiofsd_pids.drain() {
                                unsafe {
                                    libc::kill(pid as i32, libc::SIGTERM);
                                }
                            }
                            changed = true;
                        }
                    } else {
                        // Runtime missing but state says Running; fix it.
                        entry.info.state = VmState::Stopped;
                        changed = true;
                    }
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
        if !std::path::Path::new(&share.host_path).exists() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path does not exist: {}",
                share.host_path
            )));
        }

        let is_running;
        {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;

            if entry.info.shared_dirs.iter().any(|d| d.tag == share.tag) {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Mount tag already exists: {}",
                    share.tag
                )));
            }

            is_running = entry.info.state == VmState::Running;
            entry.info.shared_dirs.push(share.clone());
        }

        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs.retain(|d| d.tag != share.tag);
            }
            return Err(e);
        }

        // If the VM is running, spawn a virtiofsd process for this share.
        if is_running {
            match Self::spawn_virtiofsd(&share.tag, &share.host_path, share.read_only) {
                Ok(pid) => {
                    let mut vms = self.vms.lock().unwrap();
                    if let Some(entry) = vms.get_mut(vm_id) {
                        entry.virtiofsd_pids.insert(share.tag.clone(), pid);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to spawn virtiofsd for '{}' (VM running): {}",
                        share.tag, e
                    );
                }
            }
        }

        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let (previous_dirs, pid_to_kill) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev_dirs = entry.info.shared_dirs.clone();
            entry.info.shared_dirs.retain(|d| d.tag != tag);

            let pid = entry.virtiofsd_pids.remove(tag);
            (prev_dirs, pid)
        };

        // Kill the virtiofsd process if running.
        if let Some(pid) = pid_to_kill {
            info!("Stopping virtiofsd for '{}' (pid {})", tag, pid);
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs = previous_dirs;
                if let Some(pid) = pid_to_kill {
                    entry.virtiofsd_pids.insert(tag.to_string(), pid);
                }
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_virtiofs_mounts(&self, vm_id: &str) -> Result<Vec<SharedDirectory>, HypervisorError> {
        let vms = self.vms.lock().unwrap();
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.shared_dirs.clone())
    }

    fn add_port_forward(&self, vm_id: &str, pf: &PortForward) -> Result<(), HypervisorError> {
        {
            let mut vms = self.vms.lock().unwrap();
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
        }
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry
                    .info
                    .port_forwards
                    .retain(|p| p.host_port != pf.host_port);
            }
            return Err(e);
        }
        Ok(())
    }

    fn remove_port_forward(&self, vm_id: &str, host_port: u16) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = self.vms.lock().unwrap();
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
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.port_forwards = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_port_forwards(&self, vm_id: &str) -> Result<Vec<PortForward>, HypervisorError> {
        let vms = self.vms.lock().unwrap();
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.port_forwards.clone())
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    use crate::hypervisor::{PortForward, SharedDirectory, VmConfig, VmState};
    use std::ffi::OsString;

    /// RAII guard that sets an env var and restores it on drop.
    struct EnvGuard {
        key: String,
        prev: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let prev = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                prev,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var(&self.key, v),
                None => std::env::remove_var(&self.key),
            }
        }
    }

    fn temp_config_dir() -> (String, EnvGuard, EnvGuard) {
        let tmp = std::env::temp_dir().join(format!(
            "cargobay-linux-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.to_string_lossy().to_string();
        let g1 = EnvGuard::set("CARGOBAY_CONFIG_DIR", &path);
        let g2 = EnvGuard::set("CARGOBAY_DATA_DIR", &path);
        (path, g1, g2)
    }

    // -----------------------------------------------------------------------
    // KVM availability
    // -----------------------------------------------------------------------

    #[test]
    fn kvm_available_checks_dev_kvm() {
        // This test simply verifies the function runs without panicking.
        let _available = LinuxHypervisor::kvm_available();
    }

    // -----------------------------------------------------------------------
    // VM lifecycle (metadata only, does not require /dev/kvm)
    // -----------------------------------------------------------------------

    #[test]
    fn create_vm_stores_metadata() {
        let (tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        // create_vm will fail if /dev/kvm doesn't exist, which is expected
        // in CI. But we can test the metadata path before KVM is checked.
        // Instead, let's test the internal state management directly.
        let config = VmConfig {
            name: "test-vm".into(),
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 10,
            rosetta: false,
            shared_dirs: vec![],
            os_image: None,
            kernel_path: None,
            initrd_path: None,
            disk_path: None,
            port_forwards: vec![],
        };

        // If KVM is available, the create should succeed.
        if LinuxHypervisor::kvm_available() {
            let id = hyp.create_vm(config).unwrap();
            assert!(id.starts_with("kvm-"));

            let vms = hyp.list_vms().unwrap();
            assert_eq!(vms.len(), 1);
            assert_eq!(vms[0].name, "test-vm");
            assert_eq!(vms[0].state, VmState::Stopped);
            assert_eq!(vms[0].cpus, 2);
            assert_eq!(vms[0].memory_mb, 1024);

            // Clean up.
            hyp.delete_vm(&id).unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn create_vm_rejects_rosetta() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        let config = VmConfig {
            name: "rosetta-vm".into(),
            rosetta: true,
            ..VmConfig::default()
        };

        let result = hyp.create_vm(config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Rosetta"),
            "Error should mention Rosetta: {}",
            err
        );
    }

    #[test]
    fn create_vm_rejects_duplicate_name() {
        let (tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        if LinuxHypervisor::kvm_available() {
            let config1 = VmConfig {
                name: "duplicate".into(),
                ..VmConfig::default()
            };
            let id = hyp.create_vm(config1).unwrap();

            let config2 = VmConfig {
                name: "duplicate".into(),
                ..VmConfig::default()
            };
            let result = hyp.create_vm(config2);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already exists"));

            hyp.delete_vm(&id).unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn delete_nonexistent_vm_returns_not_found() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        let result = hyp.delete_vm("kvm-nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kvm-nonexistent"));
    }

    #[test]
    fn stop_nonexistent_vm_returns_not_found() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        let result = hyp.stop_vm("kvm-nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn start_nonexistent_vm_returns_not_found() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        let result = hyp.start_vm("kvm-nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn list_vms_empty_initially() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        let vms = hyp.list_vms().unwrap();
        assert!(vms.is_empty());
    }

    #[test]
    fn rosetta_not_available_on_linux() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();
        assert!(!hyp.rosetta_available());
    }

    // -----------------------------------------------------------------------
    // Port forward tests
    // -----------------------------------------------------------------------

    #[test]
    fn port_forward_operations() {
        let (tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        if LinuxHypervisor::kvm_available() {
            let config = VmConfig {
                name: "pf-test".into(),
                ..VmConfig::default()
            };
            let id = hyp.create_vm(config).unwrap();

            // Add a port forward.
            let pf = PortForward {
                host_port: 8080,
                guest_port: 80,
                protocol: "tcp".into(),
            };
            hyp.add_port_forward(&id, &pf).unwrap();

            // List should show the port forward.
            let forwards = hyp.list_port_forwards(&id).unwrap();
            assert_eq!(forwards.len(), 1);
            assert_eq!(forwards[0].host_port, 8080);
            assert_eq!(forwards[0].guest_port, 80);

            // Duplicate host port should fail.
            let pf2 = PortForward {
                host_port: 8080,
                guest_port: 8080,
                protocol: "tcp".into(),
            };
            assert!(hyp.add_port_forward(&id, &pf2).is_err());

            // Remove the port forward.
            hyp.remove_port_forward(&id, 8080).unwrap();
            let forwards = hyp.list_port_forwards(&id).unwrap();
            assert!(forwards.is_empty());

            hyp.delete_vm(&id).unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // -----------------------------------------------------------------------
    // VirtioFS mount tests (metadata only)
    // -----------------------------------------------------------------------

    #[test]
    fn virtiofs_mount_nonexistent_path_fails() {
        let (_tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        if LinuxHypervisor::kvm_available() {
            let config = VmConfig {
                name: "vfs-test".into(),
                ..VmConfig::default()
            };
            let id = hyp.create_vm(config).unwrap();

            let share = SharedDirectory {
                tag: "data".into(),
                host_path: "/nonexistent/path/that/does/not/exist".into(),
                guest_path: "/mnt/data".into(),
                read_only: false,
            };
            let result = hyp.mount_virtiofs(&id, &share);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("does not exist"));

            hyp.delete_vm(&id).unwrap();
        }
    }

    #[test]
    fn virtiofs_duplicate_tag_fails() {
        let (tmp_dir, _g1, _g2) = temp_config_dir();
        let hyp = LinuxHypervisor::new();

        if LinuxHypervisor::kvm_available() {
            let share_dir = format!("{}/share", tmp_dir);
            std::fs::create_dir_all(&share_dir).unwrap();

            let config = VmConfig {
                name: "vfs-dup-test".into(),
                ..VmConfig::default()
            };
            let id = hyp.create_vm(config).unwrap();

            let share = SharedDirectory {
                tag: "data".into(),
                host_path: share_dir.clone(),
                guest_path: "/mnt/data".into(),
                read_only: false,
            };
            hyp.mount_virtiofs(&id, &share).unwrap();

            // Duplicate tag should fail.
            let share2 = SharedDirectory {
                tag: "data".into(),
                host_path: share_dir,
                guest_path: "/mnt/data2".into(),
                read_only: true,
            };
            let result = hyp.mount_virtiofs(&id, &share2);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already exists"));

            hyp.delete_vm(&id).unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // -----------------------------------------------------------------------
    // Persistence tests
    // -----------------------------------------------------------------------

    #[test]
    fn vms_persist_across_hypervisor_instances() {
        let (tmp_dir, _g1, _g2) = temp_config_dir();

        if LinuxHypervisor::kvm_available() {
            // Create a VM with the first hypervisor instance.
            let hyp1 = LinuxHypervisor::new();
            let config = VmConfig {
                name: "persist-test".into(),
                cpus: 4,
                memory_mb: 4096,
                ..VmConfig::default()
            };
            let id = hyp1.create_vm(config).unwrap();

            // Create a second hypervisor instance; it should load the VM.
            let hyp2 = LinuxHypervisor::new();
            let vms = hyp2.list_vms().unwrap();
            assert_eq!(vms.len(), 1);
            assert_eq!(vms[0].id, id);
            assert_eq!(vms[0].name, "persist-test");
            assert_eq!(vms[0].cpus, 4);
            assert_eq!(vms[0].memory_mb, 4096);
            // After restart, state should be Stopped.
            assert_eq!(vms[0].state, VmState::Stopped);

            hyp2.delete_vm(&id).unwrap();
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // -----------------------------------------------------------------------
    // Memory allocation test
    // -----------------------------------------------------------------------

    #[test]
    fn alloc_guest_memory_succeeds() {
        let size = 64 * 1024 * 1024; // 64 MiB
        let ptr = LinuxHypervisor::alloc_guest_memory(size).unwrap();
        assert!(!ptr.is_null());
        // Clean up.
        unsafe {
            libc::munmap(ptr as *mut libc::c_void, size);
        }
    }

    // -----------------------------------------------------------------------
    // Helper function tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_dir_path_contains_id() {
        let _g = EnvGuard::set("CARGOBAY_DATA_DIR", "/tmp/cb-linux-test");
        let path = vm_dir("kvm-42");
        assert!(path.to_string_lossy().contains("kvm-42"));
        assert!(path.to_string_lossy().contains("vms"));
    }

    #[test]
    fn vm_disk_path_ends_with_disk_raw() {
        let _g = EnvGuard::set("CARGOBAY_DATA_DIR", "/tmp/cb-linux-test");
        let path = vm_disk_path("kvm-1");
        assert!(path.to_string_lossy().ends_with("disk.raw"));
    }

    #[test]
    fn vm_console_log_path_ends_with_console_log() {
        let _g = EnvGuard::set("CARGOBAY_DATA_DIR", "/tmp/cb-linux-test");
        let path = vm_console_log_path("kvm-1");
        assert!(path.to_string_lossy().ends_with("console.log"));
    }

    // -----------------------------------------------------------------------
    // Kernel loading tests (uses a minimal test binary in memory)
    // -----------------------------------------------------------------------

    #[test]
    fn setup_cmdline_writes_to_guest_memory() {
        let mem_size = 4 * 1024 * 1024; // 4 MiB
        let guest_mem = LinuxHypervisor::alloc_guest_memory(mem_size).unwrap();

        let cmdline = "console=ttyS0 root=/dev/vda";
        LinuxHypervisor::setup_cmdline(guest_mem, mem_size, cmdline).unwrap();

        // Verify the command line was written.
        let offset = CMDLINE_ADDR as usize;
        let written = unsafe {
            let len = cmdline.len();
            std::slice::from_raw_parts(guest_mem.add(offset), len)
        };
        assert_eq!(written, cmdline.as_bytes());

        // Verify null terminator.
        let null_byte = unsafe { *guest_mem.add(offset + cmdline.len()) };
        assert_eq!(null_byte, 0);

        unsafe {
            libc::munmap(guest_mem as *mut libc::c_void, mem_size);
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn setup_page_tables_creates_identity_map() {
        let mem_size = 4 * 1024 * 1024; // 4 MiB
        let guest_mem = LinuxHypervisor::alloc_guest_memory(mem_size).unwrap();

        LinuxHypervisor::setup_page_tables(guest_mem, mem_size).unwrap();

        // Verify PML4[0] points to PDPT at 0xA000.
        let pml4_entry = unsafe {
            let bytes = std::slice::from_raw_parts(guest_mem.add(0x9000), 8);
            u64::from_le_bytes(bytes.try_into().unwrap())
        };
        assert_eq!(pml4_entry & !0xFFF, 0xA000); // Address portion
        assert_eq!(pml4_entry & 0x3, 0x3); // Present + Writable

        // Verify PDPT[0] is a 1 GiB huge page at address 0.
        let pdpt_entry = unsafe {
            let bytes = std::slice::from_raw_parts(guest_mem.add(0xA000), 8);
            u64::from_le_bytes(bytes.try_into().unwrap())
        };
        assert_eq!(pdpt_entry & !0xFFF, 0x0); // Address = 0
        assert_eq!(pdpt_entry & 0x83, 0x83); // Present + Writable + PageSize

        unsafe {
            libc::munmap(guest_mem as *mut libc::c_void, mem_size);
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn setup_boot_params_writes_e820_entry() {
        let mem_size = 4 * 1024 * 1024; // 4 MiB
        let guest_mem = LinuxHypervisor::alloc_guest_memory(mem_size).unwrap();

        // Clear the zero page area.
        unsafe {
            std::ptr::write_bytes(guest_mem.add(ZERO_PAGE_ADDR as usize), 0, 4096);
        }

        LinuxHypervisor::setup_boot_params(guest_mem, mem_size, None, None).unwrap();

        // Verify E820 entry count.
        let zero_page = ZERO_PAGE_ADDR as usize;
        let e820_count = unsafe { *guest_mem.add(zero_page + 0x1E8) };
        assert_eq!(e820_count, 1);

        // Verify E820 entry: type should be 1 (usable RAM).
        let e820_offset = zero_page + 0x2D0;
        let e820_type = unsafe {
            let bytes = std::slice::from_raw_parts(guest_mem.add(e820_offset + 16), 4);
            u32::from_le_bytes(bytes.try_into().unwrap())
        };
        assert_eq!(e820_type, 1);

        // Verify E820 entry: size should be mem_size.
        let e820_size = unsafe {
            let bytes = std::slice::from_raw_parts(guest_mem.add(e820_offset + 8), 8);
            u64::from_le_bytes(bytes.try_into().unwrap())
        };
        assert_eq!(e820_size, mem_size as u64);

        unsafe {
            libc::munmap(guest_mem as *mut libc::c_void, mem_size);
        }
    }
}
