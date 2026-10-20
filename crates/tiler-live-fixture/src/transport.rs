//! Private local transport: Unix socket endpoint, per-run nonce, bounded frames.
//!
//! Linux/macOS serve over a Unix-domain socket inside a per-run directory
//! created mode `0700`. Windows transport is an explicit unsupported stub:
//! this fixture is a Linux development helper and refuses to pretend a
//! named-pipe equivalent exists here. See the crate README for details.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::protocol::MAX_LINE_BYTES;

/// Prepared endpoint: socket path plus the per-run connection nonce.
pub struct Endpoint {
    pub socket_path: PathBuf,
    pub nonce: String,
}

/// Generate an unpredictable per-run nonce (64 hex chars).
///
/// Reads from the OS entropy source. If entropy is unavailable this fails:
/// the nonce guards every request, so a predictable fallback secret is
/// worse than refusing to start. The value is never written to logs.
pub fn generate_nonce() -> Result<String, String> {
    match read_entropy_32(std::path::Path::new("/dev/urandom")) {
        Some(bytes) => Ok(hex(&bytes)),
        None => Err("OS entropy unavailable; refusing to start with a weak nonce".to_string()),
    }
}

/// Generate a short public run tag for window titles. This is a window label,
/// not a secret: it is visible in window titles by design, so a hash
/// fallback is acceptable where the nonce is not.
pub fn generate_run_tag() -> String {
    match read_entropy_32(std::path::Path::new("/dev/urandom")) {
        Some(bytes) => hex(&bytes[..8]),
        None => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            std::process::id().hash(&mut hasher);
            std::time::SystemTime::now().hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
    }
}

/// Read 32 entropy bytes from an explicit source path. The path parameter
/// exists so tests can point at a missing file and exercise the
/// entropy-unavailable branch without touching the real device.
fn read_entropy_32(path: &std::path::Path) -> Option<[u8; 32]> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = [0u8; 32];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

/// Default per-run socket directory: `$XDG_RUNTIME_DIR/tiler-live-fixture-<pid>-<rand>`
/// or a `tmp` fallback when the runtime dir is missing.
pub fn default_socket_dir() -> PathBuf {
    let rand = generate_run_tag();
    let pid = std::process::id();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        let mut dir = PathBuf::from(runtime);
        dir.push(format!("tiler-live-fixture-{pid}-{rand}"));
        return dir;
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("tiler-live-fixture-{pid}-{rand}"));
    dir
}

/// Create the socket directory private from the start, or accept an existing
/// directory only when it is already an owned private real directory.
/// Existing directories are never chmodded: a shared (e.g. group/other
/// accessible), foreign-owned, non-directory, or symlink path is refused.
/// Fresh directories are created non-recursively with mode `0700` (parents
/// must already exist); a racing `AlreadyExists` is re-verified, never
/// chmodded. Never touches files inside the directory.
#[cfg(unix)]
pub fn prepare_socket_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            // Existing path: verify, never mutate.
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is a symlink, refusing: {}", dir.display()),
                ));
            }
            verify_private_dir(&meta, dir)?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Fresh path: atomic private create, no world-readable interval.
            // Parents must exist (`serve --dir` with a fresh mktemp dir is
            // the expected shape); recursive creation could build a
            // world-readable prefix.
            match std::fs::DirBuilder::new()
                .recursive(false)
                .mode(0o700)
                .create(dir)
            {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Lost a race with another creator: re-verify as an
                    // existing directory, without chmodding it.
                }
                Err(e) => return Err(e),
            }
            let meta = std::fs::symlink_metadata(dir)?;
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is a symlink, refusing: {}", dir.display()),
                ));
            }
            verify_private_dir(&meta, dir)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Verify an existing directory is a real owned private directory.
