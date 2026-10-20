//! Private local transport: per-run endpoint, nonce, bounded frames.
//!
//! Linux/macOS serve over a Unix-domain socket inside a per-run directory
//! created mode `0700`. Windows serves over a Tokio named pipe created with
//! an owner-only security descriptor (see `pipe_win.rs`): the DACL grants
//! access to the literal current-user SID plus `SYSTEM`, remote clients are
//! refused, the first instance is exclusive, and the per-run nonce still
//! guards every request. On Windows `--dir` is lifecycle/naming compat only
//! (created/removed with the run): it is never a filesystem security
//! boundary. See the crate README for details.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::protocol::MAX_LINE_BYTES;

#[cfg(windows)]
use crate::pipe_win;

/// Prepared endpoint: socket path plus the per-run connection nonce.
///
/// On Unix `socket_path` is the socket file. On Windows it is the full
/// pipe name (`\\.\pipe\...`); `send --socket` accepts the same string.
pub struct Endpoint {
    pub socket_path: PathBuf,
    pub nonce: String,
}

#[cfg(unix)]
pub type Listener = std::os::unix::net::UnixListener;
#[cfg(unix)]
pub type Stream = std::os::unix::net::UnixStream;

#[cfg(windows)]
pub type Listener = PipeListener;

#[cfg(not(any(unix, windows)))]
pub type Listener = ();
#[cfg(not(any(unix, windows)))]
pub type Stream = ();

/// Generate an unpredictable per-run nonce (64 hex chars).
///
/// Reads from the OS entropy source via `getrandom` (works on
/// Linux/macOS/Windows). If entropy is unavailable this fails: the nonce
/// guards every request, so a predictable fallback secret is worse than
/// refusing to start. The value is never written to logs.
pub fn generate_nonce() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| "OS entropy unavailable; refusing to start with a weak nonce".to_string())?;
    Ok(hex(&bytes))
}

/// Generate a short public run tag for window titles. This is a window label,
/// not a secret: it is visible in window titles by design, so a hash
/// fallback is acceptable where the nonce is not.
pub fn generate_run_tag() -> String {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_ok() {
        return hex(&bytes);
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::process::id().hash(&mut hasher);
    std::time::SystemTime::now().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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

/// Plain socket/pipe endpoint names: non-empty, no path separators, no NUL.
/// Unix already rejects `/`; Windows also rejects `\`, so one validator
/// covers both transports and keeps `--socket-name` portable.
pub fn socket_name_is_valid(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains('\0')
}

/// Sanitize one pipe-name component: ASCII alphanumerics plus `_`, `-`,
/// `.` survive; everything else becomes `_`. Valid socket names pass
/// through unchanged; directory basenames are flattened into the pipe name.
#[cfg(any(test, windows))]
pub fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Deterministic 64-bit FNV-1a over bytes. Used only to disambiguate pipe
/// names for different `--dir` values sharing a basename; it is naming,
/// never a secret.
#[cfg(any(test, windows))]
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Full Windows pipe name for a run directory plus socket name.
///
/// Shape: `\\.\pipe\tiler-live-fixture-<base>-<hash>-<sock>` where `<base>`
/// is the sanitized directory basename (truncated to 32 chars) and `<hash>`
/// is FNV-1a over the full directory string, so distinct `--dir` values
/// never share a pipe while the same `--dir` reuses its pipe (a second
/// serve then fails instead of hijacking). Pure and portable so unit tests
/// run on Linux too.
#[cfg(any(test, windows))]
pub fn windows_pipe_name(dir: &Path, name: &str) -> std::io::Result<String> {
    if !socket_name_is_valid(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "socket name must be a plain file name",
        ));
    }
    let base_raw = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "run".to_string());
    let mut base = sanitize_component(&base_raw);
    if base.len() > 32 {
        base.truncate(32);
    }
    let sock = sanitize_component(name);
    if sock.len() > 64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "socket name is too long for a pipe name",
        ));
    }
    let hash = fnv1a64(dir.as_os_str().to_string_lossy().as_bytes());
    let full = format!("\\\\.\\pipe\\tiler-live-fixture-{base}-{hash:016x}-{sock}");
    if full.len() > 240 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pipe name is too long",
        ));
    }
    Ok(full)
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

