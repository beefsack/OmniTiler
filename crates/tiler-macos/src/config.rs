#![forbid(unsafe_code)]

//! Probe configuration: Ctrl-role modifier plus finite bounds.
//!
//! "Ctrl configurable default Ctrl": the chord filed under `Ctrl` uses a
//! configurable modifier that defaults to Ctrl (pass `--ctrl meta` for the
//! Cmd chord space).

use serde::{Deserialize, Serialize};

/// Finite bounds. Every live mode must fit inside these.
pub const MAX_TAP_SECONDS: u64 = 30;
pub const DEFAULT_TAP_SECONDS: u64 = 10;
pub const MAX_AX_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_AX_TIMEOUT_MS: u64 = 1_000;
pub const MAX_WINDOWS: usize = 256;
pub const DEFAULT_MAX_WINDOWS: usize = 256;
pub const MAX_PROBE_SECONDS: u64 = 120;
pub const DEFAULT_PROBE_SECONDS: u64 = 30;

/// Modifier filling the Ctrl role in a probe chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProbeModifier {
    /// Command (Meta). Opt-in via `--ctrl meta`.
    Meta,
    /// Control. Default.
    #[default]
    Ctrl,
    Alt,
    Shift,
}

impl ProbeModifier {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "meta" | "cmd" | "command" => Ok(Self::Meta),
            "ctrl" | "control" => Ok(Self::Ctrl),
            "alt" | "opt" | "option" => Ok(Self::Alt),
            "shift" => Ok(Self::Shift),
            other => Err(format!(
                "unknown modifier '{other}' (expected meta|ctrl|alt|shift)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeConfig {
    pub modifier: ProbeModifier,
    pub tap_seconds: u64,
    pub ax_timeout_ms: u64,
    pub max_windows: usize,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            modifier: ProbeModifier::Ctrl,
            tap_seconds: DEFAULT_TAP_SECONDS,
            ax_timeout_ms: DEFAULT_AX_TIMEOUT_MS,
            max_windows: DEFAULT_MAX_WINDOWS,
        }
    }
}

impl ProbeConfig {
    /// Validate finite bounds. Fails closed on any overflow/zero.
    pub fn validate(&self) -> Result<(), String> {
        if self.tap_seconds == 0 || self.tap_seconds > MAX_TAP_SECONDS {
            return Err(format!(
                "tap_seconds must be 1..={MAX_TAP_SECONDS}, got {}",
                self.tap_seconds
            ));
        }
        if self.ax_timeout_ms == 0 || self.ax_timeout_ms > MAX_AX_TIMEOUT_MS {
            return Err(format!(
                "ax_timeout_ms must be 1..={MAX_AX_TIMEOUT_MS}, got {}",
                self.ax_timeout_ms
            ));
        }
        if self.max_windows == 0 || self.max_windows > MAX_WINDOWS {
            return Err(format!(
                "max_windows must be 1..={MAX_WINDOWS}, got {}",
                self.max_windows
            ));
        }
        Ok(())
    }

    pub fn validate_probe_seconds(seconds: u64) -> Result<u64, String> {
        if seconds == 0 || seconds > MAX_PROBE_SECONDS {
            return Err(format!(
                "seconds must be 1..={MAX_PROBE_SECONDS}, got {seconds}"
            ));
        }
        Ok(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_modifier_is_ctrl() {
        assert_eq!(ProbeConfig::default().modifier, ProbeModifier::Ctrl);
        assert_eq!(ProbeModifier::parse("CTRL").unwrap(), ProbeModifier::Ctrl);
    }

    #[test]
    fn validate_rejects_unbounded() {
        let mut cfg = ProbeConfig::default();
        cfg.validate().unwrap();
        cfg.tap_seconds = 0;
        assert!(cfg.validate().is_err());
        cfg.tap_seconds = MAX_TAP_SECONDS + 1;
        assert!(cfg.validate().is_err());
        cfg.tap_seconds = DEFAULT_TAP_SECONDS;
        cfg.ax_timeout_ms = MAX_AX_TIMEOUT_MS + 1;
        assert!(cfg.validate().is_err());
        cfg.ax_timeout_ms = DEFAULT_AX_TIMEOUT_MS;
        cfg.max_windows = MAX_WINDOWS + 1;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn probe_seconds_bounded() {
        assert!(ProbeConfig::validate_probe_seconds(0).is_err());
        assert!(ProbeConfig::validate_probe_seconds(MAX_PROBE_SECONDS).is_ok());
        assert!(ProbeConfig::validate_probe_seconds(MAX_PROBE_SECONDS + 1).is_err());
    }
}
