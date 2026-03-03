// VZBridge.swift — Swift bridge to Apple Virtualization.framework.
//
// This file implements the C-callable functions declared in VZBridge.h.
// It is compiled into a static library by build.rs and linked into the
// cratebay-vz Rust binary.

import Foundation
import Virtualization

// MARK: - Internal bookkeeping

/// Wrapper that holds all state for a running VM.
private final class VMInstance {
    let vm: VZVirtualMachine
    let queue: DispatchQueue
    var lastError: String?
    /// Path to the console log file (if configured), for read-back.
    var consoleLogPath: String?

    init(vm: VZVirtualMachine, queue: DispatchQueue, consoleLogPath: String? = nil) {
        self.vm = vm
        self.queue = queue
        self.consoleLogPath = consoleLogPath
    }
}

/// Thread-safe registry of live VM instances keyed by opaque pointer.
private let registryLock = NSLock()
private var registry: [UnsafeMutableRawPointer: VMInstance] = [:]

private func registerVM(_ instance: VMInstance) -> UnsafeMutableRawPointer {
    let ptr = Unmanaged.passRetained(instance).toOpaque()
    registryLock.lock()
    registry[ptr] = instance
    registryLock.unlock()
    return ptr
}

private func lookupVM(_ handle: UnsafeMutableRawPointer?) -> VMInstance? {
    guard let handle = handle else { return nil }
    registryLock.lock()
    let instance = registry[handle]
    registryLock.unlock()
    return instance
}

private func unregisterVM(_ handle: UnsafeMutableRawPointer?) {
    guard let handle = handle else { return }
    registryLock.lock()
    if let instance = registry.removeValue(forKey: handle) {
        Unmanaged.passUnretained(instance).release()
    }
    registryLock.unlock()
}

// MARK: - Helpers

private func makeError(_ msg: String) -> UnsafeMutablePointer<CChar> {
    return strdup(msg)
}

private func setError(_ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?, _ msg: String) {
    out?.pointee = makeError(msg)
}

// MARK: - C API implementation

@_cdecl("vz_free_string")
public func vz_free_string(_ s: UnsafeMutablePointer<CChar>?) {
    free(s)
}

@_cdecl("vz_rosetta_available")
public func vz_rosetta_available() -> Bool {
    #if arch(arm64)
    if #available(macOS 13.0, *) {
        // VZLinuxRosettaDirectoryShare.availability tells us if Rosetta
        // translation is available for Linux VMs.
        switch VZLinuxRosettaDirectoryShare.availability {
        case .installed:
            return true
        case .notInstalled, .notSupported:
            return false
        @unknown default:
            return false
        }
    }
    #endif
    return false
}

@_cdecl("vz_create_disk_image")
public func vz_create_disk_image(
    _ path: UnsafePointer<CChar>?,
    _ sizeBytes: UInt64,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    guard let path = path else {
        setError(outError, "path is NULL")
        return -1
    }
    let url = URL(fileURLWithPath: String(cString: path))
    do {
        let dir = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: url.path, contents: nil)
        let fh = try FileHandle(forWritingTo: url)
        try fh.truncate(atOffset: sizeBytes)
        fh.closeFile()
        return 0
    } catch {
        setError(outError, "Failed to create disk image: \(error.localizedDescription)")
        return -1
    }
}

