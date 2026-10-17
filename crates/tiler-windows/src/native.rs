#![forbid(unsafe_code)]
//! Process identity, integrity, and product directories.
//!
//! Safe orchestration only: every native call runs inside `crate::ffi`.

use std::path::PathBuf;

use crate::ffi::{self, ProcError, ProcessHandle, WaitOutcome};
use crate::model::ProcessIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    Absent,
    AccessDenied,
    Win32(u32),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => f.write_str("absent"),
            Self::AccessDenied => f.write_str("access-denied"),
            Self::Win32(code) => write!(f, "win32-{code}"),
        }
    }
}

impl std::error::Error for IdentityError {}

fn map_error(err: ProcError) -> IdentityError {
    match err {
        ProcError::Absent => IdentityError::Absent,
        ProcError::AccessDenied => IdentityError::AccessDenied,
        ProcError::Code(code) => IdentityError::Win32(code),
    }
}

pub fn query_identity(pid: u32) -> Result<ProcessIdentity, IdentityError> {
    // Single open+liveness path shared with HeldProcess.
    HeldProcess::open(pid)?.identity()
}

pub fn current_identity() -> Result<ProcessIdentity, IdentityError> {
    query_identity(ffi::current_process_id())
}

pub fn query_integrity_level(pid: u32) -> Result<u32, IdentityError> {
    HeldProcess::open(pid)?.integrity()
}

pub fn current_integrity_level() -> Result<u32, IdentityError> {
    ffi::integrity_rid_current().map_err(map_error)
}

pub fn ledger_directory() -> Result<PathBuf, IdentityError> {
    let session = ffi::session_of_pid(ffi::current_process_id()).map_err(map_error)?;
    Ok(local_app_data()?
        .join("omnitiler")
        .join(format!("session-{session}")))
}

/// Per-user settings directory (`%LOCALAPPDATA%\omnitiler`, no
/// session suffix): the product root proven by the known-folder path above.
/// The settings file lives here so every session owner shares one store.
pub fn settings_directory() -> Result<PathBuf, IdentityError> {
    Ok(local_app_data()?.join("omnitiler"))
}

fn local_app_data() -> Result<PathBuf, IdentityError> {
    ffi::local_app_data_dir().map_err(map_error)
}

pub struct HeldProcess {
    inner: ProcessHandle,
    pid: u32,
}

impl HeldProcess {
    fn open_with(pid: u32, access: u32) -> Result<Self, IdentityError> {
        let inner = ffi::open_process_with(pid, access).map_err(map_error)?;
        match ffi::wait_for_object(&inner, 0) {
            WaitOutcome::Signaled => Err(IdentityError::Absent),
            WaitOutcome::Failed(code) => Err(IdentityError::Win32(code)),
            WaitOutcome::Pending => Ok(Self { inner, pid }),
        }
    }

    pub fn open(pid: u32) -> Result<Self, IdentityError> {
        Self::open_with(
            pid,
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION
                | windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
        )
    }

    pub fn open_terminate(pid: u32) -> Result<Self, IdentityError> {
        Self::open_with(
            pid,
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION
                | windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE
                | windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
        )
    }

    pub fn identity(&self) -> Result<ProcessIdentity, IdentityError> {
        Ok(ProcessIdentity {
            pid: self.pid,
            process_creation: ffi::creation_key_of(&self.inner).map_err(map_error)?,
            user_sid: ffi::user_sid_of(&self.inner).map_err(map_error)?,
            session_id: ffi::session_of_pid(self.pid).map_err(map_error)?,
            exe_path: ffi::image_path_of(&self.inner).map_err(map_error)?,
        })
    }

    pub fn integrity(&self) -> Result<u32, IdentityError> {
        ffi::integrity_rid_of(&self.inner).map_err(map_error)
    }

    pub fn is_alive(&self) -> bool {
        !matches!(ffi::wait_for_object(&self.inner, 0), WaitOutcome::Signaled)
    }

    pub fn wait(&self, timeout_ms: u32) -> bool {
        matches!(
            ffi::wait_for_object(&self.inner, timeout_ms),
            WaitOutcome::Signaled
        )
    }

    pub fn terminate(&self) -> Result<(), IdentityError> {
        ffi::terminate_process(&self.inner).map_err(map_error)
    }
}

pub fn current_exe_path() -> Result<String, IdentityError> {
    ffi::current_image_path().map_err(map_error)
}
