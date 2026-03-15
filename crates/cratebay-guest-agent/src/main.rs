#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("cratebay-guest-agent is only supported on Linux guests");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(e) = run() {
        eprintln!("cratebay-guest-agent: {}", e);
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), String> {
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;

    let cfg = Config::from_env_and_args()?;

    let listener_fd = vsock_listen(cfg.port)?;
    eprintln!(
        "cratebay-guest-agent listening: vsock:{} -> {}",
        cfg.port,
        cfg.docker_socket.display()
    );

    loop {
        let conn_fd =
            unsafe { libc::accept(listener_fd, std::ptr::null_mut(), std::ptr::null_mut()) };
        if conn_fd < 0 {
            return Err(format!(
                "accept failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let docker_socket = cfg.docker_socket.clone();
        std::thread::spawn(move || {
            let vsock = unsafe { std::fs::File::from_raw_fd(conn_fd) };
            let docker = match UnixStream::connect(&docker_socket) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "cratebay-guest-agent: connect {} failed: {}",
                        docker_socket.display(),
                        e
                    );
                    return;
                }
            };

            if let Err(e) = proxy_bidirectional(docker, vsock) {
                eprintln!("cratebay-guest-agent: proxy ended: {}", e);
            }
        });
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Config {
    port: u32,
    docker_socket: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
impl Config {
    fn from_env_and_args() -> Result<Self, String> {
        let mut port: u32 = std::env::var("CRATEBAY_DOCKER_VSOCK_PORT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(6237);

        let mut docker_socket = std::env::var("CRATEBAY_GUEST_DOCKER_SOCK")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"));

        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--port" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| "--port requires a value".to_string())?;
                    port = raw
                        .parse::<u32>()
                        .map_err(|_| "Invalid --port".to_string())?;
                    if port == 0 {
                        return Err("--port must be > 0".to_string());
                    }
                }
                "--docker-sock" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| "--docker-sock requires a value".to_string())?;
                    docker_socket = PathBuf::from(raw);
                }
                "--help" | "-h" => {
                    return Err(Self::usage().to_string());
                }
                other => return Err(format!("Unknown argument: {}", other)),
            }
        }

        Ok(Self {
            port,
            docker_socket,
        })
    }

    fn usage() -> &'static str {
        "Usage:\n  cratebay-guest-agent [--port <vsock_port>] [--docker-sock <path>]\n\n\
Env:\n  CRATEBAY_DOCKER_VSOCK_PORT   Guest vsock listen port (default 6237)\n  \
CRATEBAY_GUEST_DOCKER_SOCK      Guest Docker unix socket path (default /var/run/docker.sock)\n"
    }
}

#[cfg(target_os = "linux")]
fn vsock_listen(port: u32) -> Result<i32, String> {
    // Some libc versions don't expose VMADDR_CID_ANY, keep it local.
    const VMADDR_CID_ANY: u32 = 0xFFFF_FFFF;

    #[repr(C)]
    struct SockAddrVm {
        svm_family: libc::sa_family_t,
        svm_reserved1: libc::c_ushort,
        svm_port: libc::c_uint,
        svm_cid: libc::c_uint,
        svm_zero: [libc::c_uchar; 4],
    }

    let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(format!(
            "socket(AF_VSOCK) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let addr = SockAddrVm {
        svm_family: libc::AF_VSOCK as libc::sa_family_t,
        svm_reserved1: 0,
        svm_port: port as libc::c_uint,
        svm_cid: VMADDR_CID_ANY as libc::c_uint,
        svm_zero: [0; 4],
    };

    let rc = unsafe {
        libc::bind(
            fd,
            &addr as *const SockAddrVm as *const libc::sockaddr,
            std::mem::size_of::<SockAddrVm>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(format!("bind(vsock:{}) failed: {}", port, err));
    }

    let rc = unsafe { libc::listen(fd, 128) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(format!("listen failed: {}", err));
    }

    Ok(fd)
}

#[cfg(target_os = "linux")]
fn proxy_bidirectional(
    docker: std::os::unix::net::UnixStream,
    vsock: std::fs::File,
) -> Result<(), String> {
    use std::net::Shutdown;
    use std::os::fd::AsRawFd;

    let mut docker_r = docker
        .try_clone()
        .map_err(|e| format!("docker clone: {}", e))?;
    let mut docker_w = docker;

    let mut vsock_r = vsock
        .try_clone()
        .map_err(|e| format!("vsock clone: {}", e))?;
    let mut vsock_w = vsock;

    let t1 = std::thread::spawn(move || {
        let _ = std::io::copy(&mut docker_r, &mut vsock_w);
        let _ = unsafe { libc::shutdown(vsock_w.as_raw_fd(), libc::SHUT_WR) };
    });

    let t2 = std::thread::spawn(move || {
        let _ = std::io::copy(&mut vsock_r, &mut docker_w);
        let _ = docker_w.shutdown(Shutdown::Write);
    });

    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}
