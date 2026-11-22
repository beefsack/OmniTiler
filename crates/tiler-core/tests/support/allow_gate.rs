pub struct TestGate;

impl tiler_core::world_engine::CoreAdmissionGate for TestGate {
    fn check_request_size(
        &self,
        _report: &tiler_core::world_report::WorldReportV4,
    ) -> tiler_core::world_engine::AdmissionGateDecision {
        tiler_core::world_engine::AdmissionGateDecision::Allow
    }

    fn check_preflight(
        &self,
        _report: &tiler_core::world_report::WorldReportV4,
        _facts: &tiler_core::world_engine::PreflightGateFacts<'_>,
    ) -> tiler_core::world_engine::AdmissionGateDecision {
        tiler_core::world_engine::AdmissionGateDecision::Allow
    }

    fn check_exact_inner(
        &self,
        _reply: &tiler_core::world_report::WorldReplyV4,
    ) -> tiler_core::world_engine::ExactInnerDecision {
        tiler_core::world_engine::ExactInnerDecision::Allow(
            tiler_core::world_engine::ExactInnerCheck::new(0),
        )
    }

    fn check_future_reserve(
        &self,
        _reply: &tiler_core::world_report::WorldReplyV4,
        _facts: &tiler_core::world_engine::FutureReserveGateFacts,
        _exact: tiler_core::world_engine::ExactInnerCheck,
    ) -> tiler_core::world_engine::AdmissionGateDecision {
        tiler_core::world_engine::AdmissionGateDecision::Allow
    }
}