/// Prepare the Windows run directory.
///
/// Lifecycle/naming compat only, explicitly not a security boundary: the
/// directory is created non-recursively (parents must exist) or an existing
/// real directory is accepted; symlinks and non-directories are refused.
/// No mode/ownership enforcement is attempted (meaningless for pipes); the
/// pipe DACL plus the per-run nonce are the security boundary.
#[cfg(windows)]
pub fn prepare_socket_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is a symlink, refusing: {}", dir.display()),
                ));
            }
            if !meta.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is not a directory: {}", dir.display()),
                ));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::DirBuilder::new().recursive(false).create(dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            let meta = std::fs::symlink_metadata(dir)?;
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is a symlink, refusing: {}", dir.display()),
                ));
            }
            if !meta.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("socket dir is not a directory: {}", dir.display()),
                ));
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(not(any(unix, windows)))]
pub fn prepare_socket_dir(dir: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "local transport is not supported on this OS (dir: {})",
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
    if !socket_name_is_valid(name) {
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

/// Bind the owner-restricted Windows pipe: create the first instance now
/// with the exclusive flag, so a second serve for the same `--dir` /
/// `--socket-name` fails instead of hijacking the first server. The
/// returned path is the full pipe name for the `--socket` announcement.
/// Later instances (one per concurrent client) are created without the
/// exclusive flag by [`PipeListener::accept`].
#[cfg(windows)]
pub fn bind_private_socket(dir: &Path, name: &str) -> std::io::Result<(Listener, PathBuf)> {
    if !socket_name_is_valid(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "socket name must be a plain file name",
        ));
    }
    let pipe_name = windows_pipe_name(dir, name)?;
    let sec = pipe_win::PipeSecurity::owner_only()?;
    let first = rt()
        .block_on(async { pipe_win::create_server(std::ffi::OsStr::new(&pipe_name), true, &sec) });
    let first = match first {
        Ok(server) => server,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("pipe already bound by another server, refusing to hijack: {pipe_name}"),
            ));
        }
        Err(e) => return Err(e),
    };
    Ok((
        Listener {
            pipe_name: std::ffi::OsString::from(&pipe_name),
            pending: std::sync::Mutex::new(Some(first)),
        },
        PathBuf::from(pipe_name),
    ))
}

#[cfg(not(any(unix, windows)))]
pub fn bind_private_socket(_dir: &Path, _name: &str) -> std::io::Result<(Listener, PathBuf)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "local transport is not supported on this OS",
    ))
}

/// Connect to the endpoint with a short fail-fast timeout. Unix connects
/// fail immediately when no listener exists; Windows kernel-waits up to 5 s
/// for a server instance before reporting `TimedOut`. Wait paths start their
/// absolute deadline after connect, matching Unix.
#[cfg(unix)]
pub fn connect(socket: &Path) -> std::io::Result<Stream> {
    std::os::unix::net::UnixStream::connect(socket)
}

/// Clone a connection handle for split reader/writer use. Unix duplicates
/// the socket; Windows shares the pipe through reference counting (Tokio's
/// readiness wakers keep concurrent reads and writes independent).
#[cfg(unix)]
pub fn try_clone_stream(s: &Stream) -> std::io::Result<Stream> {
    s.try_clone()
}

/// Per-read timeout for plain (non-deadline) reads. Unix sets the socket
/// timeout; Windows stores it in the stream and the blocking `Read` impl
/// honors it per call, so a trickling peer resets it exactly like Unix.
#[cfg(unix)]
pub fn set_read_timeout(s: &Stream, dur: Option<std::time::Duration>) -> std::io::Result<()> {
    s.set_read_timeout(dur)
}

// ---------------------------------------------------------------------------
// Windows transport: one Tokio multi-thread runtime plus an owner-only
// pipe per connection. All waits below are event-driven (IOCP readiness or
// kernel waits with a deadline); nothing here polls or sleeps.
// ---------------------------------------------------------------------------

/// Process-wide Tokio runtime for Windows pipe IO. The sync `Read`/`Write`
/// impls and the deadline helpers drive event-driven Tokio operations via
/// `block_on` from plain (non-runtime) IO threads. Two workers are plenty:
/// readiness waits complete on IOCP, and each blocking call runs on its
/// calling thread, never on a worker.
#[cfg(windows)]
fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_io()
            .enable_time()
            .build()
            .expect("tiler-live-fixture: tokio runtime failed to start")
    })
}

