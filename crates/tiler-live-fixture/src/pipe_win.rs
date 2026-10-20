#![allow(unsafe_code)]
//! Windows-only native boundary for `tiler-live-fixture`.
//!
//! Sole `unsafe` owner in this crate. Every Win32 call, raw-pointer use,
//! and handle/SID/descriptor lifetime lives here; the transport layer
//! (`transport.rs`) and everything else uses this module through safe APIs
//! that return `io::Result` and owned values, never raw pointers or
//! validity obligations. Buffers are allocated here and freed here;
//! fallible reads fail closed with the OS error, never guessed values.
//!
//! Why this module exists instead of a safe dependency: the preferred
//! maintained pipe crate (`interprocess`) exposes a safe owner-restriction
//! API but pulls `0BSD`-only transitive crates (`doctest-file`, `recvmsg`)
//! that the workspace license gate rejects, and the policy stays unchanged.
//! Tokio (MIT, maintained) supplies the safe async pipe API but deliberately
//! leaves the security descriptor to the caller
//! (`create_with_security_attributes_raw`), so this module builds exactly
//! that one descriptor plus the two small Win32 helpers the transport needs
//! (current-user SID, pipe wait). No other Win32 surface is wrapped here.
//!
//! Owner semantics: the DACL names the literal current-user SID plus
//! `SYSTEM`, never the `OW` (Owner Rights, `S-1-3-4`) pseudo-SID. `OW`
//! resolves to the object's owner at access-check time: for an elevated
//! server the owner may be Administrators rather than the invoking user,
//! and Owner Rights can be curtailed by policy. A literal user SID in a
//! protected DACL, with the descriptor owner field set to the same SID, is
//! the precise owner-only boundary. The per-run nonce still guards every
//! request on top.

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Encode an `OsStr` as NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
fn wide_nul(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

/// Current process user SID as a string (`S-1-5-21-...`).
///
/// Reads the process token (`OpenProcessToken` + `GetTokenInformation`
/// with `TokenUser`) and renders the SID. Every fallible step returns the
/// OS error; the token handle and SID string are released on all paths.
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: all pointers are valid for their call duration (stack slots,
    // caller-owned buffers sized by the length probe); handles and heap
    // strings are released below on every path, exactly once.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        // Length probe: expected to fail; `len` receives the size needed.
        let mut len: u32 = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
        if len == 0 {
            CloseHandle(token);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "GetTokenInformation gave no token size",
            ));
        }
        // Aligned backing store: `TOKEN_USER` is pointer-sized fields only,
        // so `align_of::<TOKEN_USER>() <= align_of::<usize>()` (const-proven
        // below) and a `Vec<usize>` rounded up to `len` bytes satisfies the
        // alignment for the `cast::<TOKEN_USER>` read. A `Vec<u8>` would
        // only guarantee alignment 1: dereferencing the header through it
        // would be undefined behavior. No generic allocation helper is
        // introduced: this is the single site that reads a Win32 struct
        // out of a length-probed buffer.
        const _: () = assert!(std::mem::align_of::<TOKEN_USER>() <= std::mem::align_of::<usize>());
        let words = (len as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buf = vec![0usize; words];
        let cap = (words * std::mem::size_of::<usize>()) as u32;
        if GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            cap,
            &mut len,
        ) == 0
        {
            let err = io::Error::last_os_error();
            CloseHandle(token);
            return Err(err);
        }
        let sid = (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let mut text: *mut u16 = null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            let err = io::Error::last_os_error();
            CloseHandle(token);
            return Err(err);
        }
        let mut out = Vec::new();
        let mut i: usize = 0;
        loop {
            let w = *text.add(i);
            if w == 0 {
                break;
            }
            out.push(w);
            i += 1;
        }
        LocalFree(text as *mut core::ffi::c_void);
        CloseHandle(token);
        String::from_utf16(&out)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "user SID is not UTF-16"))
    }
}

/// Owned absolute-format security descriptor: owner-only DACL.
pub struct PipeSecurity {
    sd: PSECURITY_DESCRIPTOR,
}

