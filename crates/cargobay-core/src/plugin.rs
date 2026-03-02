//! Plugin system for CargoBay.
//!
//! Provides a trait-based plugin architecture with lifecycle hooks for VM and
//! container events.  Plugins are registered with a [`PluginManager`] which
//! dispatches events to every loaded plugin in registration order.
//!
//! # Built-in plugins
//!
//! * [`LoggingPlugin`] -- logs every lifecycle event via the `tracing` crate.
//!
//! # Example
//!
//! ```rust
//! use cargobay_core::plugin::{LoggingPlugin, PluginManager};
//!
//! let mut manager = PluginManager::new();
//! manager.register(Box::new(LoggingPlugin));
//! assert_eq!(manager.plugins().len(), 1);
//! ```

use crate::hypervisor::VmConfig;

// ---------------------------------------------------------------------------
// Plugin trait
// ---------------------------------------------------------------------------

/// A CargoBay plugin that can react to VM and container lifecycle events.
///
/// All hook methods have default no-op implementations so that plugins only
/// need to override the events they care about.
pub trait Plugin: Send + Sync {
    /// Human-readable name of the plugin (e.g. "logging").
    fn name(&self) -> &str;

    /// SemVer version string of the plugin.
    fn version(&self) -> &str;

    // -- VM lifecycle hooks -------------------------------------------------

    /// Called just before a VM is created.
    fn on_vm_create(&self, _config: &VmConfig) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called after a VM has been started.
    fn on_vm_start(&self, _id: &str) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called after a VM has been stopped.
    fn on_vm_stop(&self, _id: &str) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called after a VM has been deleted.
    fn on_vm_delete(&self, _id: &str) -> Result<(), PluginError> {
        Ok(())
    }

    // -- Container lifecycle hooks ------------------------------------------

    /// Called after a container has been started.
    fn on_container_start(&self, _id: &str) -> Result<(), PluginError> {
        Ok(())
    }

    /// Called after a container has been stopped.
    fn on_container_stop(&self, _id: &str) -> Result<(), PluginError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PluginError
// ---------------------------------------------------------------------------

/// Error type returned by plugin hooks.
#[derive(Debug, Clone)]
pub struct PluginError {
    /// Name of the plugin that produced the error.
    pub plugin: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plugin '{}': {}", self.plugin, self.message)
    }
}

impl std::error::Error for PluginError {}

// ---------------------------------------------------------------------------
// PluginManager
// ---------------------------------------------------------------------------

/// Manages a collection of [`Plugin`] instances and dispatches lifecycle
/// events to each one in registration order.
///
/// If a plugin hook returns an error the manager logs it and continues
/// dispatching to remaining plugins.  The collected errors are returned to the
/// caller so it can decide how to handle them (e.g. abort the operation or
/// carry on).
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    /// Create an empty plugin manager with no plugins registered.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin.  Plugins are called in registration order.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        tracing::info!(
            plugin_name = plugin.name(),
            plugin_version = plugin.version(),
            "plugin registered"
        );
        self.plugins.push(plugin);
    }

    /// Return a read-only slice of the currently registered plugins.
    pub fn plugins(&self) -> &[Box<dyn Plugin>] {
        &self.plugins
    }

    /// Return how many plugins are registered.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    // -- Event dispatchers --------------------------------------------------

    /// Fire the `on_vm_create` hook on every registered plugin.
    pub fn fire_vm_create(&self, config: &VmConfig) -> Vec<PluginError> {
        self.dispatch("on_vm_create", |p| p.on_vm_create(config))
    }

    /// Fire the `on_vm_start` hook on every registered plugin.
    pub fn fire_vm_start(&self, id: &str) -> Vec<PluginError> {
        self.dispatch("on_vm_start", |p| p.on_vm_start(id))
    }

    /// Fire the `on_vm_stop` hook on every registered plugin.
    pub fn fire_vm_stop(&self, id: &str) -> Vec<PluginError> {
        self.dispatch("on_vm_stop", |p| p.on_vm_stop(id))
    }

    /// Fire the `on_vm_delete` hook on every registered plugin.
    pub fn fire_vm_delete(&self, id: &str) -> Vec<PluginError> {
        self.dispatch("on_vm_delete", |p| p.on_vm_delete(id))
    }

    /// Fire the `on_container_start` hook on every registered plugin.
    pub fn fire_container_start(&self, id: &str) -> Vec<PluginError> {
        self.dispatch("on_container_start", |p| p.on_container_start(id))
    }

    /// Fire the `on_container_stop` hook on every registered plugin.
    pub fn fire_container_stop(&self, id: &str) -> Vec<PluginError> {
        self.dispatch("on_container_stop", |p| p.on_container_stop(id))
    }

    // -- Internal -----------------------------------------------------------

    /// Call `hook_fn` on every registered plugin, collecting any errors.
    fn dispatch<F>(&self, hook_name: &str, hook_fn: F) -> Vec<PluginError>
    where
        F: Fn(&dyn Plugin) -> Result<(), PluginError>,
    {
        let mut errors = Vec::new();
        for plugin in &self.plugins {
            if let Err(e) = hook_fn(plugin.as_ref()) {
                tracing::warn!(
                    plugin = plugin.name(),
                    hook = hook_name,
                    error = %e,
                    "plugin hook failed"
                );
                errors.push(e);
            }
        }
        errors
    }
}