/// One side of a Windows pipe connection. Server and client pipes share the
/// same readiness API (`readable`/`try_read`, `writable`/`try_write` taking
/// `&self`), so concurrent reader/writer threads share one `Arc` without
/// handle juggling; Tokio keeps independent read/write wakers.
#[cfg(windows)]
#[derive(Clone)]
enum Pipe {
    Server(std::sync::Arc<tokio::net::windows::named_pipe::NamedPipeServer>),
    Client(std::sync::Arc<tokio::net::windows::named_pipe::NamedPipeClient>),
}

#[cfg(windows)]
impl Pipe {
    async fn readable(&self) -> std::io::Result<()> {
        match self {
            Pipe::Server(p) => p.readable().await,
            Pipe::Client(p) => p.readable().await,
        }
    }

    fn try_read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Pipe::Server(p) => p.try_read(buf),
            Pipe::Client(p) => p.try_read(buf),
        }
    }

    async fn writable(&self) -> std::io::Result<()> {
        match self {
            Pipe::Server(p) => p.writable().await,
            Pipe::Client(p) => p.writable().await,
        }
    }

    fn try_write(&self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Pipe::Server(p) => p.try_write(buf),
            Pipe::Client(p) => p.try_write(buf),
        }
    }
}

/// Windows connection: Tokio pipe plus runtime handle plus an optional
/// per-read timeout (set by [`set_read_timeout`], honored by [`Read`]).
#[cfg(windows)]
#[derive(Clone)]
pub struct Stream {
    rt: tokio::runtime::Handle,
    pipe: Pipe,
    read_timeout: std::cell::Cell<Option<std::time::Duration>>,
}

#[cfg(windows)]
impl Stream {
    fn server(
        rt: tokio::runtime::Handle,
        server: tokio::net::windows::named_pipe::NamedPipeServer,
    ) -> Self {
        Self {
            rt,
            pipe: Pipe::Server(std::sync::Arc::new(server)),
            read_timeout: std::cell::Cell::new(None),
        }
    }

    fn client(
        rt: tokio::runtime::Handle,
        client: tokio::net::windows::named_pipe::NamedPipeClient,
    ) -> Self {
        Self {
            rt,
            pipe: Pipe::Client(std::sync::Arc::new(client)),
            read_timeout: std::cell::Cell::new(None),
        }
    }
}

/// Listening Windows pipe: the full pipe name plus the pending instance.
/// [`PipeListener::accept`] connects the pending instance, parks the next
/// one before returning, then hands the connected pipe out — so at least
/// one instance always exists and clients never see `NotFound`.
#[cfg(windows)]
pub struct PipeListener {
    pipe_name: std::ffi::OsString,
    pending: std::sync::Mutex<Option<tokio::net::windows::named_pipe::NamedPipeServer>>,
}

#[cfg(windows)]
impl PipeListener {
    /// Wait for one client (event-driven `connect`, no timeout: shutdown
    /// wakes the loop with a dummy client, mirroring Unix blocking accept).
    pub fn accept(&self) -> std::io::Result<Stream> {
        let rt = rt().handle().clone();
        let server = self
            .pending
            .lock()
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "pipe listener lock poisoned")
            })?
            .take()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "pipe listener closed")
            })?;
        // A failed `connect` is retryable (cancel-safe), so the instance
        // goes back and the caller retries after its backoff.
        if let Err(e) = rt.block_on(async { server.connect().await }) {
            *self.pending.lock().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "pipe listener lock poisoned")
            })? = Some(server);
            return Err(e);
        }
        // Park the next instance before handing this one out. The flagged
        // exclusive create happened once in `bind_private_socket`; every
        // instance parked here omits the flag (concurrent clients need
        // coexisting instances of the same name).
        let sec = pipe_win::PipeSecurity::owner_only()?;
        let next = rt
            .block_on(async { pipe_win::create_server(self.pipe_name.as_os_str(), false, &sec) })?;
        *self.pending.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, "pipe listener lock poisoned")
        })? = Some(next);
        Ok(Stream::server(rt, server))
    }
}

