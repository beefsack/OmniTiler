//! R-MAX-06 born-maximized admission policy.
//!
//! One global setting with two validated values: `tile-on-launch` (default,
//! a tiled-first born maximized window clears once, then admits ordinarily)
//! and `keep-maximized-overlay` (the maximized frame is kept over a reserved
//! slot). The wire tokens are `tile-on-launch` and `keep-maximized-overlay`.
//! Anything else falls back to the default at the adapter boundary.
//!
//! This module owns only the shared values, their parse/default handling,
//! and the tile-vs-keep admission outcome for an ordinary eligible
//! tiled-first born-maximized admission. Host-specific eligibility gates
//! (held fullscreen exits, fixed-size skips, floating/sticky identity,
//! floating-first bindings, prior attempts, applied evidence, reload) stay
//! adapter-owned.

/// R-MAX-06 born-maximized admission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BornMaximized {
    /// Tile on launch (default): clear once, then admit ordinarily.
    #[default]
    TileOnLaunch,
    /// Keep the maximized frame over a reserved slot (no clear).
    KeepMaximizedOverlay,
}

impl BornMaximized {
    /// Wire token for this value (`tile-on-launch` / `keep-maximized-overlay`).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::TileOnLaunch => "tile-on-launch",
            Self::KeepMaximizedOverlay => "keep-maximized-overlay",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers fall back to the default fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "tile-on-launch" => Some(Self::TileOnLaunch),
            "keep-maximized-overlay" => Some(Self::KeepMaximizedOverlay),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the [`BornMaximized::TileOnLaunch`]
    /// default. Non-string JSON shapes (null, numbers, booleans) map to
    /// `None` at the fixture boundary before reaching this helper, so the
    /// core stays dependency-free.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }

    /// Admission outcome for an ordinary eligible tiled-first born-maximized
    /// window: true clears once under `tile-on-launch`, false keeps the
    /// overlay under `keep-maximized-overlay`.
    #[must_use]
    pub const fn should_clear_at_admission(self) -> bool {
        match self {
            Self::TileOnLaunch => true,
            Self::KeepMaximizedOverlay => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_roundtrips_with_tile_on_launch_default() {
        assert_eq!(BornMaximized::default(), BornMaximized::TileOnLaunch);
        assert_eq!(BornMaximized::TileOnLaunch.as_wire_str(), "tile-on-launch");
        assert_eq!(
            BornMaximized::KeepMaximizedOverlay.as_wire_str(),
            "keep-maximized-overlay"
        );
        assert_eq!(
            BornMaximized::parse_wire("tile-on-launch"),
            Some(BornMaximized::TileOnLaunch)
        );
        assert_eq!(
            BornMaximized::parse_wire("keep-maximized-overlay"),
            Some(BornMaximized::KeepMaximizedOverlay)
        );
        for invalid in [
            "",
            "tile",
            "COSMIC",
            "keep_maximized_overlay",
            "tile-on-launch ",
        ] {
            assert_eq!(BornMaximized::parse_wire(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn normalize_falls_back_to_tile_on_launch() {
        assert_eq!(
            BornMaximized::normalize_wire(None),
            BornMaximized::TileOnLaunch
        );
        assert_eq!(
            BornMaximized::normalize_wire(Some("tile-on-launch")),
            BornMaximized::TileOnLaunch
        );
        assert_eq!(
            BornMaximized::normalize_wire(Some("keep-maximized-overlay")),
            BornMaximized::KeepMaximizedOverlay
        );
        assert_eq!(
            BornMaximized::normalize_wire(Some("bogus")),
            BornMaximized::TileOnLaunch
        );
    }

    #[test]
    fn admission_policy_clears_only_under_tile_on_launch() {
        assert!(BornMaximized::TileOnLaunch.should_clear_at_admission());
        assert!(!BornMaximized::KeepMaximizedOverlay.should_clear_at_admission());
    }

    #[test]
    fn shared_fixture_table_conformance() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../test-fixtures/born-maximized.json"))
                .expect("shared born-maximized fixture parses");
        let cases = fixture
            .get("cases")
            .and_then(serde_json::Value::as_array)
            .expect("fixture carries a case table");
        assert!(!cases.is_empty(), "fixture case table is non-empty");
        for case in cases {
            let name = case
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unnamed>");
            // A missing `raw` key pins the absent setting; any non-string
            // JSON shape (null, number, boolean) normalizes as missing.
            let raw = case.get("raw").and_then(serde_json::Value::as_str);
            let mode = BornMaximized::normalize_wire(raw);
            let normalized = case
                .get("normalized")
                .and_then(serde_json::Value::as_str)
                .expect("each case pins its normalized token");
            assert_eq!(mode.as_wire_str(), normalized, "{name}");
            let should_clear = case
                .get("should_clear")
                .and_then(serde_json::Value::as_bool)
                .expect("each case pins its clear-vs-keep outcome");
            assert_eq!(mode.should_clear_at_admission(), should_clear, "{name}");
        }
    }
}