@_cdecl("vz_create_and_start_vm")
public func vz_create_and_start_vm(
    _ configPtr: UnsafePointer<VZVMConfig>?,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> UnsafeMutableRawPointer? {
    guard let configPtr = configPtr else {
        setError(outError, "config is NULL")
        return nil
    }
    let cfg = configPtr.pointee

    // --- Kernel path ---
    guard let kernelCStr = cfg.kernel_path else {
        setError(outError, "kernel_path is NULL")
        return nil
    }
    let kernelPath = String(cString: kernelCStr)

    // --- Disk path ---
    guard let diskCStr = cfg.disk_path else {
        setError(outError, "disk_path is NULL")
        return nil
    }
    let diskPath = String(cString: diskCStr)

    // --- Initrd (optional) ---
    var initrdPath: String? = nil
    if let initrdCStr = cfg.initrd_path {
        initrdPath = String(cString: initrdCStr)
    }

    // --- Command line ---
    var cmdline = "console=hvc0"
    if let cmdlineCStr = cfg.cmdline {
        cmdline = String(cString: cmdlineCStr)
    }

    let cpus = max(Int(cfg.cpus), 1)
    let memoryBytes = UInt64(cfg.memory_mb) * 1024 * 1024

    // --- Build VZ configuration ---
    let vzConfig = VZVirtualMachineConfiguration()

    // Boot loader
    let kernelURL = URL(fileURLWithPath: kernelPath)
    let bootLoader = VZLinuxBootLoader(kernelURL: kernelURL)
    bootLoader.commandLine = cmdline
    if let initrdPath = initrdPath {
        bootLoader.initialRamdiskURL = URL(fileURLWithPath: initrdPath)
    }
    vzConfig.bootLoader = bootLoader

    // CPU + Memory
    vzConfig.cpuCount = cpus
    vzConfig.memorySize = memoryBytes

    // --- Storage (virtio-blk) ---
    let diskURL = URL(fileURLWithPath: diskPath)
    let diskAttachment: VZDiskImageStorageDeviceAttachment
    do {
        diskAttachment = try VZDiskImageStorageDeviceAttachment(url: diskURL, readOnly: false)
    } catch {
        setError(outError, "Failed to create disk attachment: \(error.localizedDescription)")
        return nil
    }
    let blockDevice = VZVirtioBlockDeviceConfiguration(attachment: diskAttachment)
    vzConfig.storageDevices = [blockDevice]

    // --- Network (NAT) ---
    let networkDevice = VZVirtioNetworkDeviceConfiguration()
    networkDevice.attachment = VZNATNetworkDeviceAttachment()
    vzConfig.networkDevices = [networkDevice]

    // --- Memory balloon (allows guest to report unused memory) ---
    vzConfig.memoryBalloonDevices = [VZVirtioTraditionalMemoryBalloonDeviceConfiguration()]

    // --- Entropy ---
    vzConfig.entropyDevices = [VZVirtioEntropyDeviceConfiguration()]

    // --- Serial console (file or stdout) ---
    let serialPort = VZVirtioConsoleDeviceSerialPortConfiguration()

    var consoleLogPath: String? = nil
    if let consoleCStr = cfg.console_log_path {
        consoleLogPath = String(cString: consoleCStr)
    }

    if let logPath = consoleLogPath {
        let logURL = URL(fileURLWithPath: logPath)
        let logDir = logURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: logDir, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: logPath) {
            FileManager.default.createFile(atPath: logPath, contents: nil)
        }
        if let writeHandle = FileHandle(forWritingAtPath: logPath) {
            writeHandle.seekToEndOfFile()
            serialPort.attachment = VZFileHandleSerialPortAttachment(
                fileHandleForReading: nil,
                fileHandleForWriting: writeHandle
            )
        } else {
            // Fallback to stdout if file cannot be opened
            serialPort.attachment = VZFileHandleSerialPortAttachment(
                fileHandleForReading: nil,
                fileHandleForWriting: FileHandle.standardOutput
            )
        }
    } else {
        let stdoutHandle = FileHandle.standardOutput
        serialPort.attachment = VZFileHandleSerialPortAttachment(
            fileHandleForReading: nil,
            fileHandleForWriting: stdoutHandle
        )
    }
    vzConfig.serialPorts = [serialPort]

    // --- VirtioFS shared directories ---
    var fileSystems: [VZVirtioFileSystemDeviceConfiguration] = []

    if cfg.shared_dirs_count > 0, let dirs = cfg.shared_dirs {
        for i in 0..<Int(cfg.shared_dirs_count) {
            let sd = dirs[i]
            guard let tagCStr = sd.tag, let hostCStr = sd.host_path else {
                continue
            }
            let tag = String(cString: tagCStr)
            let hostPath = String(cString: hostCStr)

            let sharedDir = VZSharedDirectory(url: URL(fileURLWithPath: hostPath),
                                               readOnly: sd.read_only)
            let share = VZSingleDirectoryShare(directory: sharedDir)
            let fsDevice = VZVirtioFileSystemDeviceConfiguration(tag: tag)
            fsDevice.share = share
            fileSystems.append(fsDevice)
        }
    }

    // --- Rosetta ---
    if cfg.rosetta {
        #if arch(arm64)
        if #available(macOS 13.0, *) {
            if VZLinuxRosettaDirectoryShare.availability == .installed {
                do {
                    let rosettaShare = try VZLinuxRosettaDirectoryShare()
                    let rosettaFS = VZVirtioFileSystemDeviceConfiguration(tag: "rosetta")
                    rosettaFS.share = rosettaShare
                    fileSystems.append(rosettaFS)
                } catch {
                    setError(outError, "Failed to create Rosetta share: \(error.localizedDescription)")
                    return nil
                }
            } else {
                setError(outError, "Rosetta for Linux is not installed on this system")
                return nil
            }
        } else {
            setError(outError, "Rosetta for Linux requires macOS 13.0 or later")
            return nil
        }
        #else
        setError(outError, "Rosetta for Linux is only available on Apple Silicon")
        return nil
        #endif
    }

    if !fileSystems.isEmpty {
        vzConfig.directorySharingDevices = fileSystems
    }

    // --- Validate ---
    do {
        try vzConfig.validate()
    } catch {
        setError(outError, "VZ configuration validation failed: \(error.localizedDescription)")
        return nil
    }

    // --- Create VM on a serial dispatch queue ---
    let queue = DispatchQueue(label: "com.cratebay.vz.vm.\(UUID().uuidString)")
    let semaphore = DispatchSemaphore(value: 0)
    var startError: String? = nil

    var vm: VZVirtualMachine? = nil

    queue.sync {
        vm = VZVirtualMachine(configuration: vzConfig, queue: queue)
    }

    guard let virtualMachine = vm else {
        setError(outError, "Failed to create VZVirtualMachine instance")
        return nil
    }

    let instance = VMInstance(vm: virtualMachine, queue: queue, consoleLogPath: consoleLogPath)

    // Start the VM
    queue.async {
        virtualMachine.start { result in
            switch result {
            case .success:
                break
            case .failure(let error):
                startError = "VZ start failed: \(error.localizedDescription)"
            }
            semaphore.signal()
        }
    }

    // Wait for start to complete (up to 30 seconds)
    let waitResult = semaphore.wait(timeout: .now() + 30)
    if waitResult == .timedOut {
        setError(outError, "Timed out waiting for VM to start (30s)")
        return nil
    }

    if let err = startError {
        setError(outError, err)
        return nil
    }

    let handle = registerVM(instance)
    return handle
}

