#![forbid(unsafe_code)]
// DescribePlan uses the fail-closed same-UID check in [`crate::planner_service`]
// via `GetConnectionUnixUser`; intent methods additionally require the current
// KWin owner. The tray trusts same-UID callers and authorizes snapshots only by
// sender unique name equal to the current `org.kde.KWin` owner.
pub(crate) mod float_intent_store;
pub mod planner_service;
pub mod tray;
pub mod tray_endpoint;
