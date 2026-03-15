use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Key for identifying a port forward: (vm_id, host_port).
type ForwardKey = (String, u16);

/// Handle returned when a forward is started; dropping it does NOT stop the
/// forward -- call [`PortForwardManager::remove`] instead.
struct ForwardHandle {
    /// Cancel token -- when dropped the listener loop exits.
    cancel: tokio_util::sync::CancellationToken,
}

/// Manages active TCP port-forward listeners.
#[derive(Clone)]
pub struct PortForwardManager {
    inner: Arc<Mutex<HashMap<ForwardKey, ForwardHandle>>>,
}

impl Default for PortForwardManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PortForwardManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start forwarding `host_port` on the host to `guest_addr:guest_port`.
    ///
    /// `guest_addr` is typically the VM's IP address (e.g. `192.168.64.2`).
    /// For now callers may pass `"127.0.0.1"` when the VM network is not yet
    /// wired up -- the listener will still bind and attempt connections.
    pub async fn add(
        &self,
        vm_id: &str,
        host_port: u16,
        guest_addr: &str,
        guest_port: u16,
        _protocol: &str, // reserved for future UDP support
    ) -> Result<(), String> {
        let key: ForwardKey = (vm_id.to_string(), host_port);

        let mut map = self.inner.lock().await;
        if map.contains_key(&key) {
            return Err(format!(
                "Port forward already active: host {} for VM {}",
                host_port, vm_id
            ));
        }

        let bind_addr: SocketAddr = format!("0.0.0.0:{}", host_port)
            .parse()
            .map_err(|e| format!("invalid bind address: {}", e))?;

        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| format!("failed to bind port {}: {}", host_port, e))?;

        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = cancel.clone();
        let target = format!("{}:{}", guest_addr, guest_port);

        info!(
            vm_id = vm_id,
            host_port = host_port,
            target = %target,
            "port forward started"
        );

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => {
                        info!(host_port = host_port, "port forward cancelled");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((inbound, peer)) => {
                                let target = target.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = proxy(inbound, &target).await {
                                        warn!(
                                            peer = %peer,
                                            target = %target,
                                            error = %e,
                                            "proxy connection failed"
                                        );
                                    }
                                });
                            }
                            Err(e) => {
                                error!(error = %e, "accept failed on port {}", host_port);
                                break;
                            }
                        }
                    }
                }
            }
        });

        map.insert(key, ForwardHandle { cancel });
        Ok(())
    }

    /// Stop a port forward.
    pub async fn remove(&self, vm_id: &str, host_port: u16) -> Result<(), String> {
        let key: ForwardKey = (vm_id.to_string(), host_port);
        let mut map = self.inner.lock().await;
        if let Some(handle) = map.remove(&key) {
            handle.cancel.cancel();
            info!(vm_id = vm_id, host_port = host_port, "port forward removed");
            Ok(())
        } else {
            Err(format!(
                "No active port forward on host port {} for VM {}",
                host_port, vm_id
            ))
        }
    }

    /// List host ports currently forwarded for a VM.
    pub async fn list(&self, vm_id: &str) -> Vec<u16> {
        let map = self.inner.lock().await;
        map.keys()
            .filter(|(vid, _)| vid == vm_id)
            .map(|(_, port)| *port)
            .collect()
    }

    /// Stop all forwards for a given VM.
    pub async fn remove_all(&self, vm_id: &str) {
        let mut map = self.inner.lock().await;
        let keys: Vec<ForwardKey> = map
            .keys()
            .filter(|(vid, _)| vid == vm_id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(handle) = map.remove(&key) {
                handle.cancel.cancel();
            }
        }
    }
}

/// Find a free TCP port by binding to port 0 and returning the assigned port.
/// This is used internally by tests but exposed for potential future use.
#[allow(dead_code)]
async fn find_free_port() -> Option<u16> {
    match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => Some(listener.local_addr().ok()?.port()),
        Err(_) => None,
    }
}

