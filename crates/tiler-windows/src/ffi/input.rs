//! Safe cursor, key-state, and input-prime reads.
//!
//! `INPUT` unions and out-pointers stay inside these functions.

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    Some((point.x, point.y))
}

pub fn async_key_down(vk: u32) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

/// MAKELPARAM packing for mouse-position messages: low word x, high word
/// y (narrowing truncation is the packing). Pure and unit-tested.
pub fn pack_mouse_lparam(x: i32, y: i32) -> isize {
    (((y as u32) << 16) | (x as u32 & 0xFFFF)) as usize as isize
}

/// Monotonic tick count in milliseconds (the trace-only native delivery
/// gap domain). No timing claim beyond the counter itself.
pub fn tick_count() -> u32 {
    unsafe { GetTickCount() }
}

/// One keyboard down/up pair through `SendInput`: the down plus its up
/// with no scancode masquerading. Returns the accepted-event count plus
/// whether the bounded partial-release ran (a down without its up gets
/// exactly one up release, nothing else repaired).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPairSend {
    pub inserted: u8,
    pub release_sent: bool,
}

pub fn send_key_pair(vk: u16) -> KeyPairSend {
    let inserted = unsafe {
        let mut pair: [INPUT; 2] = std::mem::zeroed();
        pair[0].r#type = INPUT_KEYBOARD;
        pair[0].Anonymous.ki = KEYBDINPUT {
            wVk: vk,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        pair[1].r#type = INPUT_KEYBOARD;
        pair[1].Anonymous.ki = KEYBDINPUT {
            wVk: vk,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        };
        let size = std::mem::size_of::<INPUT>() as i32;
        SendInput(2, pair.as_ptr(), size)
    };
    if inserted == 1 {
        let release = unsafe {
            let mut up: INPUT = std::mem::zeroed();
            up.r#type = INPUT_KEYBOARD;
            up.Anonymous.ki = KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            let size = std::mem::size_of::<INPUT>() as i32;
            SendInput(1, &up, size)
        };
        KeyPairSend {
            inserted: 1,
            release_sent: release == 1,
        }
    } else {
        KeyPairSend {
            inserted: inserted.min(u32::from(u8::MAX)) as u8,
            release_sent: false,
        }
    }
}

/// Timeout for the hit-test send, in milliseconds (matches the previous
/// inline call site).
pub const HITTEST_TIMEOUT_MS: u32 = 10;

/// Foreground-rights prime: one `vk`-down plus `vk`-up through `SendInput`
/// (the OS grants foreground rights to the last input provider), with the
/// bounded single-release when only the down inserts. Returns how many
/// inputs inserted. Runs on the owner loop thread, never in a callback,
/// with no wait and no policy effect.
pub fn prime_foreground_key(vk: u16) -> u8 {
    send_key_pair(vk).inserted
}