// SAFETY: the pointer owns a heap blob from
// `ConvertStringSecurityDescriptorToSecurityDescriptorW`, freed once in
// `Drop`. It is never aliased mutably and never dereferenced outside the
// creating Win32 call, so sharing it across threads is sound.
unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: `sd` came from the converter and is freed exactly once here.
        unsafe {
            LocalFree(self.sd);
        }
    }
}

impl PipeSecurity {
    /// Descriptor with owner field = current user and protected DACL
    /// granting generic-all to exactly the current-user SID and `SYSTEM`.
    /// Everyone else is denied by omission; nothing is inherited.
    pub fn owner_only() -> io::Result<Self> {
        let sid = current_user_sid()?;
        let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})(A;;GA;;;SY)");
        let wide = wide_nul(OsStr::new(&sddl));
        // SAFETY: `wide` is NUL-terminated and lives for the call; `sd`/`len`
        // are valid out-slots. On success `sd` owns a heap blob transferred
        // to `Self` (freed in `Drop`); on failure nothing is owned.
        unsafe {
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            let mut len: u32 = 0;
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                &mut len,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if sd.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "security descriptor converter returned null",
                ));
            }
            Ok(Self { sd })
        }
    }

    /// `SECURITY_ATTRIBUTES` referencing the owned descriptor. The returned
    /// value borrows `self`; it must not outlive the creating call (callers
    /// pass it straight into the pipe create). Filling the struct copies a
    /// pointer but dereferences nothing, so this is safe.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.sd,
            bInheritHandle: 0,
        }
    }
}

/// Create one pipe server instance with the owner-only descriptor.
///
/// `first` selects `FILE_FLAG_FIRST_PIPE_INSTANCE`: the very first create
/// fails when another server already holds the name (exclusive bind);
/// later instances for concurrent clients omit it. Requires a Tokio
/// runtime with I/O enabled on the calling thread (provided by the
/// transport's `block_on`); a missing runtime panics inside Tokio, which is
/// a programming error, not memory unsafety.
pub fn create_server(
    pipe_name: &OsStr,
    first: bool,
    sec: &PipeSecurity,
) -> io::Result<NamedPipeServer> {
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first)
        .reject_remote_clients(true)
        .pipe_mode(PipeMode::Byte);
    let mut attrs = sec.attributes();
    // SAFETY: `attrs` points at a valid `SECURITY_ATTRIBUTES` whose
    // descriptor (`sec`) outlives this call; Tokio copies the descriptor
    // during creation. The raw-pointer handoff is the only unsafe act.
    unsafe {
        opts.create_with_security_attributes_raw(
            pipe_name,
            (&mut attrs as *mut SECURITY_ATTRIBUTES) as *mut core::ffi::c_void,
        )
    }
}