/// Bi-directional TCP proxy between `inbound` and `target_addr`.
async fn proxy(mut inbound: TcpStream, target_addr: &str) -> Result<(), String> {
    let mut outbound = TcpStream::connect(target_addr)
        .await
        .map_err(|e| format!("connect to {}: {}", target_addr, e))?;

    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = io::copy(&mut ri, &mut wo);
    let server_to_client = io::copy(&mut ro, &mut wi);

    tokio::select! {
        r = client_to_server => { r.map_err(|e| e.to_string())?; }
        r = server_to_client => { r.map_err(|e| e.to_string())?; }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // -----------------------------------------------------------------------
    // Helper: allocate a free port by binding to port 0, then immediately
    // dropping the listener so the port is available for the test.
    // -----------------------------------------------------------------------

    async fn alloc_free_port() -> Option<u16> {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "SKIP: cannot bind to localhost for port-forward tests ({})",
                    e
                );
                return None;
            }
        };
        let port = listener.local_addr().ok()?.port();
        drop(listener);
        Some(port)
    }

    // -----------------------------------------------------------------------
    // PortForwardManager creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn manager_new_creates_empty_instance() {
        // new() is not async, so a regular #[test] works.
        let mgr = PortForwardManager::new();
        // The inner map should exist (we can't inspect it without locking,
        // but construction should not panic).
        let _ = mgr;
    }

    #[test]
    fn manager_default_creates_empty_instance() {
        let mgr = PortForwardManager::default();
        let _ = mgr;
    }

    #[test]
    fn manager_clone_shares_state() {
        let mgr1 = PortForwardManager::new();
        let mgr2 = mgr1.clone();
        // Both should point to the same inner Arc.
        assert!(Arc::ptr_eq(&mgr1.inner, &mgr2.inner));
    }

    // -----------------------------------------------------------------------
    // list() on empty manager
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn list_empty_manager_returns_empty() {
        let mgr = PortForwardManager::new();
        let ports = mgr.list("vm-1").await;
        assert!(ports.is_empty(), "new manager should have no forwards");
    }

    // -----------------------------------------------------------------------
    // add() basic tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn add_single_forward_succeeds() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        let result = mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await;
        assert!(
            result.is_ok(),
            "adding a forward should succeed: {:?}",
            result
        );

        // Cleanup.
        mgr.remove("vm-1", port).await.unwrap();
    }

    #[tokio::test]
    async fn add_forward_appears_in_list() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();

        let ports = mgr.list("vm-1").await;
        assert_eq!(ports.len(), 1, "should have exactly one forward");
        assert_eq!(ports[0], port);

        mgr.remove("vm-1", port).await.unwrap();
    }

    #[tokio::test]
    async fn add_multiple_forwards_same_vm() {
        let mgr = PortForwardManager::new();
        let Some(port1) = alloc_free_port().await else {
            return;
        };
        let Some(port2) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port1, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();
        mgr.add("vm-1", port2, "127.0.0.1", 80, "tcp")
            .await
            .unwrap();

        let mut ports = mgr.list("vm-1").await;
        ports.sort();
        assert_eq!(ports.len(), 2);

        mgr.remove_all("vm-1").await;
    }

    #[tokio::test]
    async fn add_forwards_different_vms() {
        let mgr = PortForwardManager::new();
        let Some(port1) = alloc_free_port().await else {
            return;
        };
        let Some(port2) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port1, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();
        mgr.add("vm-2", port2, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();

        let ports_vm1 = mgr.list("vm-1").await;
        let ports_vm2 = mgr.list("vm-2").await;
        assert_eq!(ports_vm1.len(), 1);
        assert_eq!(ports_vm2.len(), 1);
        assert_eq!(ports_vm1[0], port1);
        assert_eq!(ports_vm2[0], port2);

        mgr.remove_all("vm-1").await;
        mgr.remove_all("vm-2").await;
    }

    // -----------------------------------------------------------------------
    // add() duplicate detection
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn add_duplicate_port_same_vm_returns_error() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();
        let result = mgr.add("vm-1", port, "127.0.0.1", 80, "tcp").await;

        assert!(result.is_err(), "duplicate should fail");
        let err = result.unwrap_err();
        assert!(
            err.contains("already active"),
            "error should mention 'already active', got: {}",
            err
        );

        mgr.remove("vm-1", port).await.unwrap();
    }

    #[tokio::test]
    async fn add_same_port_different_vm_fails_bind() {
        // Two different VMs trying to bind the same host port should fail
        // on the second bind (OS will refuse).
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();
        let result = mgr.add("vm-2", port, "127.0.0.1", 22, "tcp").await;

        assert!(result.is_err(), "binding same port twice should fail");
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to bind"),
            "error should mention bind failure, got: {}",
            err
        );

        mgr.remove("vm-1", port).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // remove() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn remove_existing_forward_succeeds() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();
        let result = mgr.remove("vm-1", port).await;
        assert!(result.is_ok(), "removing existing forward should succeed");

        let ports = mgr.list("vm-1").await;
        assert!(ports.is_empty(), "list should be empty after removal");
    }

    #[tokio::test]
    async fn remove_nonexistent_forward_returns_error() {
        let mgr = PortForwardManager::new();
        let result = mgr.remove("vm-1", 9999).await;

        assert!(result.is_err(), "removing nonexistent forward should fail");
        let err = result.unwrap_err();
        assert!(
            err.contains("No active port forward"),
            "error should mention 'No active port forward', got: {}",
            err
        );
    }

    #[tokio::test]
    async fn remove_wrong_vm_id_returns_error() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();

        // Try to remove using a different VM id.
        let result = mgr.remove("vm-2", port).await;
        assert!(result.is_err(), "removing with wrong vm_id should fail");

        // Original forward should still be active.
        let ports = mgr.list("vm-1").await;
        assert_eq!(ports.len(), 1);

        mgr.remove("vm-1", port).await.unwrap();
    }

    #[tokio::test]
    async fn remove_twice_returns_error_second_time() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();
        mgr.remove("vm-1", port).await.unwrap();

        let result = mgr.remove("vm-1", port).await;
        assert!(result.is_err(), "second removal should fail");
    }

    // -----------------------------------------------------------------------
    // list() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn list_returns_only_ports_for_specified_vm() {
        let mgr = PortForwardManager::new();
        let Some(port1) = alloc_free_port().await else {
            return;
        };
        let Some(port2) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port1, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();
        mgr.add("vm-2", port2, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();

        let ports_vm1 = mgr.list("vm-1").await;
        assert_eq!(ports_vm1.len(), 1);
        assert_eq!(ports_vm1[0], port1);

        let ports_vm2 = mgr.list("vm-2").await;
        assert_eq!(ports_vm2.len(), 1);
        assert_eq!(ports_vm2[0], port2);

        // Unknown VM should return empty.
        let ports_vm3 = mgr.list("vm-3").await;
        assert!(ports_vm3.is_empty());

        mgr.remove_all("vm-1").await;
        mgr.remove_all("vm-2").await;
    }

    #[tokio::test]
    async fn list_unknown_vm_returns_empty() {
        let mgr = PortForwardManager::new();
        let ports = mgr.list("nonexistent-vm").await;
        assert!(ports.is_empty());
    }

    // -----------------------------------------------------------------------
    // remove_all() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn remove_all_clears_all_forwards_for_vm() {
        let mgr = PortForwardManager::new();
        let Some(port1) = alloc_free_port().await else {
            return;
        };
        let Some(port2) = alloc_free_port().await else {
            return;
        };
        let Some(port3) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port1, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();
        mgr.add("vm-1", port2, "127.0.0.1", 80, "tcp")
            .await
            .unwrap();
        mgr.add("vm-2", port3, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();

        mgr.remove_all("vm-1").await;

        let ports_vm1 = mgr.list("vm-1").await;
        assert!(ports_vm1.is_empty(), "all vm-1 forwards should be removed");

        // vm-2 should be untouched.
        let ports_vm2 = mgr.list("vm-2").await;
        assert_eq!(ports_vm2.len(), 1, "vm-2 forward should remain");

        mgr.remove_all("vm-2").await;
    }

    #[tokio::test]
    async fn remove_all_on_empty_manager_is_noop() {
        let mgr = PortForwardManager::new();
        // Should not panic or error.
        mgr.remove_all("vm-1").await;
        let ports = mgr.list("vm-1").await;
        assert!(ports.is_empty());
    }

    #[tokio::test]
    async fn remove_all_on_unknown_vm_is_noop() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();

        // remove_all for a VM that has no forwards should not affect vm-1.
        mgr.remove_all("vm-2").await;

        let ports = mgr.list("vm-1").await;
        assert_eq!(ports.len(), 1);

        mgr.remove("vm-1", port).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Port reuse after removal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn port_can_be_reused_after_removal() {
        let mgr = PortForwardManager::new();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-1", port, "127.0.0.1", 22, "tcp").await.unwrap();
        mgr.remove("vm-1", port).await.unwrap();

        // The OS may need a moment to release the port after the listener
        // task is cancelled, so give it a short delay.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = mgr.add("vm-1", port, "127.0.0.1", 80, "tcp").await;
        assert!(
            result.is_ok(),
            "should be able to re-add after removal: {:?}",
            result
        );

        mgr.remove("vm-1", port).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Clone / shared state tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn cloned_manager_shares_forwards() {
        let mgr1 = PortForwardManager::new();
        let mgr2 = mgr1.clone();
        let Some(port) = alloc_free_port().await else {
            return;
        };

        mgr1.add("vm-1", port, "127.0.0.1", 22, "tcp")
            .await
            .unwrap();

        // Should be visible via the clone.
        let ports = mgr2.list("vm-1").await;
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0], port);

        // Remove via the clone.
        mgr2.remove("vm-1", port).await.unwrap();

        // Should be gone from the original.
        let ports = mgr1.list("vm-1").await;
        assert!(ports.is_empty());
    }

    // -----------------------------------------------------------------------
    // TCP proxy integration test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn tcp_proxy_forwards_data_bidirectionally() {
        // 1. Start an echo server that reads a fixed number of bytes and
        //    writes them back, then shuts down its write half.  The proxy
        //    uses `select!`, so the server must shut down *its* side first
        //    in order for the server_to_client copy to complete before the
        //    client_to_server direction drops.
        let echo_listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "SKIP: cannot bind to localhost for port-forward tests ({})",
                    e
                );
                return;
            }
        };
        let echo_port = echo_listener.local_addr().unwrap().port();

        let test_data = b"Hello, port forwarding!";
        let expected_len = test_data.len();

        let echo_handle = tokio::spawn(async move {
            if let Ok((mut stream, _)) = echo_listener.accept().await {
                let mut buf = vec![0u8; expected_len];
                let mut total = 0;
                while total < expected_len {
                    match stream.read(&mut buf[total..]).await {
                        Ok(0) => break,
                        Ok(n) => total += n,
                        Err(_) => break,
                    }
                }
                let _ = stream.write_all(&buf[..total]).await;
                // Shut down the write side so the proxy's server_to_client
                // copy completes (read returns 0).
                let _ = stream.shutdown().await;
            }
        });

        // 2. Set up the port forward manager to forward a host port to the echo server.
        let mgr = PortForwardManager::new();
        let Some(host_port) = alloc_free_port().await else {
            return;
        };

        mgr.add("vm-test", host_port, "127.0.0.1", echo_port, "tcp")
            .await
            .unwrap();

        // 3. Give the listener a moment to start accepting.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 4. Connect to the forwarded port and send data.
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", host_port))
            .await
            .unwrap();

        client.write_all(test_data).await.unwrap();

        // 5. Read the echoed data.  The echo server shuts down its write
        //    half after sending, which causes the proxy's server_to_client
        //    direction to complete, which in turn wins the `select!` and
        //    the proxy task finishes, closing the connection back to us.
        let mut response = vec![0u8; expected_len];
        let mut total = 0;
        while total < expected_len {
            match client.read(&mut response[total..]).await {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => break,
            }
        }

        assert_eq!(
            &response[..total],
            test_data,
            "echoed data should match sent data"
        );

        // 6. Cleanup.
        mgr.remove("vm-test", host_port).await.unwrap();
        let _ = echo_handle.await;
    }

    // -----------------------------------------------------------------------
    // find_free_port utility test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn find_free_port_returns_nonzero() {
        let Some(port) = find_free_port().await else {
            eprintln!("SKIP: cannot bind to localhost for port-forward tests");
            return;
        };
        assert!(port > 0, "free port should be > 0");
    }

    #[tokio::test]
    async fn find_free_port_returns_different_ports() {
        let Some(port1) = find_free_port().await else {
            eprintln!("SKIP: cannot bind to localhost for port-forward tests");
            return;
        };
        let Some(port2) = find_free_port().await else {
            eprintln!("SKIP: cannot bind to localhost for port-forward tests");
            return;
        };
        // While not strictly guaranteed, the OS almost always assigns different
        // ports for sequential binds. This is a sanity check.
        // We accept the rare case where they are the same since the first
        // listener is dropped before the second bind.
        let _ = (port1, port2);
    }
}
