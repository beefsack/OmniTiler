#![forbid(unsafe_code)]
// World and intent calls use the fail-closed same-UID check in
// [`crate::planner_service`] via `GetConnectionUnixUser`; world and intent
// methods additionally require the current KWin owner. The tray trusts
// same-UID callers and authorizes snapshots only by sender unique name
// equal to the current `org.kde.KWin` owner.
pub(crate) mod float_intent_store;
pub(crate) mod kde_gesture;
pub(crate) mod kde_raw;
pub mod planner_service;
pub mod tray;
pub mod tray_endpoint;
pub mod world_broker;
pub mod world_service;