@_cdecl("vz_stop_vm")
public func vz_stop_vm(
    _ handle: UnsafeMutableRawPointer?,
    _ timeoutSecs: Double,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    guard let instance = lookupVM(handle) else {
        setError(outError, "Invalid VM handle")
        return -1
    }

    let timeout = timeoutSecs > 0 ? timeoutSecs : 10.0

    // Step 1: Try ACPI graceful shutdown via requestStop (sends power button event).
    let acpiSem = DispatchSemaphore(value: 0)
    var acpiError: String? = nil

    instance.queue.async {
        if instance.vm.canRequestStop {
            do {
                try instance.vm.requestStop()
            } catch {
                acpiError = "Failed to request ACPI stop: \(error.localizedDescription)"
            }
        }
        acpiSem.signal()
    }

    // Wait briefly for the requestStop call to complete.
    let _ = acpiSem.wait(timeout: .now() + 2.0)

    // Step 2: Poll the VM state until it stops, or the timeout expires.
    let deadline = DispatchTime.now() + timeout
    var stopped = false

    while DispatchTime.now() < deadline {
        let stateSem = DispatchSemaphore(value: 0)
        var currentState: VZVirtualMachine.State = .running

        instance.queue.async {
            currentState = instance.vm.state
            stateSem.signal()
        }

        let _ = stateSem.wait(timeout: .now() + 2.0)

        if currentState == .stopped || currentState == .error {
            stopped = true
            break
        }

        Thread.sleep(forTimeInterval: 0.25)
    }

    if stopped {
        return 0
    }

    // Step 3: Force stop (VZVirtualMachine.stop) as fallback.
    let forceSem = DispatchSemaphore(value: 0)
    var forceError: String? = nil

    instance.queue.async {
        instance.vm.stop { error in
            if let error = error {
                forceError = "Force stop failed: \(error.localizedDescription)"
            }
            forceSem.signal()
        }
    }

    let forceResult = forceSem.wait(timeout: .now() + 5.0)
    if forceResult == .timedOut {
        let msg = acpiError ?? "Timed out waiting for VM to stop"
        setError(outError, msg)
        return -1
    }

    if let err = forceError {
        setError(outError, err)
        return -1
    }

    return 0
}

