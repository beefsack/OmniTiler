//! Process, token, and known-folder primitives backing `native.rs`.
//!
//! Handles are RAII-owned here (`ProcessHandle`, `TokenHandle`); token
//! buffers, SID strings, and wide-string scratch never leave this file.
//! Errors mirror the previous mapping vocabulary so `native.rs` keeps its
//! exact public errors.

use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TOKEN_USER, TokenIntegrityLevel, TokenUser,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
    PROCESS_NAME_WIN32, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};
use windows_sys::core::PWSTR;

/// Process-query failure in the previous mapping vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcError {
    Absent,
    AccessDenied,
    Code(u32),
}

/// Owned process handle; closing is automatic and the raw value never
/// leaves this module.
pub struct ProcessHandle {
    raw: HANDLE,
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                CloseHandle(self.raw);
            }
        }
    }
}

struct TokenHandle {
    raw: HANDLE,
}

impl Drop for TokenHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                CloseHandle(self.raw);
            }
        }
    }
}

pub fn last_error() -> u32 {
    unsafe { GetLastError() }
}

fn map_open_error(code: u32) -> ProcError {
    match code {
        5 => ProcError::AccessDenied,
        // OpenProcess reports a bad/nonexistent pid as invalid parameter.
        2 | 87 => ProcError::Absent,
        other => ProcError::Code(other),
    }
}

pub fn open_process_with(pid: u32, access: u32) -> Result<ProcessHandle, ProcError> {
    let raw = unsafe { OpenProcess(access, 0, pid) };
    if raw.is_null() {
        return Err(map_open_error(last_error()));
    }
    Ok(ProcessHandle { raw })
}

/// Outcome of a zero-timeout or bounded liveness wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Signaled,
    Pending,
    Failed(u32),
}

pub fn wait_for_object(handle: &ProcessHandle, timeout_ms: u32) -> WaitOutcome {
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_FAILED: u32 = 0xFFFF_FFFF;
    let wait = unsafe { WaitForSingleObject(handle.raw, timeout_ms) };
    if wait == WAIT_OBJECT_0 {
        WaitOutcome::Signaled
    } else if wait == WAIT_FAILED {
        WaitOutcome::Failed(last_error())
    } else {
        WaitOutcome::Pending
    }
}

pub fn terminate_process(handle: &ProcessHandle) -> Result<(), ProcError> {
    let ok = unsafe { TerminateProcess(handle.raw, 1) };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        return Err(ProcError::Code(code));
    }
    Ok(())
}

pub fn current_process_id() -> u32 {
    unsafe { GetCurrentProcessId() }
}

fn open_process_token_raw(process: HANDLE) -> Result<TokenHandle, ProcError> {
    let mut token: HANDLE = std::ptr::null_mut();
    let ok = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        return Err(ProcError::Code(code));
    }
    Ok(TokenHandle { raw: token })
}

fn token_bytes(token: &TokenHandle, class: i32) -> Result<Vec<u8>, ProcError> {
    let mut needed: u32 = 0;
    unsafe {
        GetTokenInformation(token.raw, class, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed == 0 {
        return Err(ProcError::Code(last_error()));
    }
    let mut buf = vec![0u8; needed as usize];
    let mut returned: u32 = 0;
    let ok = unsafe {
        GetTokenInformation(
            token.raw,
            class,
            buf.as_mut_ptr().cast(),
            needed,
            &mut returned,
        )
    };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        return Err(ProcError::Code(code));
    }
    Ok(buf)
}

fn wide_len(ptr: PWSTR) -> usize {
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
    }
    len
}

fn sid_to_string(sid: *mut std::ffi::c_void) -> Result<String, ProcError> {
    let mut out: PWSTR = std::ptr::null_mut();
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut out) };
    if ok == 0 || out.is_null() {
        return Err(ProcError::Code(last_error()));
    }
    let len = wide_len(out);
    let text = unsafe { std::slice::from_raw_parts(out, len) };
    let sid = String::from_utf16_lossy(text);
    unsafe {
        LocalFree(out.cast());
    }
    Ok(sid)
}

pub fn user_sid_of(handle: &ProcessHandle) -> Result<String, ProcError> {
    let token = open_process_token_raw(handle.raw)?;
    let buf = token_bytes(&token, TokenUser)?;
    if buf.len() < std::mem::size_of::<TOKEN_USER>() {
        return Err(ProcError::Code(24));
    }
    let user = unsafe { buf.as_ptr().cast::<TOKEN_USER>().read_unaligned() };
    sid_to_string(user.User.Sid)
}