/// Never mutates: callers refuse instead of chmodding foreign paths.
#[cfg(unix)]
fn verify_private_dir(meta: &std::fs::Metadata, dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("socket dir is not a directory: {}", dir.display()),
        ));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "socket dir must already be mode 0700 (found {mode:03o}); refusing to chmod an existing directory: {}",
                dir.display()
            ),
        ));
    }
    let euid = rustix::process::geteuid().as_raw();
    if meta.uid() != euid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "socket dir is not owned by the current user; refusing: {}",
                dir.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn prepare_socket_dir(dir: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "Unix socket transport is not supported on this OS (dir: {})",
            dir.display()
        ),
    ))
}

/// Bind the endpoint socket. Refuses when the socket path already exists:
/// preexisting paths are never unlinked or overwritten. The new socket is
/// chmodded to `0600` (private); only files this run created are ever
/// removed at shutdown.
#[cfg(unix)]
pub fn bind_private_socket(
    dir: &Path,
    name: &str,
) -> std::io::Result<(std::os::unix::net::UnixListener, PathBuf)> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    if name.contains('/') || name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "socket name must be a plain file name",
        ));
    }
    let mut path = dir.to_path_buf();
    path.push(name);
    if path.exists() || std::fs::symlink_metadata(&path).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "socket path already exists, refusing to unlink or overwrite: {}",
                path.display()
            ),
        ));
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, path))
}

#[cfg(not(unix))]
pub fn bind_private_socket(_dir: &Path, _name: &str) -> std::io::Result<((), PathBuf)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Unix socket transport is not supported on this OS",
    ))
}

#[derive(Debug, PartialEq)]
pub enum FrameRead {
    Line(String),
    Eof,
}

/// Read one bounded JSON-lines frame. `TooLong` callers must close the
/// connection: the frame boundary is unknown without unbounded buffering.
#[derive(Debug, PartialEq)]
pub enum FrameError {
    TooLong,
    InvalidUtf8,
    Io(String),
}

pub fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    scratch: &mut Vec<u8>,
) -> Result<FrameRead, FrameError> {
    scratch.clear();
    loop {
        // Scope the fill_buf borrow: copy the next chunk (up to and
        // including the first newline, so coalesced writes split into
        // separate frames), then consume it after the borrow ends.
        let (take, has_newline) = {
            let chunk = match reader.fill_buf() {
                Ok(chunk) => chunk,
                Err(e) => return Err(FrameError::Io(e.to_string())),
            };
            if chunk.is_empty() {
                if scratch.is_empty() {
                    return Ok(FrameRead::Eof);
                }
                return Err(FrameError::Io("connection closed mid-line".to_string()));
            }
            let (end, has_newline) = match chunk.iter().position(|&b| b == b'\n') {
                Some(pos) => (pos + 1, true),
                None => (chunk.len(), false),
            };
            // Bound the whole frame (including the newline) before buffering.
            if scratch.len().saturating_add(end) > MAX_LINE_BYTES {
                return Err(FrameError::TooLong);
            }
            (chunk[..end].to_vec(), has_newline)
        };
        let end = take.len();
        scratch.extend_from_slice(&take);
        reader.consume(end);
        if has_newline {
            scratch.pop();
            if scratch.ends_with(b"\r") {
                scratch.pop();
            }
            return String::from_utf8(std::mem::take(scratch))
                .map(FrameRead::Line)
                .map_err(|_| FrameError::InvalidUtf8);
        }
    }
}

