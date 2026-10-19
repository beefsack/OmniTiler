//! Experimental private `ApplicationView` cloak (test helper only).
//!
//! Native calls stay in this `ffi` submodule. Every HRESULT failure fails
//! closed with no retries and no layout fallbacks. Experimental ABI is not
//! validated: `QueryService`/IID checks are not proof of layout.

use windows::Win32::Foundation::HWND as WinHw;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    IServiceProvider,
};
use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};
use windows::core::{GUID, HRESULT, Interface};

/// Hard-coded experimental fence (mirrors `crate::cloak::EXPERIMENTAL_BUILD`).
pub const EXPECTED_BUILD: u32 = crate::cloak::EXPERIMENTAL_BUILD;
/// ImmersiveShell service CLSID (pinned Glaze/komorebi agreement).
pub const CLSID_IMMERSIVE_SHELL: GUID = GUID::from_u128(0xC2F0_3A33_21F5_47FA_B4BB_1563_62A2_F239);

/// Pinned collection IID `1841c6d7-4f9d-42c0-af41-8747538f10e5`.
#[windows::core::interface("1841c6d7-4f9d-42c0-af41-8747538f10e5")]
pub unsafe trait IApplicationViewCollection: windows::core::IUnknown {
    pub unsafe fn m1(&self);
    pub unsafe fn m2(&self);
    pub unsafe fn m3(&self);
    pub unsafe fn get_view_for_hwnd(
        &self,
        window: isize,
        application_view: *mut Option<IApplicationView>,
    ) -> HRESULT;
}

/// Pinned view IID `372e1d3b-38d3-42e4-a15b-8ab2b178f513`; first three
/// post-`IUnknown` slots are `IInspectable`-shaped, `SetCloak` is slot 12.
#[windows::core::interface("372E1D3B-38D3-42E4-A15B-8AB2B178F513")]
pub unsafe trait IApplicationView: windows::core::IUnknown {
    pub unsafe fn m1(&self);
    pub unsafe fn m2(&self);
    pub unsafe fn m3(&self);
    pub unsafe fn m4(&self);
    pub unsafe fn m5(&self);
    pub unsafe fn m6(&self);
    pub unsafe fn m7(&self);
    pub unsafe fn m8(&self);
    pub unsafe fn m9(&self);
    pub unsafe fn set_cloak(&self, cloak_type: u32, cloak_flag: i32) -> HRESULT;
}

/// Closed native failure vocabulary. Every display line carries the
/// experimental-ABI disclaimer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloakError {
    UnsupportedArch,
    UnsupportedBuild { found: Option<u32> },
    ComInit(i32),
    Service(i32),
    NoView,
    ViewHresult { op: &'static str, code: i32 },
    Taskbar { op: &'static str, code: i32 },
}

impl CloakError {
    fn disclaimer() -> &'static str {
        "experimental ABI not validated; QueryService/IID checks are not proof of layout"
    }
}

impl std::fmt::Display for CloakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Raw messages only: the safe caller adds the `refuse:` / `error:`
        // prefix via its own fail-closed error type.
        match self {
            Self::UnsupportedArch => write!(
                f,
                "cloak spike needs Windows x64 ({}); {}",
                std::env::consts::ARCH,
                Self::disclaimer()
            ),
            Self::UnsupportedBuild { found } => match found {
                Some(b) => write!(
                    f,
                    "cloak spike needs build {EXPECTED_BUILD}, found {b}; {}",
                    Self::disclaimer()
                ),
                None => write!(
                    f,
                    "cloak spike needs build {EXPECTED_BUILD}, build unreadable; {}",
                    Self::disclaimer()
                ),
            },
            Self::ComInit(code) => write!(f, "ComInit failed code={code}; {}", Self::disclaimer()),
            Self::Service(code) => write!(
                f,
                "shell service query failed code={code}; {}",
                Self::disclaimer()
            ),
            Self::NoView => write!(f, "no application view for hwnd; {}", Self::disclaimer()),
            Self::ViewHresult { op, code } => {
                write!(f, "{op} failed code={code}; {}", Self::disclaimer())
            }
            Self::Taskbar { op, code } => {
                write!(f, "taskbar {op} failed code={code}; {}", Self::disclaimer())
            }
        }
    }
}

#[repr(C)]
struct RtlVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    csd: [u16; 128],
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut RtlVersionInfo) -> i32;
}

/// Read-only OS build number via `RtlGetVersion`; `None` when unreadable.
pub fn os_build_number() -> Option<u32> {
    let mut info = RtlVersionInfo {
        size: std::mem::size_of::<RtlVersionInfo>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd: [0; 128],
    };
    let status = unsafe { RtlGetVersion(&mut info) };
    if status == 0 && info.size != 0 {
        Some(info.build)
    } else {
        None
    }
}