/// Clone a connection handle for split reader/writer use. Unix duplicates
/// the socket; Windows shares the pipe through reference counting (Tokio's
/// readiness wakers keep concurrent reads and writes independent).
#[cfg(windows)]
pub fn try_clone_stream(s: &Stream) -> std::io::Result<Stream> {
    Ok(s.clone())
}

/// Per-read timeout for plain (non-deadline) reads. Unix sets the socket
/// timeout; Windows stores it in the stream and the blocking `Read` impl
/// honors it per call, so a trickling peer resets it exactly like Unix.
#[cfg(windows)]
pub fn set_read_timeout(s: &Stream, dur: Option<std::time::Duration>) -> std::io::Result<()> {
    s.read_timeout.set(dur);
    Ok(())
}

#[cfg(windows)]
impl std::io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let timeout = self.read_timeout.get();
        self.rt.block_on(async {
            let op = async {
                loop {
                    self.pipe.readable().await?;
                    match self.pipe.try_read(buf) {
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        r => return r,
                    }
                }
            };
            match timeout {
                Some(d) => tokio::time::timeout(d, op).await.map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "read timed out")
                })?,
                None => op.await,
            }
        })
    }
}

#[cfg(windows)]
impl std::io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.rt.block_on(async {
            loop {
                self.pipe.writable().await?;
                match self.pipe.try_write(buf) {
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    r => return r,
                }
            }
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One event-driven pipe read against an absolute deadline: waits for
/// IOCP readability, then drains what is ready. Returns `Ok(None)` on
/// clean EOF and `TimedOut` when the deadline passes first. No polling.
#[cfg(windows)]
pub fn read_chunk(
    s: &Stream,
    buf: &mut [u8],
    deadline: std::time::Instant,
) -> std::io::Result<Option<usize>> {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "read timed out",
        ));
    }
    s.rt.block_on(async {
        let op = async {
            loop {
                s.pipe.readable().await?;
                match s.pipe.try_read(buf) {
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Ok(0) => return Ok(None),
                    r => return r.map(Some),
                }
            }
        };
        match tokio::time::timeout(remaining, op).await {
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "read timed out",
            )),
            Ok(r) => r,
        }
    })
}