/// Write one JSON line (message plus `\n`).
pub fn write_line<W: Write>(writer: &mut W, line: &str) -> std::io::Result<()> {
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Constant-time nonce comparison for request authentication.
pub fn nonce_ok(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in provided.bytes().zip(expected.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn round_trip_single_line() {
        let mut reader = BufReader::new("{\"v\":1}\n".as_bytes());
        let mut scratch = Vec::new();
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Line(r#"{"v":1}"#.to_string()))
        );
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Eof)
        );
    }

    #[test]
    fn coalesced_writes_split_into_frames() {
        // Two lines arriving in one fill_buf chunk must not merge: rapid
        // marker+reply writes previously parsed as one invalid frame.
        let mut reader = BufReader::new("{\"a\":1}\n{\"b\":2}\n".as_bytes());
        let mut scratch = Vec::new();
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Line(r#"{"a":1}"#.to_string()))
        );
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Line(r#"{"b":2}"#.to_string()))
        );
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Eof)
        );
    }

    #[test]
    fn eof_mid_line_is_an_error() {
        let mut reader = BufReader::new(r#"{"v":1}"#.as_bytes());
        let mut scratch = Vec::new();
        assert!(matches!(
            read_bounded_line(&mut reader, &mut scratch),
            Err(FrameError::Io(_))
        ));
    }

    #[test]
    fn oversize_frame_is_rejected() {
        let big = "x".repeat(MAX_LINE_BYTES + 16);
        let mut reader = BufReader::new(big.as_bytes());
        let mut scratch = Vec::new();
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Err(FrameError::TooLong)
        );
    }

    #[test]
    fn exactly_max_line_is_accepted() {
        let body = "y".repeat(MAX_LINE_BYTES - 1);
        let framed = format!("{body}\n");
        let mut reader = BufReader::new(framed.as_bytes());
        let mut scratch = Vec::new();
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Ok(FrameRead::Line(body))
        );
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let mut reader = BufReader::new([0xff, 0xfe, b'\n'].as_slice());
        let mut scratch = Vec::new();
        assert_eq!(
            read_bounded_line(&mut reader, &mut scratch),
            Err(FrameError::InvalidUtf8)
        );
    }

    #[test]
    fn nonces_are_unique_and_hex() {
        let a = generate_nonce().expect("entropy must be available");
        let b = generate_nonce().expect("entropy must be available");
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn missing_entropy_is_an_error_not_a_fallback() {
        assert!(read_entropy_32(std::path::Path::new("/nonexistent-entropy-source")).is_none());
    }

    #[test]
    fn nonce_comparison_rejects_mismatch() {
        assert!(nonce_ok("abc", "abc"));
        assert!(!nonce_ok("abc", "abd"));
        assert!(!nonce_ok("ab", "abc"));
    }

    #[cfg(unix)]
    #[test]
    fn bind_refuses_preexisting_path() {
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&dir).unwrap();
        let (listener, path) = bind_private_socket(&dir, "fixture.sock").unwrap();
        assert!(bind_private_socket(&dir, "fixture.sock").is_err());
        drop(listener);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn socket_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        std::fs::remove_dir(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_shared_dir_is_refused_without_mutation() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-shared-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let before = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(before, 0o755);
        assert!(prepare_socket_dir(&dir).is_err());
        // Never chmodded: the shared directory is left exactly as found.
        let after = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(after, 0o755);
        std::fs::remove_dir(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_private_dir_is_permitted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-private-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(prepare_socket_dir(&dir).is_ok());
        std::fs::remove_dir(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_dir_is_refused() {
        let target = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-target-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&target).unwrap();
        let link = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-link-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(prepare_socket_dir(&link).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_dir(&target).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_socket_is_retained_and_new_socket_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-sock-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&dir).unwrap();
        let (listener, path) = bind_private_socket(&dir, "fixture.sock").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // A foreign preexisting path is never unlinked: the second bind
        // fails and the first socket file is still there.
        assert!(bind_private_socket(&dir, "fixture.sock").is_err());
        assert!(std::fs::symlink_metadata(&path).is_ok());
        drop(listener);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn missing_parents_are_refused_not_created() {
        // Non-recursive creation: parents must already exist, so a typo'd
        // --dir can never scaffold a world-readable prefix.
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-test-noparent-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        let nested = dir.join("child");
        assert!(prepare_socket_dir(&nested).is_err());
        assert!(std::fs::symlink_metadata(&nested).is_err());
        assert!(std::fs::symlink_metadata(&dir).is_err());
    }
}