@_cdecl("vz_destroy_vm")
public func vz_destroy_vm(
    _ handle: UnsafeMutableRawPointer?,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    guard let instance = lookupVM(handle) else {
        setError(outError, "Invalid VM handle")
        return -1
    }

    // If the VM is still running, attempt to stop it before deallocating.
    let semaphore = DispatchSemaphore(value: 0)

    instance.queue.async {
        let state = instance.vm.state
        if state == .running || state == .paused || state == .starting {
            instance.vm.stop { _ in
                semaphore.signal()
            }
        } else {
            semaphore.signal()
        }
    }

    // Wait up to 5 seconds for the VM to stop.
    let _ = semaphore.wait(timeout: .now() + 5.0)

    unregisterVM(handle)
    return 0
}

@_cdecl("vz_vm_state")
public func vz_vm_state(_ handle: UnsafeMutableRawPointer?) -> Int32 {
    guard let instance = lookupVM(handle) else {
        return -1
    }

    var stateVal: Int32 = -1
    let semaphore = DispatchSemaphore(value: 0)

    instance.queue.async {
        let state = instance.vm.state
        switch state {
        case .stopped:
            stateVal = 0
        case .running:
            stateVal = 1
        case .paused:
            stateVal = 2
        case .error:
            stateVal = 3
        case .starting:
            stateVal = 4
        case .pausing:
            stateVal = 5
        case .resuming:
            stateVal = 6
        case .stopping:
            stateVal = 7
        @unknown default:
            stateVal = -1
        }
        semaphore.signal()
    }

    let _ = semaphore.wait(timeout: .now() + 5.0)
    return stateVal
}

// MARK: - Console read-back

@_cdecl("vz_read_console")
public func vz_read_console(
    _ handle: UnsafeMutableRawPointer?,
    _ offset: UInt64,
    _ buffer: UnsafeMutablePointer<UInt8>?,
    _ bufferLen: UInt64,
    _ outBytesRead: UnsafeMutablePointer<UInt64>?,
    _ outError: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    guard let instance = lookupVM(handle) else {
        setError(outError, "Invalid VM handle")
        return -1
    }

    guard let logPath = instance.consoleLogPath else {
        // No console log configured; return 0 bytes (not an error).
        outBytesRead?.pointee = 0
        return 0
    }

    guard let buffer = buffer, bufferLen > 0 else {
        setError(outError, "buffer is NULL or bufferLen is 0")
        return -1
    }

    guard let fileHandle = FileHandle(forReadingAtPath: logPath) else {
        // File does not exist yet; return 0 bytes.
        outBytesRead?.pointee = 0
        return 0
    }
    defer { fileHandle.closeFile() }

    fileHandle.seek(toFileOffset: offset)
    let data = fileHandle.readData(ofLength: Int(bufferLen))

    if data.count > 0 {
        data.withUnsafeBytes { rawBuf in
            if let baseAddress = rawBuf.baseAddress {
                buffer.initialize(from: baseAddress.assumingMemoryBound(to: UInt8.self), count: data.count)
            }
        }
    }

    outBytesRead?.pointee = UInt64(data.count)
    return 0
}