/// One event-driven pipe write against an absolute deadline: waits for
/// IOCP writability, then writes what fits. No polling.
#[cfg(windows)]
pub fn write_chunk(s: &Stream, buf: &[u8], deadline: std::time::Instant) -> std::io::Result<usize> {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "write timed out",
        ));
    }
    s.rt.block_on(async {
        let op = async {
            loop {
                s.pipe.writable().await?;
                match s.pipe.try_write(buf) {
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    r => return r,
                }
            }
        };
        match tokio::time::timeout(remaining, op).await {
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "write timed out",
            )),
            Ok(r) => r,
        }
    })
}
/// Connect with a short fail-fast timeout. The kernel waits up to 5 s for
/// a server instance ([`pipe_win::wait_for_pipe`]); a raced-away instance
/// retries within the same budget. No polling: every wait blocks in the
/// kernel or on IOCP readiness.
#[cfg(windows)]
pub fn connect(socket: &Path) -> std::io::Result<Stream> {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
    let rt = rt().handle().clone();
    let name = socket.as_os_str();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "connect timed out waiting for pipe",
            ));
        }
        let ms = remaining.as_millis().min(u32::MAX as u128).max(1) as u32;
        match pipe_win::wait_for_pipe(name, ms) {
            Ok(()) => {}
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "connect timed out waiting for pipe",
                    ));
                }
                return Err(e);
            }
        }
        match rt
            .block_on(async { tokio::net::windows::named_pipe::ClientOptions::new().open(name) })
        {
            Ok(client) => return Ok(Stream::client(rt, client)),
            Err(e)
                if (e.kind() == std::io::ErrorKind::NotFound
                    || e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32))
                    && Instant::now() < deadline =>
            {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub fn connect(_socket: &Path) -> std::io::Result<Stream> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "local transport is not supported on this OS",
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
    fn run_tag_is_short_hex() {
        let tag = generate_run_tag();
        assert_eq!(tag.len(), 16);
        assert!(tag.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn nonce_comparison_rejects_mismatch() {
        assert!(nonce_ok("abc", "abc"));
        assert!(!nonce_ok("abc", "abd"));
        assert!(!nonce_ok("ab", "abc"));
    }

    #[test]
    fn socket_names_reject_separators() {
        assert!(socket_name_is_valid("fixture.sock"));
        assert!(!socket_name_is_valid(""));
        assert!(!socket_name_is_valid("a/b"));
        assert!(!socket_name_is_valid("a\\b"));
        assert!(!socket_name_is_valid("a\0b"));
    }

    #[test]
    fn sanitize_flattens_components() {
        assert_eq!(sanitize_component("fixture.sock"), "fixture.sock");
        assert_eq!(sanitize_component("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_component(""), "_");
    }

    #[test]
    fn pipe_names_are_stable_and_disambiguated() {
        let a = Path::new("/tmp/tiler-live-fixture-1-abc");
        let b = Path::new("/tmp/tiler-live-fixture-1-abc");
        let c = Path::new("/var/tmp/tiler-live-fixture-1-abc");
        let pa = windows_pipe_name(a, "fixture.sock").unwrap();
        let pb = windows_pipe_name(b, "fixture.sock").unwrap();
        let pc = windows_pipe_name(c, "fixture.sock").unwrap();
        assert_eq!(pa, pb);
        assert_ne!(pa, pc);
        assert!(pa.starts_with("\\\\.\\pipe\\tiler-live-fixture-"));
        assert!(pa.ends_with("-fixture.sock"));
        assert!(!pa[10..].contains('\\'));
        assert!(!pa[10..].contains('/'));
    }

    #[test]
    fn pipe_names_reject_bad_socket_names() {
        let dir = Path::new("/tmp/run");
        assert!(windows_pipe_name(dir, "").is_err());
        assert!(windows_pipe_name(dir, "a/b").is_err());
        assert!(windows_pipe_name(dir, "a\\b").is_err());
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

    /// Windows headless transport: real pipe round trip plus exclusive
    /// bind. No windows are created.
    #[cfg(windows)]
    #[test]
    fn windows_pipe_round_trip_and_exclusive_bind() {
        use std::io::BufReader;

        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-win-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&dir).unwrap();
        let (listener, pipe_path) = bind_private_socket(&dir, "fixture.sock").unwrap();
        assert!(
            pipe_path
                .to_string_lossy()
                .starts_with("\\\\.\\pipe\\tiler-live-fixture-")
        );
        // A second bind for the same endpoint must fail, never hijack.
        let second = bind_private_socket(&dir, "fixture.sock");
        assert!(second.is_err());
        assert_eq!(
            second.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );

        let server = std::thread::spawn(move || {
            let conn = listener.accept().expect("accept");
            let clone = try_clone_stream(&conn).expect("clone server stream");
            let mut reader = BufReader::new(clone);
            let mut scratch = Vec::new();
            match read_bounded_line(&mut reader, &mut scratch) {
                Ok(FrameRead::Line(line)) => {
                    assert_eq!(line, r#"{"v":1}"#);
                }
                other => panic!("expected request line, got {other:?}"),
            }
            let mut writer = conn;
            write_line(&mut writer, r#"{"v":1,"ok":true}"#).expect("reply");
        });
        let mut client = connect(&pipe_path).expect("connect");
        let reader_clone = try_clone_stream(&client).expect("clone client stream");
        let mut reader = BufReader::new(reader_clone);
        write_line(&mut client, r#"{"v":1}"#).expect("write");
        let mut scratch = Vec::new();
        match read_bounded_line(&mut reader, &mut scratch) {
            Ok(FrameRead::Line(line)) => assert_eq!(line, r#"{"v":1,"ok":true}"#),
            other => panic!("expected reply, got {other:?}"),
        }
        server.join().expect("server thread");
        std::fs::remove_dir(&dir).unwrap();
    }

    /// Connecting where no server listens fails promptly instead of
    /// hanging: the kernel wait is bounded.
    #[cfg(windows)]
    #[test]
    fn windows_connect_to_missing_pipe_fails() {
        let missing = std::path::PathBuf::from(format!(
            "\\\\.\\pipe\\tiler-live-fixture-absent-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        assert!(connect(&missing).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_prepare_refuses_files() {
        let dir = std::env::temp_dir().join(format!(
            "tiler-live-fixture-winfile-{}-{}",
            std::process::id(),
            generate_run_tag()
        ));
        prepare_socket_dir(&dir).unwrap();
        let file = dir.join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        assert!(prepare_socket_dir(&file).is_err());
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
