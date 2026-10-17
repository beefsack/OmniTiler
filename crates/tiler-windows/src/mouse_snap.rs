#![forbid(unsafe_code)]
//! Session-only mouse-Snap (`SPI_SETWINARRANGING`) prevention.
//!
//! Portable preimage/restore decisions live in [`crate::model`]; the native
//! getter/setter below is `cfg(windows)` only. The setting is desktop-wide and
//! session-only: the setter omits `SPIF_UPDATEINIFILE` (no profile write, no
//! registry/policy) and passes `SPIF_SENDCHANGE` so running apps pick up the
//! live value. There is no per-monitor variant; a monitor-scoped illusion is
//! disallowed. Every setter pairs with an exact `SPI_GETWINARRANGING`
//! readback: API success alone never counts as an effect.

#[cfg(windows)]
pub mod sys {
    type DynError = Box<dyn std::error::Error>;
    type Result<T> = std::result::Result<T, DynError>;

    fn err(msg: impl Into<String>) -> DynError {
        Box::new(std::io::Error::other(msg.into()))
    }

    /// Exact live `SPI_GETWINARRANGING` boolean. Fails closed on API error.
    pub fn get() -> Result<bool> {
        crate::ffi::get_win_arranging().ok_or_else(|| err("error: SPI_GETWINARRANGING read failed"))
    }

    /// Set the live value session-only (no `SPIF_UPDATEINIFILE`) and verify
    /// with an exact readback. Returns the readback value so the caller can
    /// compare it against the desired effect instead of trusting the setter.
    ///
    /// The `BOOL` travels in `uiParam` with `pvParam` NULL (the sibling
    /// `SPI_SETBEEP`/`SPI_SETSHOWSOUNDS` convention). Microsoft's
    /// `SPI_SETWINARRANGING` row says "set pvParam to TRUE/FALSE", but live
    /// evidence on Windows 11 build 26200 (2026-10-02) shows the pvParam-value
    /// encoding cannot restore `TRUE`: the setter reports success yet the
    /// `SPI_GETWINARRANGING` readback stays `FALSE` (owner teardown
    /// `mismatch`, independent restore `mismatch`), while `uiParam = 1` with
    /// `pvParam` NULL restores `TRUE` with a 3x readback and PEN 35 intact.
    pub fn set_verified(enabled: bool) -> Result<bool> {
        if !crate::ffi::set_win_arranging_session(enabled) {
            return Err(err("error: SPI_SETWINARRANGING write failed"));
        }
        get()
    }

    #[cfg(test)]
    mod spi_constant_tests {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETPENVISUALIZATION, SPI_SETPENVISUALIZATION,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETWINARRANGING, SPI_SETWINARRANGING, SPIF_SENDCHANGE,
        };

        #[test]
        fn sdk_winarranging_numerics_match_documented_values() {
            // Microsoft SystemParametersInfoW docs: SPI_GETWINARRANGING 0x0082,
            // SPI_SETWINARRANGING 0x0083 (window params); PENVISUALIZATION GET
            // 0x201E / SET 0x201F is a different pen target. Local doc cache
            // tool_0f7edd44b001jEAY4xyCyhAzEV lines 374/398 vs 235/263.
            assert_eq!(SPI_GETWINARRANGING, 0x0082);
            assert_eq!(SPI_SETWINARRANGING, 0x0083);
            assert_eq!(SPIF_SENDCHANGE, 0x02);
            assert_eq!(SPI_GETPENVISUALIZATION, 0x201E);
            assert_eq!(SPI_SETPENVISUALIZATION, 0x201F);
            assert_ne!(SPI_GETWINARRANGING, SPI_GETPENVISUALIZATION);
            assert_ne!(SPI_SETWINARRANGING, SPI_SETPENVISUALIZATION);
        }
    }
}