/// Block (kernel wait, no polling) until a pipe instance is available or
/// `timeout` passes. Used to bound client connects.
pub fn wait_for_pipe(pipe_name: &OsStr, timeout_ms: u32) -> io::Result<()> {
    let wide = wide_nul(pipe_name);
    // SAFETY: `wide` is NUL-terminated and lives for the call.
    unsafe {
        if WaitNamedPipeW(wide.as_ptr(), timeout_ms) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, OWNER_SECURITY_INFORMATION,
    };

    #[test]
    fn owner_sid_is_a_literal_account_sid() {
        let sid = current_user_sid().expect("process token must yield a user SID");
        // A concrete account SID (`S-1-5-21-...`), never the Owner Rights
        // pseudo-SID (`S-1-3-4`) the old SDDL relied on, and never SYSTEM.
        assert!(sid.starts_with("S-1-5-21-"), "unexpected owner SID: {sid}");
        assert_ne!(sid, "S-1-3-4");
        assert_ne!(sid, "S-1-5-18");
        assert!(sid.split('-').count() > 4);
    }

    /// `ACCESS_ALLOWED_ACE_TYPE` (`Win32::System::SystemServices`) without
    /// pulling that feature: the Win32 value is 0. Kept local so this
    /// module's `windows-sys` surface stays at five features.
    const ACCESS_ALLOW_TYPE: u8 = 0;

    #[test]
    fn pipe_security_is_owner_only_with_literal_sids() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime");
        let user = current_user_sid().expect("user SID");
        let name = format!(
            "\\\\.\\pipe\\tiler-live-fixture-sdtest-{}",
            std::process::id()
        );
        let server = rt.block_on(async {
            let sec = PipeSecurity::owner_only().expect("owner-only SD");
            create_server(OsStr::new(&name), true, &sec).expect("first instance")
        });
        // SAFETY: test-only Win32 reads below; all pointers are valid for
        // their call, heap outputs are freed, the pipe handle stays owned
        // by `server` (never closed here). `GetSecurityInfo` reports
        // success with `WIN32_ERROR` 0 (`ERROR_SUCCESS`), unlike the
        // `BOOL`-returning calls elsewhere in this file.
        unsafe {
            use std::os::windows::io::AsRawHandle;
            let handle: HANDLE = server.as_raw_handle();
            let mut owner: PSID = null_mut();
            let mut dacl: *mut ACL = null_mut();
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            assert_eq!(
                GetSecurityInfo(
                    handle,
                    SE_KERNEL_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    &mut owner,
                    null_mut(),
                    &mut dacl,
                    null_mut(),
                    &mut sd,
                ),
                0
            );
            assert!(!owner.is_null() && !dacl.is_null() && !sd.is_null());

            // Owner field is literally the current user.
            let mut text: *mut u16 = null_mut();
            assert_ne!(ConvertSidToStringSidW(owner, &mut text), 0);
            let mut got = Vec::new();
            let mut i = 0;
            loop {
                let w = *text.add(i);
                if w == 0 {
                    break;
                }
                got.push(w);
                i += 1;
            }
            LocalFree(text as *mut core::ffi::c_void);
            assert_eq!(String::from_utf16(&got).unwrap(), user);

            // DACL holds exactly two allow ACEs: user + SYSTEM, nothing else.
            // A typed `ACL_SIZE_INFORMATION` out-buffer: no hand parsing of
            // the header layout.
            let mut info = ACL_SIZE_INFORMATION {
                AceCount: 0,
                AclBytesInUse: 0,
                AclBytesFree: 0,
            };
            assert_ne!(
                GetAclInformation(
                    dacl.cast_const(),
                    &mut info as *mut ACL_SIZE_INFORMATION as *mut core::ffi::c_void,
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ),
                0
            );
            let ace_count = info.AceCount;
            assert_eq!(ace_count, 2, "owner-only DACL must hold user + SYSTEM");
            let mut sids = Vec::new();
            for index in 0..ace_count {
                let mut ace: *mut core::ffi::c_void = null_mut();
                assert_ne!(GetAce(dacl.cast_const(), index, &mut ace), 0);
                let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
                assert_eq!(ace.Header.AceType, ACCESS_ALLOW_TYPE);
                let sid_ptr = std::ptr::addr_of!(ace.SidStart) as PSID;
                let mut text: *mut u16 = null_mut();
                assert_ne!(ConvertSidToStringSidW(sid_ptr, &mut text), 0);
                let mut s = Vec::new();
                let mut i = 0;
                loop {
                    let w = *text.add(i);
                    if w == 0 {
                        break;
                    }
                    s.push(w);
                    i += 1;
                }
                LocalFree(text as *mut core::ffi::c_void);
                sids.push(String::from_utf16(&s).unwrap());
            }
            sids.sort();
            let mut expected = vec![user, "S-1-5-18".to_string()];
            expected.sort();
            assert_eq!(sids, expected);
            LocalFree(sd as *mut core::ffi::c_void);
        }
        drop(server);
    }

    #[test]
    fn flagged_first_instance_is_exclusive() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime");
        let name = format!(
            "\\\\.\\pipe\\tiler-live-fixture-first-{}-{}",
            std::process::id(),
            "exclusive"
        );
        rt.block_on(async {
            let sec = PipeSecurity::owner_only().expect("owner-only SD");
            let _first = create_server(OsStr::new(&name), true, &sec).expect("first instance");
            let err = create_server(OsStr::new(&name), true, &sec).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        });
    }
}
