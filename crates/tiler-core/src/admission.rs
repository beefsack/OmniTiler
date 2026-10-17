//! Best-effort admission policy (D2/D3 of `best-effort-admission.md`).
//!
//! Two global settings with validated wire tokens. Anything else falls back
//! to the default at the adapter boundary. This module owns only the shared
//! values and their parse/default handling; anchor search, float/overlap
//! actuation and session lifecycle gating live elsewhere.

/// D2 anchor fallback when the ordinary anchor cannot split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorFallback {
    /// Search the domain for the largest splittable window (default).
    #[default]
    LargestSplittable,
    /// Search sibling subtrees nearest-first in topology order.
    ClosestRelative,
    /// Skip search; policy exhaustion feeds the no-space setting directly.
    NoSearch,
}

impl AnchorFallback {
    /// Wire token for this value.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::LargestSplittable => "largest-splittable-window",
            Self::ClosestRelative => "closest-splittable-relative",
            Self::NoSearch => "no-search",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers fall back to the default fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "largest-splittable-window" => Some(Self::LargestSplittable),
            "closest-splittable-relative" => Some(Self::ClosestRelative),
            "no-search" => Some(Self::NoSearch),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

/// D3 no-space result when no split is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NoSpaceResult {
    /// Float the window; tiling stays intact (default).
    #[default]
    Float,
    /// Keep tiled membership with positive overflowing geometry (D9).
    AllowOverlap,
}

impl NoSpaceResult {
    /// Wire token for this value.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::AllowOverlap => "allow-overlap",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers fall back to the default fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "float" => Some(Self::Float),
            "allow-overlap" => Some(Self::AllowOverlap),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_wire_roundtrips_with_largest_default() {
        assert_eq!(AnchorFallback::default(), AnchorFallback::LargestSplittable);
        assert_eq!(
            AnchorFallback::LargestSplittable.as_wire_str(),
            "largest-splittable-window"
        );
        assert_eq!(
            AnchorFallback::ClosestRelative.as_wire_str(),
            "closest-splittable-relative"
        );
        assert_eq!(AnchorFallback::NoSearch.as_wire_str(), "no-search");
        assert_eq!(
            AnchorFallback::parse_wire("largest-splittable-window"),
            Some(AnchorFallback::LargestSplittable)
        );
        assert_eq!(
            AnchorFallback::parse_wire("closest-splittable-relative"),
            Some(AnchorFallback::ClosestRelative)
        );
        assert_eq!(
            AnchorFallback::parse_wire("no-search"),
            Some(AnchorFallback::NoSearch)
        );
        for invalid in ["", "largest", "no_search", "largest-splittable-window "] {
            assert_eq!(AnchorFallback::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(
            AnchorFallback::normalize_wire(None),
            AnchorFallback::LargestSplittable
        );
        assert_eq!(
            AnchorFallback::normalize_wire(Some("bogus")),
            AnchorFallback::LargestSplittable
        );
    }

    #[test]
    fn no_space_wire_roundtrips_with_float_default() {
        assert_eq!(NoSpaceResult::default(), NoSpaceResult::Float);
        assert_eq!(NoSpaceResult::Float.as_wire_str(), "float");
        assert_eq!(NoSpaceResult::AllowOverlap.as_wire_str(), "allow-overlap");
        assert_eq!(
            NoSpaceResult::parse_wire("float"),
            Some(NoSpaceResult::Float)
        );
        assert_eq!(
            NoSpaceResult::parse_wire("allow-overlap"),
            Some(NoSpaceResult::AllowOverlap)
        );
        for invalid in ["", "overlap", "allow_overlap", "float "] {
            assert_eq!(NoSpaceResult::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(NoSpaceResult::normalize_wire(None), NoSpaceResult::Float);
        assert_eq!(
            NoSpaceResult::normalize_wire(Some("bogus")),
            NoSpaceResult::Float
        );
    }
}