// ---------------------------------------------------------------------------
// Built-in: LoggingPlugin
// ---------------------------------------------------------------------------

/// A simple built-in plugin that logs every lifecycle event via `tracing`.
///
/// This is useful for debugging and serves as a reference implementation for
/// the [`Plugin`] trait.
pub struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn name(&self) -> &str {
        "logging"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn on_vm_create(&self, config: &VmConfig) -> Result<(), PluginError> {
        tracing::info!(
            vm_name = config.name,
            cpus = config.cpus,
            memory_mb = config.memory_mb,
            disk_gb = config.disk_gb,
            "[logging-plugin] vm_create"
        );
        Ok(())
    }

    fn on_vm_start(&self, id: &str) -> Result<(), PluginError> {
        tracing::info!(vm_id = id, "[logging-plugin] vm_start");
        Ok(())
    }

    fn on_vm_stop(&self, id: &str) -> Result<(), PluginError> {
        tracing::info!(vm_id = id, "[logging-plugin] vm_stop");
        Ok(())
    }

    fn on_vm_delete(&self, id: &str) -> Result<(), PluginError> {
        tracing::info!(vm_id = id, "[logging-plugin] vm_delete");
        Ok(())
    }

    fn on_container_start(&self, id: &str) -> Result<(), PluginError> {
        tracing::info!(container_id = id, "[logging-plugin] container_start");
        Ok(())
    }

    fn on_container_stop(&self, id: &str) -> Result<(), PluginError> {
        tracing::info!(container_id = id, "[logging-plugin] container_stop");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hypervisor::VmConfig;

    // -- A test-only plugin that always succeeds -----------------------------

    struct NoOpPlugin;

    impl Plugin for NoOpPlugin {
        fn name(&self) -> &str {
            "noop"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }
    }

    // -- A test-only plugin that always fails --------------------------------

    struct FailingPlugin {
        label: String,
    }

    impl FailingPlugin {
        fn new(label: &str) -> Self {
            Self {
                label: label.to_string(),
            }
        }
    }

    impl Plugin for FailingPlugin {
        fn name(&self) -> &str {
            &self.label
        }

        fn version(&self) -> &str {
            "0.0.1"
        }

        fn on_vm_create(&self, _config: &VmConfig) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "create rejected".into(),
            })
        }

        fn on_vm_start(&self, _id: &str) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "start rejected".into(),
            })
        }

        fn on_vm_stop(&self, _id: &str) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "stop rejected".into(),
            })
        }

        fn on_vm_delete(&self, _id: &str) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "delete rejected".into(),
            })
        }

        fn on_container_start(&self, _id: &str) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "container start rejected".into(),
            })
        }

        fn on_container_stop(&self, _id: &str) -> Result<(), PluginError> {
            Err(PluginError {
                plugin: self.label.clone(),
                message: "container stop rejected".into(),
            })
        }
    }

    // -- A test-only plugin that tracks calls via shared state ---------------

    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CallLog {
        calls: Vec<String>,
    }

    struct TrackingPlugin {
        log: Arc<Mutex<CallLog>>,
    }

    impl TrackingPlugin {
        fn new(log: Arc<Mutex<CallLog>>) -> Self {
            Self { log }
        }
    }

    impl Plugin for TrackingPlugin {
        fn name(&self) -> &str {
            "tracking"
        }

        fn version(&self) -> &str {
            "0.1.0"
        }

        fn on_vm_create(&self, config: &VmConfig) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("vm_create:{}", config.name));
            Ok(())
        }

        fn on_vm_start(&self, id: &str) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("vm_start:{}", id));
            Ok(())
        }

        fn on_vm_stop(&self, id: &str) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("vm_stop:{}", id));
            Ok(())
        }

        fn on_vm_delete(&self, id: &str) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("vm_delete:{}", id));
            Ok(())
        }

        fn on_container_start(&self, id: &str) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("container_start:{}", id));
            Ok(())
        }

        fn on_container_stop(&self, id: &str) -> Result<(), PluginError> {
            self.log
                .lock()
                .unwrap()
                .calls
                .push(format!("container_stop:{}", id));
            Ok(())
        }
    }

    // =======================================================================
    // PluginManager tests
    // =======================================================================

    #[test]
    fn new_manager_has_no_plugins() {
        let mgr = PluginManager::new();
        assert_eq!(mgr.plugin_count(), 0);
        assert!(mgr.plugins().is_empty());
    }

    #[test]
    fn default_manager_has_no_plugins() {
        let mgr = PluginManager::default();
        assert_eq!(mgr.plugin_count(), 0);
    }

    #[test]
    fn register_adds_plugin() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        assert_eq!(mgr.plugin_count(), 1);
        assert_eq!(mgr.plugins()[0].name(), "noop");
        assert_eq!(mgr.plugins()[0].version(), "1.0.0");
    }

    #[test]
    fn register_multiple_plugins_preserves_order() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        mgr.register(Box::new(LoggingPlugin));
        assert_eq!(mgr.plugin_count(), 2);
        assert_eq!(mgr.plugins()[0].name(), "noop");
        assert_eq!(mgr.plugins()[1].name(), "logging");
    }

    // -- fire_vm_create -----------------------------------------------------

    #[test]
    fn fire_vm_create_no_plugins_returns_empty() {
        let mgr = PluginManager::new();
        let errors = mgr.fire_vm_create(&VmConfig::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_vm_create_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_vm_create(&VmConfig::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_vm_create_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("bad-1")));
        mgr.register(Box::new(FailingPlugin::new("bad-2")));
        let errors = mgr.fire_vm_create(&VmConfig::default());
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].plugin, "bad-1");
        assert_eq!(errors[1].plugin, "bad-2");
    }

    #[test]
    fn fire_vm_create_mixed_success_and_failure() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        mgr.register(Box::new(FailingPlugin::new("bad")));
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_vm_create(&VmConfig::default());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].plugin, "bad");
    }

    // -- fire_vm_start ------------------------------------------------------

    #[test]
    fn fire_vm_start_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_vm_start("vm-1");
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_vm_start_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("fail")));
        let errors = mgr.fire_vm_start("vm-1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("start rejected"));
    }

    // -- fire_vm_stop -------------------------------------------------------

    #[test]
    fn fire_vm_stop_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_vm_stop("vm-1");
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_vm_stop_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("fail")));
        let errors = mgr.fire_vm_stop("vm-1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("stop rejected"));
    }

    // -- fire_vm_delete -----------------------------------------------------

    #[test]
    fn fire_vm_delete_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_vm_delete("vm-1");
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_vm_delete_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("fail")));
        let errors = mgr.fire_vm_delete("vm-1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("delete rejected"));
    }

    // -- fire_container_start -----------------------------------------------

    #[test]
    fn fire_container_start_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_container_start("ctr-1");
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_container_start_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("fail")));
        let errors = mgr.fire_container_start("ctr-1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("container start rejected"));
    }

    // -- fire_container_stop ------------------------------------------------

    #[test]
    fn fire_container_stop_success() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(NoOpPlugin));
        let errors = mgr.fire_container_stop("ctr-1");
        assert!(errors.is_empty());
    }

    #[test]
    fn fire_container_stop_collects_errors() {
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(FailingPlugin::new("fail")));
        let errors = mgr.fire_container_stop("ctr-1");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("container stop rejected"));
    }

    // -- Tracking plugin (verifies hooks actually receive correct args) ------

    #[test]
    fn tracking_plugin_receives_correct_vm_create_args() {
        let log = Arc::new(Mutex::new(CallLog::default()));
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));

        let config = VmConfig {
            name: "test-vm".into(),
            ..VmConfig::default()
        };
        let errors = mgr.fire_vm_create(&config);
        assert!(errors.is_empty());

        let calls = &log.lock().unwrap().calls;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "vm_create:test-vm");
    }

    #[test]
    fn tracking_plugin_receives_correct_vm_lifecycle_args() {
        let log = Arc::new(Mutex::new(CallLog::default()));
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));

        mgr.fire_vm_start("vm-42");
        mgr.fire_vm_stop("vm-42");
        mgr.fire_vm_delete("vm-42");

        let calls = &log.lock().unwrap().calls;
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0], "vm_start:vm-42");
        assert_eq!(calls[1], "vm_stop:vm-42");
        assert_eq!(calls[2], "vm_delete:vm-42");
    }

    #[test]
    fn tracking_plugin_receives_correct_container_args() {
        let log = Arc::new(Mutex::new(CallLog::default()));
        let mut mgr = PluginManager::new();
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));

        mgr.fire_container_start("ctr-7");
        mgr.fire_container_stop("ctr-7");

        let calls = &log.lock().unwrap().calls;
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], "container_start:ctr-7");
        assert_eq!(calls[1], "container_stop:ctr-7");
    }

    #[test]
    fn multiple_plugins_all_called_in_order() {
        let log = Arc::new(Mutex::new(CallLog::default()));
        let mut mgr = PluginManager::new();

        // Register two tracking plugins sharing the same log.
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));

        mgr.fire_vm_start("vm-1");

        let calls = &log.lock().unwrap().calls;
        // Both plugins should have been called (2 entries for the same event).
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], "vm_start:vm-1");
        assert_eq!(calls[1], "vm_start:vm-1");
    }

    // -- PluginError --------------------------------------------------------

    #[test]
    fn plugin_error_display() {
        let err = PluginError {
            plugin: "test".into(),
            message: "something broke".into(),
        };
        assert_eq!(err.to_string(), "plugin 'test': something broke");
    }

    #[test]
    fn plugin_error_debug_includes_fields() {
        let err = PluginError {
            plugin: "my-plugin".into(),
            message: "oops".into(),
        };
        let debug = format!("{:?}", err);
        assert!(debug.contains("my-plugin"));
        assert!(debug.contains("oops"));
    }

    #[test]
    fn plugin_error_clone() {
        let err = PluginError {
            plugin: "p".into(),
            message: "m".into(),
        };
        let cloned = err.clone();
        assert_eq!(err.plugin, cloned.plugin);
        assert_eq!(err.message, cloned.message);
    }

    // -- LoggingPlugin trait impl -------------------------------------------

    #[test]
    fn logging_plugin_name_and_version() {
        let p = LoggingPlugin;
        assert_eq!(p.name(), "logging");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn logging_plugin_hooks_succeed() {
        let p = LoggingPlugin;
        assert!(p.on_vm_create(&VmConfig::default()).is_ok());
        assert!(p.on_vm_start("vm-1").is_ok());
        assert!(p.on_vm_stop("vm-1").is_ok());
        assert!(p.on_vm_delete("vm-1").is_ok());
        assert!(p.on_container_start("ctr-1").is_ok());
        assert!(p.on_container_stop("ctr-1").is_ok());
    }

    // -- Default trait implementations (NoOpPlugin uses them) ----------------

    #[test]
    fn default_hooks_return_ok() {
        let p = NoOpPlugin;
        assert!(p.on_vm_create(&VmConfig::default()).is_ok());
        assert!(p.on_vm_start("x").is_ok());
        assert!(p.on_vm_stop("x").is_ok());
        assert!(p.on_vm_delete("x").is_ok());
        assert!(p.on_container_start("x").is_ok());
        assert!(p.on_container_stop("x").is_ok());
    }

    // -- Error continues dispatching ----------------------------------------

    #[test]
    fn error_in_first_plugin_does_not_block_second() {
        let log = Arc::new(Mutex::new(CallLog::default()));
        let mut mgr = PluginManager::new();

        mgr.register(Box::new(FailingPlugin::new("fail")));
        mgr.register(Box::new(TrackingPlugin::new(log.clone())));

        let errors = mgr.fire_vm_start("vm-1");
        assert_eq!(errors.len(), 1, "only the failing plugin should error");

        let calls = &log.lock().unwrap().calls;
        assert_eq!(
            calls.len(),
            1,
            "tracking plugin should still have been called"
        );
        assert_eq!(calls[0], "vm_start:vm-1");
    }
}