/// Hard-coded experimental gate: x64 + build 26300, before any private call.
pub fn check_experimental_gate() -> Result<u32, CloakError> {
    if cfg!(not(target_arch = "x86_64")) {
        return Err(CloakError::UnsupportedArch);
    }
    match os_build_number() {
        Some(b) if b == EXPECTED_BUILD => Ok(b),
        other => Err(CloakError::UnsupportedBuild { found: other }),
    }
}

const DWMWA_CLOAKED: u32 = 14;

/// Numeric `DWMWA_CLOAKED` mask; `None` when the query fails (fail closed).
pub fn cloak_mask_of(hwnd_u64: u64) -> Option<u32> {
    let mut value: i32 = 0;
    let ok = unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd_u64 as isize as windows_sys::Win32::Foundation::HWND,
            DWMWA_CLOAKED,
            (&mut value as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } == 0;
    if ok { Some(value as u32) } else { None }
}

fn with_apartment<T>(f: impl FnOnce() -> Result<T, CloakError>) -> Result<T, CloakError> {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.is_err() {
        return Err(CloakError::ComInit(hr.0));
    }
    let out = f();
    unsafe {
        CoUninitialize();
    }
    out
}

fn set_cloak_raw(hwnd_u64: u64, cloak_type: u32, flag: i32) -> Result<(), CloakError> {
    check_experimental_gate()?;
    // One apartment spans service query, view lookup, and the cloak call:
    // the view must not outlive its apartment.
    with_apartment(|| {
        let provider: IServiceProvider = unsafe {
            CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_ALL)
                .map_err(|e| CloakError::Service(e.code().0))?
        };
        let collection: IApplicationViewCollection = unsafe {
            provider
                .QueryService(&IApplicationViewCollection::IID)
                .map_err(|e| CloakError::Service(e.code().0))?
        };
        let mut view: Option<IApplicationView> = None;
        let hr = unsafe { collection.get_view_for_hwnd(hwnd_u64 as isize, &mut view) };
        if hr.is_err() {
            return Err(CloakError::ViewHresult {
                op: "GetViewForHwnd",
                code: hr.0,
            });
        }
        let Some(view) = view else {
            return Err(CloakError::NoView);
        };
        let hr = unsafe { view.set_cloak(cloak_type, flag) };
        if hr.is_err() {
            return Err(CloakError::ViewHresult {
                op: "SetCloak",
                code: hr.0,
            });
        }
        Ok(())
    })
}

/// Private `SetCloak(1, 2)` on one HWND. Gate first in the caller.
pub fn cloak_hwnd(hwnd_u64: u64) -> Result<(), CloakError> {
    set_cloak_raw(hwnd_u64, crate::cloak::CLOAK_TYPE, crate::cloak::CLOAK_FLAG)
}

/// Private `SetCloak(1, 0)` on one HWND. Gate first in the caller.
pub fn uncloak_hwnd(hwnd_u64: u64) -> Result<(), CloakError> {
    set_cloak_raw(
        hwnd_u64,
        crate::cloak::CLOAK_TYPE,
        crate::cloak::UNCLOAK_FLAG,
    )
}

fn with_taskbar(f: impl FnOnce(&ITaskbarList) -> Result<(), CloakError>) -> Result<(), CloakError> {
    with_apartment(|| {
        let list: ITaskbarList = unsafe {
            CoCreateInstance(&TaskbarList, None, CLSCTX_ALL).map_err(|e| CloakError::Taskbar {
                op: "create",
                code: e.code().0,
            })?
        };
        unsafe {
            list.HrInit().map_err(|e| CloakError::Taskbar {
                op: "HrInit",
                code: e.code().0,
            })?;
        }
        f(&list)
    })
}

/// `ITaskbarList::DeleteTab` pairing for the `--delete-tab` variant.
pub fn taskbar_delete_tab(hwnd_u64: u64) -> Result<(), CloakError> {
    with_taskbar(|list| {
        let hwnd = WinHw(hwnd_u64 as usize as *mut core::ffi::c_void);
        unsafe {
            list.DeleteTab(hwnd).map_err(|e| CloakError::Taskbar {
                op: "DeleteTab",
                code: e.code().0,
            })?;
        }
        Ok(())
    })
}

/// `ITaskbarList::AddTab` restoration for the `--added-tab` recovery.
pub fn taskbar_add_tab(hwnd_u64: u64) -> Result<(), CloakError> {
    with_taskbar(|list| {
        let hwnd = WinHw(hwnd_u64 as usize as *mut core::ffi::c_void);
        unsafe {
            list.AddTab(hwnd).map_err(|e| CloakError::Taskbar {
                op: "AddTab",
                code: e.code().0,
            })?;
        }
        Ok(())
    })
}
