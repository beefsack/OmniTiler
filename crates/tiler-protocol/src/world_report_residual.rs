//! Protocol-owned residual evidence for [`DecodedReportV4`](crate::world_protocol::DecodedReportV4).
//!
//! Retains canonical raw fields that core held rows omit (currently the
//! singleton `HeldPresentationRow` raw carried fields) for lossless wire
//! round-tripping, exact bounded request estimation, and exact replay
//! discrimination at checked ingress. Direct unchecked typed core entries that
//! never had a wire shape have `None` residual and pay zero heap allocation.

use tiler_core::connection::RawSettingValue;
use tiler_core::world_engine::ReplayResidualToken;

use crate::world_admission_rows::held_fields_estimate;

/// Canonical ordered raw fields retained for one held row.
pub type HeldFields = Vec<(String, RawSettingValue)>;

/// Canonical protocol-owned residual evidence carried alongside a decoded v4
/// report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldReportResidual {
    /// Canonical ordered raw fields of a held `presentation` singleton object,
    /// when `report.presentation` is `PresentationRowV4::Held(_)`.
    pub held_presentation_fields: Option<HeldFields>,
}

impl WorldReportResidual {
    /// Whether this residual carries no retained wire evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held_presentation_fields.is_none()
    }

    /// Exact canonical byte estimate of retained residual fields for
    /// [`report_estimate_v4_with_residual`](crate::world_admission_sizing::report_estimate_v4_with_residual).
    #[must_use]
    pub(crate) fn estimate_bytes(&self) -> usize {
        match &self.held_presentation_fields {
            Some(fields) => held_fields_estimate(fields),
            None => 0,
        }
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
