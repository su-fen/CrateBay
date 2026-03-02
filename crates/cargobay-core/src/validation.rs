//! Input validation functions for security hardening.
//!
//! All validators return `Ok(())` on success or `Err(String)` with a
//! human-readable description of the violation.

/// Validate a VM name.
///
/// Rules:
/// - 1 to 63 characters
/// - Only lowercase alphanumeric characters and hyphens
/// - Must not start or end with a hyphen
/// - Must start with a letter
pub fn validate_vm_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("VM name must not be empty".into());
    }
    if name.len() > 63 {
        return Err(format!(
            "VM name must be at most 63 characters, got {}",
            name.len()
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("VM name must not start or end with a hyphen".into());
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err("VM name must start with a lowercase letter".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "VM name must contain only lowercase alphanumeric characters and hyphens".into(),
        );
    }
    Ok(())
}

/// Validate a Docker container name.
///
/// Rules (matching Docker naming conventions):
/// - 1 to 128 characters
/// - Must match `[a-zA-Z0-9][a-zA-Z0-9_.-]*`
pub fn validate_container_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Container name must not be empty".into());
    }
    if name.len() > 128 {
        return Err(format!(
            "Container name must be at most 128 characters, got {}",
            name.len()
        ));
    }
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        if !first.is_ascii_alphanumeric() {
            return Err("Container name must start with an alphanumeric character".into());
        }
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return Err(
            "Container name must contain only alphanumeric characters, underscores, dots, and hyphens".into(),
        );
    }
    Ok(())
}

/// Validate a Docker image reference.
///
/// Accepts references in the forms:
/// - `image`
/// - `image:tag`
/// - `registry/image`
/// - `registry/image:tag`
/// - `registry:port/image:tag`
/// - `registry/namespace/image:tag@sha256:...`
///
/// Rejects empty strings and strings containing whitespace or shell meta-characters.
pub fn validate_image_reference(reference: &str) -> Result<(), String> {
    if reference.is_empty() {
        return Err("Image reference must not be empty".into());
    }
    if reference.len() > 512 {
        return Err(format!(
            "Image reference must be at most 512 characters, got {}",
            reference.len()
        ));
    }
    if reference.contains(char::is_whitespace) {
        return Err("Image reference must not contain whitespace".into());
    }
    // Reject shell meta-characters that could enable injection.
    const SHELL_META: &[char] = &[
        ';', '&', '|', '$', '`', '(', ')', '{', '}', '<', '>', '!', '\\', '\'', '"',
    ];
    for ch in SHELL_META {
        if reference.contains(*ch) {
            return Err(format!(
                "Image reference contains invalid character '{}'",
                ch
            ));
        }
    }
    // Must have at least one component that looks like a name.
    let name_part = reference.split(':').next().unwrap_or("");
    let name_part = name_part.split('@').next().unwrap_or("");
    if name_part.is_empty() {
        return Err("Image reference has an empty name component".into());
    }
    Ok(())
}

/// Validate a port number.
///
/// Ports 0 are not valid for binding. Ports 1-65535 are accepted.
pub fn validate_port(port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("Port must be between 1 and 65535".into());
    }
    Ok(())
}

/// Validate a mount path for safety.
///
/// Rules:
/// - Must be an absolute path (starts with `/` on Unix or a drive letter on Windows)
/// - Must not contain `..` path traversal components
/// - Must not be empty
/// - Must not contain null bytes
pub fn validate_mount_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Mount path must not be empty".into());
    }
    if path.contains('\0') {
        return Err("Mount path must not contain null bytes".into());
    }

    // Check for path traversal.
    for component in path.split('/') {
        if component == ".." {
            return Err("Mount path must not contain '..' path traversal".into());
        }
    }
    // Also check Windows-style separators.
    for component in path.split('\\') {
        if component == ".." {
            return Err("Mount path must not contain '..' path traversal".into());
        }
    }

    // Must be absolute.
    let is_absolute = path.starts_with('/')
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && (path.as_bytes()[2] == b'\\' || path.as_bytes()[2] == b'/'));

    if !is_absolute {
        return Err("Mount path must be absolute (e.g. /home/user/data or C:\\Users\\data)".into());
    }

    Ok(())
}

