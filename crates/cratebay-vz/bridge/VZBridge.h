// VZBridge.h — C-compatible interface to Apple Virtualization.framework.
//
// This header defines the C ABI that Rust FFI calls into. The implementations
// live in VZBridge.swift and are compiled via build.rs into a static library.

#ifndef VZ_BRIDGE_H
#define VZ_BRIDGE_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/// Opaque error string returned by bridge functions.
/// Caller must free with vz_free_string().
typedef char *VZErrorString;

/// Free a string allocated by the bridge.
void vz_free_string(char *s);

// ---------------------------------------------------------------------------
// VM handle
// ---------------------------------------------------------------------------

/// Opaque handle to a running VZ virtual machine.
typedef void *VZVMHandle;

// ---------------------------------------------------------------------------
// Shared directory descriptor (passed from Rust to Swift).
// ---------------------------------------------------------------------------

typedef struct {
    const char *tag;
    const char *host_path;
    bool read_only;
} VZSharedDir;

// ---------------------------------------------------------------------------
// VM configuration (passed from Rust to Swift).
// ---------------------------------------------------------------------------

typedef struct {
    const char *kernel_path;
    const char *initrd_path;   // NULL if not used
    const char *cmdline;       // NULL => "console=hvc0"
    const char *disk_path;
    const char *console_log_path; // NULL => write to stdout
    uint32_t cpus;
    uint64_t memory_mb;
    bool rosetta;
    const VZSharedDir *shared_dirs;
    uint32_t shared_dirs_count;
} VZVMConfig;

// ---------------------------------------------------------------------------
// Lifecycle functions
// ---------------------------------------------------------------------------

/// Create and start a VM synchronously. Returns a handle on success, or
/// NULL on failure (with *out_error set to a heap-allocated error string).
///
/// The VM runs on an internal dispatch queue. The caller must eventually call
/// vz_stop_vm() and vz_destroy_vm() to clean up.
VZVMHandle vz_create_and_start_vm(const VZVMConfig *config,
                                   VZErrorString *out_error);

/// Request the VM to stop. Blocks until the VM has stopped or the timeout
/// (in seconds) expires. Returns 0 on success, non-zero on failure.
int32_t vz_stop_vm(VZVMHandle handle, double timeout_secs,
                    VZErrorString *out_error);

/// Destroy the VM handle and free all associated resources. The VM must
/// already be stopped. Returns 0 on success.
int32_t vz_destroy_vm(VZVMHandle handle, VZErrorString *out_error);

// ---------------------------------------------------------------------------
// Query functions
// ---------------------------------------------------------------------------

/// Returns the VM state as an integer:
///   0 = stopped, 1 = running, 2 = paused, 3 = error, 4 = starting,
///   5 = pausing, 6 = resuming, 7 = stopping, -1 = unknown/invalid handle.
int32_t vz_vm_state(VZVMHandle handle);

/// Read console output from the VM's serial port log.
/// Reads up to `buffer_len` bytes starting at `offset` into `buffer`.
/// On success, writes the number of bytes actually read to `*out_bytes_read`
/// and returns 0. Returns non-zero on error.
int32_t vz_read_console(VZVMHandle handle, uint64_t offset,
                         uint8_t *buffer, uint64_t buffer_len,
                         uint64_t *out_bytes_read,
                         VZErrorString *out_error);

/// Check whether Rosetta translation is available on this system.
/// Returns true on Apple Silicon with Rosetta support.
bool vz_rosetta_available(void);

// ---------------------------------------------------------------------------
// Disk image utilities
// ---------------------------------------------------------------------------

/// Create a raw (sparse) disk image at the given path with the specified
/// size in bytes. Returns 0 on success.
int32_t vz_create_disk_image(const char *path, uint64_t size_bytes,
                              VZErrorString *out_error);

#ifdef __cplusplus
}
#endif

#endif // VZ_BRIDGE_H
