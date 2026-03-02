//! cargobay-vz — VM runner process using Apple Virtualization.framework.
//!
//! This binary is spawned by `MacOSHypervisor` (in cargobay-core) to run a
//! single Linux VM via the Virtualization.framework Swift bridge.
//!
//! On non-macOS platforms, it prints an error and exits.

#[cfg(target_os = "macos")]
mod ffi;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("cargobay-vz is only supported on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    cargobay_core::logging::init();

    let args = match Args::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            eprintln!();
            eprintln!("{}", Args::usage());
            std::process::exit(2);
        }
    };

    if let Err(e) = run(args) {
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct Args {
    kernel: std::path::PathBuf,
    initrd: Option<std::path::PathBuf>,
    disk: std::path::PathBuf,
    cpus: u32,
    memory_mb: u64,
    cmdline: String,
    ready_file: Option<std::path::PathBuf>,
    rosetta: bool,
    /// Shared directories in "tag:host_path[:ro]" format.
    shared_dirs: Vec<String>,
}

#[cfg(target_os = "macos")]
impl Args {
    fn usage() -> &'static str {
        "Usage:\n  cargobay-vz --kernel <path> --disk <path> --cpus <n> --memory-mb <n> \
         [--initrd <path>] [--cmdline <str>] [--ready-file <path>] \
         [--rosetta] [--share tag:host_path[:ro]]\n"
    }

    fn parse() -> Result<Self, String> {
        let mut kernel: Option<std::path::PathBuf> = None;
        let mut initrd: Option<std::path::PathBuf> = None;
        let mut disk: Option<std::path::PathBuf> = None;
        let mut cpus: Option<u32> = None;
        let mut memory_mb: Option<u64> = None;
        let mut cmdline: Option<String> = None;
        let mut ready_file: Option<std::path::PathBuf> = None;
        let mut rosetta = false;
        let mut shared_dirs: Vec<String> = Vec::new();

        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    return Err(Self::usage().to_string());
                }
                "--kernel" => {
                    kernel = Some(
                        it.next()
                            .ok_or_else(|| "--kernel requires a value".to_string())?
                            .into(),
                    );
                }
                "--initrd" => {
                    initrd = Some(
                        it.next()
                            .ok_or_else(|| "--initrd requires a value".to_string())?
                            .into(),
                    );
                }
                "--disk" => {
                    disk = Some(
                        it.next()
                            .ok_or_else(|| "--disk requires a value".to_string())?
                            .into(),
                    );
                }
                "--cpus" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| "--cpus requires a value".to_string())?;
                    cpus = Some(
                        raw.parse::<u32>()
                            .map_err(|_| "Invalid --cpus".to_string())?,
                    );
                }
                "--memory-mb" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| "--memory-mb requires a value".to_string())?;
                    memory_mb = Some(
                        raw.parse::<u64>()
                            .map_err(|_| "Invalid --memory-mb".to_string())?,
                    );
                }
                "--cmdline" => {
                    cmdline = Some(
                        it.next()
                            .ok_or_else(|| "--cmdline requires a value".to_string())?,
                    );
                }
                "--ready-file" => {
                    ready_file = Some(
                        it.next()
                            .ok_or_else(|| "--ready-file requires a value".to_string())?
                            .into(),
                    );
                }
                "--rosetta" => {
                    rosetta = true;
                }
                "--share" => {
                    shared_dirs.push(
                        it.next()
                            .ok_or_else(|| "--share requires a value".to_string())?,
                    );
                }
                other => return Err(format!("Unknown argument: {}", other)),
            }
        }

        let kernel = kernel.ok_or_else(|| "Missing --kernel".to_string())?;
        let disk = disk.ok_or_else(|| "Missing --disk".to_string())?;
        let cpus = cpus.ok_or_else(|| "Missing --cpus".to_string())?;
        let memory_mb = memory_mb.ok_or_else(|| "Missing --memory-mb".to_string())?;
        let cmdline = cmdline.unwrap_or_else(|| "console=hvc0".to_string());

        Ok(Self {
            kernel,
            initrd,
            disk,
            cpus,
            memory_mb,
            cmdline,
            ready_file,
            rosetta,
            shared_dirs,
        })
    }
}

#[cfg(target_os = "macos")]
fn parse_shared_dir(spec: &str) -> Result<ffi::SharedDirFFI, String> {
    // Format: "tag:host_path" or "tag:host_path:ro"
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Err(format!(
            "Invalid --share format '{}', expected 'tag:host_path[:ro]'",
            spec
        ));
    }
    let tag = parts[0];
    let host_path = parts[1];
    let read_only = parts.get(2).is_some_and(|s| *s == "ro");

    let tag = std::ffi::CString::new(tag).map_err(|e| format!("invalid tag: {}", e))?;
    let host_path =
        std::ffi::CString::new(host_path).map_err(|e| format!("invalid host_path: {}", e))?;

    Ok(ffi::SharedDirFFI {
        tag,
        host_path,
        read_only,
    })
}

#[cfg(target_os = "macos")]
fn run(args: Args) -> Result<(), String> {
    let kernel_path = args
        .kernel
        .to_str()
        .ok_or_else(|| "Kernel path is not valid UTF-8".to_string())?
        .to_string();
    let disk_path = args
        .disk
        .to_str()
        .ok_or_else(|| "Disk path is not valid UTF-8".to_string())?
        .to_string();
    let initrd_path = args
        .initrd
        .as_ref()
        .map(|p| {
            p.to_str()
                .ok_or_else(|| "Initrd path is not valid UTF-8".to_string())
                .map(|s| s.to_string())
        })
        .transpose()?;

    // Parse shared directory specs.
    let shared_dirs: Vec<ffi::SharedDirFFI> = args
        .shared_dirs
        .iter()
        .map(|s| parse_shared_dir(s))
        .collect::<Result<Vec<_>, _>>()?;

    let config = ffi::VmCreateConfig {
        kernel_path,
        initrd_path,
        cmdline: args.cmdline.clone(),
        disk_path,
        console_log_path: None,
        cpus: args.cpus,
        memory_mb: args.memory_mb,
        rosetta: args.rosetta,
        shared_dirs,
    };

    let handle = ffi::create_and_start_vm(&config)?;

    // Signal readiness.
    if let Some(path) = args.ready_file.as_ref() {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        std::fs::write(path, b"ready\n")
            .map_err(|e| format!("Failed to write ready file: {}", e))?;
    }

    tracing::info!(
        "VZ VM started (pid {}, state {:?})",
        std::process::id(),
        handle.state()
    );

    // Park the main thread; the VM runs on its dispatch queue.
    // The process will be killed by MacOSHypervisor::stop_vm().
    loop {
        std::thread::park();
    }
}