/// Sanitize a string for safe inclusion in log messages.
///
/// Strips control characters (except newline and tab) to prevent log injection
/// and terminal escape sequence attacks.
pub fn sanitize_log_string(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                // Replace control characters with the Unicode replacement character.
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // validate_vm_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_name_valid_simple() {
        assert!(validate_vm_name("my-vm").is_ok());
    }

    #[test]
    fn vm_name_valid_single_char() {
        assert!(validate_vm_name("a").is_ok());
    }

    #[test]
    fn vm_name_valid_with_numbers() {
        assert!(validate_vm_name("vm-01-test").is_ok());
    }

    #[test]
    fn vm_name_valid_63_chars() {
        let name = "a".repeat(63);
        assert!(validate_vm_name(&name).is_ok());
    }

    #[test]
    fn vm_name_empty() {
        let err = validate_vm_name("").unwrap_err();
        assert!(err.contains("empty"), "error: {}", err);
    }

    #[test]
    fn vm_name_too_long() {
        let name = "a".repeat(64);
        let err = validate_vm_name(&name).unwrap_err();
        assert!(err.contains("63"), "error: {}", err);
    }

    #[test]
    fn vm_name_starts_with_hyphen() {
        let err = validate_vm_name("-bad").unwrap_err();
        assert!(err.contains("hyphen"), "error: {}", err);
    }

    #[test]
    fn vm_name_ends_with_hyphen() {
        let err = validate_vm_name("bad-").unwrap_err();
        assert!(err.contains("hyphen"), "error: {}", err);
    }

    #[test]
    fn vm_name_starts_with_number() {
        let err = validate_vm_name("1bad").unwrap_err();
        assert!(err.contains("lowercase letter"), "error: {}", err);
    }

    #[test]
    fn vm_name_uppercase() {
        let err = validate_vm_name("Bad").unwrap_err();
        assert!(err.contains("lowercase"), "error: {}", err);
    }

    #[test]
    fn vm_name_special_chars() {
        let err = validate_vm_name("bad_vm").unwrap_err();
        assert!(err.contains("alphanumeric"), "error: {}", err);
    }

    #[test]
    fn vm_name_spaces() {
        let err = validate_vm_name("bad vm").unwrap_err();
        assert!(err.contains("alphanumeric"), "error: {}", err);
    }

    // -----------------------------------------------------------------------
    // validate_container_name tests
    // -----------------------------------------------------------------------

    #[test]
    fn container_name_valid_simple() {
        assert!(validate_container_name("my-container").is_ok());
    }

    #[test]
    fn container_name_valid_underscore_dot() {
        assert!(validate_container_name("my_container.v2").is_ok());
    }

    #[test]
    fn container_name_valid_uppercase() {
        assert!(validate_container_name("MyContainer").is_ok());
    }

    #[test]
    fn container_name_empty() {
        assert!(validate_container_name("").is_err());
    }

    #[test]
    fn container_name_too_long() {
        let name = "a".repeat(129);
        assert!(validate_container_name(&name).is_err());
    }

    #[test]
    fn container_name_starts_with_hyphen() {
        let err = validate_container_name("-bad").unwrap_err();
        assert!(err.contains("alphanumeric"), "error: {}", err);
    }

    #[test]
    fn container_name_starts_with_dot() {
        let err = validate_container_name(".bad").unwrap_err();
        assert!(err.contains("alphanumeric"), "error: {}", err);
    }

    #[test]
    fn container_name_spaces() {
        assert!(validate_container_name("bad name").is_err());
    }

    #[test]
    fn container_name_special_chars() {
        assert!(validate_container_name("bad$name").is_err());
    }

    // -----------------------------------------------------------------------
    // validate_image_reference tests
    // -----------------------------------------------------------------------

    #[test]
    fn image_ref_valid_simple() {
        assert!(validate_image_reference("nginx").is_ok());
    }

    #[test]
    fn image_ref_valid_with_tag() {
        assert!(validate_image_reference("nginx:latest").is_ok());
    }

    #[test]
    fn image_ref_valid_registry() {
        assert!(validate_image_reference("ghcr.io/org/image:v1.2.3").is_ok());
    }

    #[test]
    fn image_ref_valid_registry_port() {
        assert!(validate_image_reference("localhost:5000/myimage:latest").is_ok());
    }

    #[test]
    fn image_ref_valid_with_digest() {
        assert!(validate_image_reference(
            "nginx@sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        )
        .is_ok());
    }

    #[test]
    fn image_ref_empty() {
        assert!(validate_image_reference("").is_err());
    }

    #[test]
    fn image_ref_whitespace() {
        assert!(validate_image_reference("nginx latest").is_err());
    }

    #[test]
    fn image_ref_shell_injection_semicolon() {
        assert!(validate_image_reference("nginx; rm -rf /").is_err());
    }

    #[test]
    fn image_ref_shell_injection_pipe() {
        assert!(validate_image_reference("nginx|cat /etc/passwd").is_err());
    }

    #[test]
    fn image_ref_shell_injection_backtick() {
        assert!(validate_image_reference("nginx`whoami`").is_err());
    }

    #[test]
    fn image_ref_shell_injection_dollar() {
        assert!(validate_image_reference("nginx$(whoami)").is_err());
    }

    #[test]
    fn image_ref_too_long() {
        let name = "a".repeat(513);
        assert!(validate_image_reference(&name).is_err());
    }

    // -----------------------------------------------------------------------
    // validate_port tests
    // -----------------------------------------------------------------------

    #[test]
    fn port_valid_min() {
        assert!(validate_port(1).is_ok());
    }

    #[test]
    fn port_valid_common() {
        assert!(validate_port(8080).is_ok());
    }

    #[test]
    fn port_valid_max() {
        assert!(validate_port(65535).is_ok());
    }

    #[test]
    fn port_zero() {
        assert!(validate_port(0).is_err());
    }

    // -----------------------------------------------------------------------
    // validate_mount_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn mount_path_valid_unix() {
        assert!(validate_mount_path("/home/user/data").is_ok());
    }

    #[test]
    fn mount_path_valid_root() {
        assert!(validate_mount_path("/").is_ok());
    }

    #[test]
    fn mount_path_valid_windows() {
        assert!(validate_mount_path("C:\\Users\\data").is_ok());
    }

    #[test]
    fn mount_path_valid_windows_forward() {
        assert!(validate_mount_path("C:/Users/data").is_ok());
    }

    #[test]
    fn mount_path_empty() {
        assert!(validate_mount_path("").is_err());
    }

    #[test]
    fn mount_path_relative() {
        let err = validate_mount_path("relative/path").unwrap_err();
        assert!(err.contains("absolute"), "error: {}", err);
    }

    #[test]
    fn mount_path_traversal_unix() {
        let err = validate_mount_path("/home/user/../../../etc/passwd").unwrap_err();
        assert!(err.contains(".."), "error: {}", err);
    }

    #[test]
    fn mount_path_traversal_start() {
        let err = validate_mount_path("/../etc/passwd").unwrap_err();
        assert!(err.contains(".."), "error: {}", err);
    }

    #[test]
    fn mount_path_traversal_windows() {
        let err = validate_mount_path("C:\\Users\\..\\System32").unwrap_err();
        assert!(err.contains(".."), "error: {}", err);
    }

    #[test]
    fn mount_path_null_byte() {
        let err = validate_mount_path("/home/user/\0bad").unwrap_err();
        assert!(err.contains("null"), "error: {}", err);
    }

    // -----------------------------------------------------------------------
    // sanitize_log_string tests
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_normal_string() {
        let input = "hello world";
        assert_eq!(sanitize_log_string(input), "hello world");
    }

    #[test]
    fn sanitize_preserves_newlines() {
        let input = "line1\nline2";
        assert_eq!(sanitize_log_string(input), "line1\nline2");
    }

    #[test]
    fn sanitize_preserves_tabs() {
        let input = "col1\tcol2";
        assert_eq!(sanitize_log_string(input), "col1\tcol2");
    }

    #[test]
    fn sanitize_strips_null() {
        let input = "hello\0world";
        assert_eq!(sanitize_log_string(input), "hello\u{FFFD}world");
    }

    #[test]
    fn sanitize_strips_escape() {
        // ESC character (0x1B) used in ANSI escape codes
        let input = "hello\x1b[31mred\x1b[0m";
        let sanitized = sanitize_log_string(input);
        assert!(!sanitized.contains('\x1b'));
        assert!(sanitized.contains('\u{FFFD}'));
    }

    #[test]
    fn sanitize_strips_bell() {
        let input = "hello\x07world";
        assert_eq!(sanitize_log_string(input), "hello\u{FFFD}world");
    }

    #[test]
    fn sanitize_strips_carriage_return() {
        // CR can be used for log line overwriting attacks
        let input = "safe log\rmalicious overwrite";
        let sanitized = sanitize_log_string(input);
        assert!(!sanitized.contains('\r'));
    }

    #[test]
    fn sanitize_empty_string() {
        assert_eq!(sanitize_log_string(""), "");
    }

    #[test]
    fn sanitize_unicode() {
        let input = "hello world";
        assert_eq!(sanitize_log_string(input), "hello world");
    }
}
