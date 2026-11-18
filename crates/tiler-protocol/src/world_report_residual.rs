//! Protocol-owned residual evidence for [`DecodedReportV4`](crate::world_protocol::DecodedReportV4).
//!
//! Retains canonical raw fields that core held rows omit (held
//! `HeldScreenRow` entries, held `HeldWindowRow` entries, held
//! `HeldNativeRow` entries, held `HeldAttemptRow` entries, and the singleton
//! `HeldPresentationRow` raw carried fields) for lossless wire round-tripping,
//! exact bounded request estimation, and exact replay discrimination at checked
//! ingress. Direct unchecked typed core entries that never had a wire shape
//! have `None` residual and pay zero heap allocation.

use tiler_core::connection::RawSettingValue;
use tiler_core::world_engine::ReplayResidualToken;
use tiler_core::world_report::AttemptLaneV4;

use crate::world_admission_rows::held_fields_estimate;

/// Canonical ordered raw fields retained for one held row.
pub type HeldFields = Vec<(String, RawSettingValue)>;

/// Canonical protocol-owned residual evidence carried alongside a decoded v4
/// report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldReportResidual {
    /// Canonical ordered raw fields for held `screens` rows, in the same
    /// logical order as `report.held_screens` (sorted by `ordinal`), each entry
    /// paired with its carried `ordinal`.
    pub held_screen_fields: Vec<(usize, HeldFields)>,
    /// Canonical ordered raw fields for held `windows` rows, in the same
    /// logical order as `report.held_windows` (sorted by `ordinal`), each entry
    /// paired with its carried `ordinal`.
    pub held_window_fields: Vec<(usize, HeldFields)>,
    /// Canonical ordered raw fields for held `native` rows, in the same
    /// logical order as `report.held_native` (sorted by `ordinal`), each entry
    /// paired with its carried `ordinal`.
    pub held_native_fields: Vec<(usize, HeldFields)>,
    /// Canonical ordered raw fields for held attempt rows, ordered by
    /// `(lane, ordinal)`, each entry paired with its carried `(lane, ordinal)`.
    pub held_attempt_fields: Vec<(AttemptLaneV4, usize, HeldFields)>,
    /// Canonical ordered raw fields of a held `presentation` singleton object,
    /// when `report.presentation` is `PresentationRowV4::Held(_)`.
    pub held_presentation_fields: Option<HeldFields>,
}

impl WorldReportResidual {
    /// Whether this residual carries no retained wire evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held_screen_fields.is_empty()
            && self.held_window_fields.is_empty()
            && self.held_native_fields.is_empty()
            && self.held_attempt_fields.is_empty()
            && self.held_presentation_fields.is_none()
    }

    /// Look up retained raw fields for one held screen ordinal in sorted
    /// `(ordinal, fields)` entries via binary search.
    #[must_use]
    pub fn screen_fields(&self, ordinal: usize) -> Option<&[(String, RawSettingValue)]> {
        let idx = self
            .held_screen_fields
            .binary_search_by_key(&ordinal, |(ord, _)| *ord)
            .ok()?;
        Some(self.held_screen_fields[idx].1.as_slice())
    }

    /// Look up retained raw fields for one held window ordinal in sorted
    /// `(ordinal, fields)` entries via binary search.
    #[must_use]
    pub fn window_fields(&self, ordinal: usize) -> Option<&[(String, RawSettingValue)]> {
        let idx = self
            .held_window_fields
            .binary_search_by_key(&ordinal, |(ord, _)| *ord)
            .ok()?;
        Some(self.held_window_fields[idx].1.as_slice())
    }

    /// Look up retained raw fields for one held native ordinal in sorted
    /// `(ordinal, fields)` entries via binary search.
    #[must_use]
    pub fn native_fields(&self, ordinal: usize) -> Option<&[(String, RawSettingValue)]> {
        let idx = self
            .held_native_fields
            .binary_search_by_key(&ordinal, |(ord, _)| *ord)
            .ok()?;
        Some(self.held_native_fields[idx].1.as_slice())
    }

    /// Look up retained raw fields for one held attempt row in sorted
    /// `(lane, ordinal, fields)` entries via binary search.
    #[must_use]
    pub fn attempt_fields(
        &self,
        lane: AttemptLaneV4,
        ordinal: usize,
    ) -> Option<&[(String, RawSettingValue)]> {
        let idx = self
            .held_attempt_fields
            .binary_search_by_key(&(lane, ordinal), |(row_lane, ord, _)| (*row_lane, *ord))
            .ok()?;
        Some(self.held_attempt_fields[idx].2.as_slice())
    }

    /// Exact canonical byte estimate of retained residual fields for
    /// [`report_estimate_v4_with_residual`](crate::world_admission_sizing::report_estimate_v4_with_residual).
    #[must_use]
    pub(crate) fn estimate_bytes(&self) -> usize {
        let screen_bytes: usize = self
            .held_screen_fields
            .iter()
            .map(|(_, fields)| held_fields_estimate(fields))
            .sum();
        let window_bytes: usize = self
            .held_window_fields
            .iter()
            .map(|(_, fields)| held_fields_estimate(fields))
            .sum();
        let native_bytes: usize = self
            .held_native_fields
            .iter()
            .map(|(_, fields)| held_fields_estimate(fields))
            .sum();
        let attempt_bytes: usize = self
            .held_attempt_fields
            .iter()
            .map(|(_, _, fields)| held_fields_estimate(fields))
            .sum();
        let presentation_bytes = match &self.held_presentation_fields {
            Some(fields) => held_fields_estimate(fields),
            None => 0,
        };
        screen_bytes
            .saturating_add(window_bytes)
            .saturating_add(native_bytes)
            .saturating_add(attempt_bytes)
            .saturating_add(presentation_bytes)
    }
}
impl ReplayResidualToken for WorldReportResidual {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn residual_eq(&self, other: &dyn ReplayResidualToken) -> bool {
        other
            .as_any()
            .downcast_ref::<WorldReportResidual>()
            .is_some_and(|rhs| self == rhs)
    }
}
