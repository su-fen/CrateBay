/// Build script for cratebay-vz.
///
/// On macOS: compiles `bridge/VZBridge.swift` into a static library and links
/// it together with the Virtualization framework.
///
/// On other platforms: this is a no-op.
fn main() {
    #[cfg(target_os = "macos")]
    macos_build();
}

#[cfg(target_os = "macos")]
fn macos_build() {
    use std::env;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let bridge_dir = manifest_dir.join("bridge");
    let swift_file = bridge_dir.join("VZBridge.swift");
    let header_file = bridge_dir.join("VZBridge.h");

    // Tell cargo to rerun if the bridge files change.
    println!("cargo:rerun-if-changed={}", swift_file.display());
    println!("cargo:rerun-if-changed={}", header_file.display());

    let object_file = out_dir.join("VZBridge.o");
    let static_lib = out_dir.join("libvzbridge.a");

    // Determine the macOS SDK path.
    let sdk_output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("Failed to run xcrun --show-sdk-path");
    let sdk_path = String::from_utf8_lossy(&sdk_output.stdout)
        .trim()
        .to_string();

    // Find the Swift resource directory (needed for linking the Swift runtime).
    let swift_path_output = Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .expect("Failed to run xcrun --find swiftc");
    let swiftc_path = String::from_utf8_lossy(&swift_path_output.stdout)
        .trim()
        .to_string();

    // Derive the Swift library path from the swiftc location.
    // e.g. /usr/bin/swiftc -> /usr/lib/swift
    // or Xcode: .../Toolchains/.../usr/bin/swiftc -> .../usr/lib/swift/macosx
    let swiftc_dir = Path::new(&swiftc_path)
        .parent()
        .expect("no parent for swiftc");
    let swift_lib_dir = swiftc_dir
        .parent()
        .unwrap()
        .join("lib")
        .join("swift")
        .join("macosx");

    // Import the C header via a bridging header mechanism:
    // Swift can import C declarations directly using -import-objc-header.
    let status = Command::new(&swiftc_path)
        .args([
            "-c",
            swift_file.to_str().unwrap(),
            "-o",
            object_file.to_str().unwrap(),
            "-sdk",
            &sdk_path,
            "-target",
            &target_triple(),
            "-import-objc-header",
            header_file.to_str().unwrap(),
            "-parse-as-library",
            "-O",
            "-whole-module-optimization",
        ])
        .status()
        .expect("Failed to invoke swiftc");

    if !status.success() {
        panic!("swiftc compilation failed");
    }

    // Create a static library from the object file.
    let ar_status = Command::new("ar")
        .args([
            "rcs",
            static_lib.to_str().unwrap(),
            object_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to invoke ar");

    if !ar_status.success() {
        panic!("ar failed to create static library");
    }

    // Link instructions for cargo.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=vzbridge");

    // Link the Virtualization framework.
    println!("cargo:rustc-link-lib=framework=Virtualization");

    // Link the Swift runtime libraries.
    if swift_lib_dir.exists() {
        let swift_lib_path = swift_lib_dir.display().to_string();
        println!("cargo:rustc-link-search=native={}", swift_lib_path);
        // Set the rpath so that test binaries (and debug builds) can find
        // libswiftCore.dylib at runtime. Without this, `cargo test` fails on
        // Intel macOS with "Library not loaded: @rpath/libswiftCore.dylib".
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", swift_lib_path);
    }

    // Also check the SDK's Swift library path.
    let sdk_swift_lib = Path::new(&sdk_path).join("usr").join("lib").join("swift");
    if sdk_swift_lib.exists() {
        let sdk_swift_lib_path = sdk_swift_lib.display().to_string();
        println!("cargo:rustc-link-search=native={}", sdk_swift_lib_path);
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", sdk_swift_lib_path);
    }

    // Link system Swift support libraries.
    // On macOS, the Swift runtime is part of the OS since macOS 10.14.4,
    // so we just need to tell the linker where to find the compatibility libs.
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=dylib=swiftFoundation");
    println!("cargo:rustc-link-lib=dylib=swiftDarwin");
    println!("cargo:rustc-link-lib=dylib=swiftDispatch");
    println!("cargo:rustc-link-lib=dylib=swiftObjectiveC");
    println!("cargo:rustc-link-lib=dylib=swiftos");
    println!("cargo:rustc-link-lib=framework=Foundation");
}

#[cfg(target_os = "macos")]
fn target_triple() -> String {
    use std::env;
    // Use Cargo's target if available, otherwise infer from arch.
    if let Ok(target) = env::var("TARGET") {
        // Convert Rust target triple to Apple triple format.
        // e.g. aarch64-apple-darwin -> arm64-apple-macosx13.0
        // e.g. x86_64-apple-darwin -> x86_64-apple-macosx13.0
        let arch = if target.starts_with("aarch64") {
            "arm64"
        } else if target.starts_with("x86_64") {
            "x86_64"
        } else {
            "arm64" // fallback
        };
        format!("{}-apple-macosx13.0", arch)
    } else {
        #[cfg(target_arch = "aarch64")]
        {
            "arm64-apple-macosx13.0".to_string()
        }
        #[cfg(target_arch = "x86_64")]
        {
            "x86_64-apple-macosx13.0".to_string()
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            "arm64-apple-macosx13.0".to_string()
        }
    }
}