pub fn integrity_rid_of(handle: &ProcessHandle) -> Result<u32, ProcError> {
    let token = open_process_token_raw(handle.raw)?;
    integrity_rid_of_token(&token)
}

fn integrity_rid_of_token(token: &TokenHandle) -> Result<u32, ProcError> {
    let buf = token_bytes(token, TokenIntegrityLevel)?;
    if buf.len() < std::mem::size_of::<TOKEN_MANDATORY_LABEL>() {
        return Err(ProcError::Code(24));
    }
    let label = unsafe {
        buf.as_ptr()
            .cast::<TOKEN_MANDATORY_LABEL>()
            .read_unaligned()
    };
    let sid = label.Label.Sid;
    if sid.is_null() {
        return Err(ProcError::Code(87));
    }
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    if count == 0 {
        return Err(ProcError::Code(87));
    }
    let rid = unsafe { *GetSidSubAuthority(sid, u32::from(count) - 1) };
    Ok(rid)
}

/// Current-process integrity level; open failures report the raw code
/// (no access-denied special case, as before).
pub fn integrity_rid_current() -> Result<u32, ProcError> {
    let token = open_current_process_token().map_err(ProcError::Code)?;
    integrity_rid_of_token(&token)
}

/// Current-process token; open failures report the raw code.
fn open_current_process_token() -> Result<TokenHandle, u32> {
    let mut token: HANDLE = std::ptr::null_mut();
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok == 0 {
        return Err(last_error());
    }
    Ok(TokenHandle { raw: token })
}

pub fn creation_key_of(handle: &ProcessHandle) -> Result<String, ProcError> {
    let mut creation = std::mem::MaybeUninit::zeroed();
    let mut exit = std::mem::MaybeUninit::zeroed();
    let mut kernel = std::mem::MaybeUninit::zeroed();
    let mut user = std::mem::MaybeUninit::zeroed();
    let ok = unsafe {
        GetProcessTimes(
            handle.raw,
            creation.as_mut_ptr(),
            exit.as_mut_ptr(),
            kernel.as_mut_ptr(),
            user.as_mut_ptr(),
        )
    };
    if ok == 0 {
        return Err(ProcError::Code(last_error()));
    }
    let creation = unsafe { creation.assume_init() };
    let combined = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    Ok(format!("{combined:016x}"))
}

pub fn image_path_of(handle: &ProcessHandle) -> Result<String, ProcError> {
    let mut buf = vec![0u16; 32_768];
    let mut size = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle.raw, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size)
    };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        return Err(ProcError::Code(code));
    }
    Ok(String::from_utf16_lossy(&buf[..size as usize]))
}

pub fn current_image_path() -> Result<String, ProcError> {
    let mut buf = vec![0u16; 32_768];
    let mut size = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            GetCurrentProcess(),
            PROCESS_NAME_WIN32,
            buf.as_mut_ptr(),
            &mut size,
        )
    };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        return Err(ProcError::Code(code));
    }
    Ok(String::from_utf16_lossy(&buf[..size as usize]))
}

pub fn session_of_pid(pid: u32) -> Result<u32, ProcError> {
    let mut session: u32 = 0;
    let ok = unsafe { ProcessIdToSessionId(pid, &mut session) };
    if ok == 0 {
        let code = last_error();
        if code == 5 {
            return Err(ProcError::AccessDenied);
        }
        // Nonexistent pid surfaces as invalid parameter here as well.
        if code == 87 {
            return Err(ProcError::Absent);
        }
        return Err(ProcError::Code(code));
    }
    Ok(session)
}

pub fn local_app_data_dir() -> Result<PathBuf, ProcError> {
    let mut out: PWSTR = std::ptr::null_mut();
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, std::ptr::null_mut(), &mut out) };
    if hr != 0 || out.is_null() {
        return Err(ProcError::Code(hr as u32));
    }
    let len = wide_len(out);
    let text = unsafe { std::slice::from_raw_parts(out, len) };
    let base = std::ffi::OsString::from_wide(text);
    unsafe {
        CoTaskMemFree(out.cast());
    }
    Ok(PathBuf::from(base))
}
