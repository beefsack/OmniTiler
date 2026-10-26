#![forbid(unsafe_code)]
//! Portable tiling model over adapter-normalized integer geometry units.
//! Adapters convert host coordinates into these units and keep frame insets
//! outside the core; a core rectangle never encodes host-specific frame insets.

pub mod active_group;
pub mod admission;
pub mod born_maximized;
pub mod boundary;
pub mod bounds;
pub mod contract;
pub mod cosmic_v1;
pub mod cursor_targeting;
pub mod directional;
pub(crate) mod engine;
pub mod fresh_focus;
pub mod geometry;
pub mod gesture;
pub mod ids;
pub mod output_selection;
pub mod policy;
pub mod reconcile;
pub mod seed;
pub mod session;
pub mod settings_catalog;
pub mod size_hints;
pub mod visual;
pub mod window_policy;
pub mod workspace;
pub mod workspace_scope;
pub mod world_born;
pub mod world_clear;
pub mod world_commands;
pub mod world_cursor;
pub mod world_directional;
pub mod world_engine;
mod world_frames;
mod world_gesture;
pub mod world_members;
pub mod world_model;
pub mod world_native;
pub mod world_observe;
pub mod world_operations;
pub mod world_presentation;
pub mod world_publish;
pub mod world_report;
pub mod world_sends;
pub mod world_settle;
pub mod world_sizing;
pub mod world_tiling_views;

#[cfg(test)]
mod capacity_admission_tests;
#[cfg(test)]
mod fixed_size_admission_tests;
#[cfg(test)]
mod focused_fullscreen_presentation_tests;
#[cfg(test)]
mod gap_repair_replay_tests;
#[cfg(test)]
mod maximized_focus_send_tests;
#[cfg(test)]
mod minimize_restore_tests;
#[cfg(test)]
mod observation_send_r4_tests;
#[cfg(test)]
mod per_domain_drag_preview_tests;
#[cfg(test)]
pub(crate) mod per_domain_fixture;
#[cfg(test)]
mod per_domain_reconcile_tests;
#[cfg(test)]
mod per_domain_resize_tests;
#[cfg(test)]
mod release_domain_tests;
#[cfg(test)]
mod restart_disjoint_adoption_tests;
#[cfg(test)]
mod session_float_focus_admission_tests;
#[cfg(test)]
mod session_observation_convergence_tests;
#[cfg(test)]
mod size_hints_tests;
#[cfg(test)]
mod world_admission_close_stay_tests;
#[cfg(test)]
mod world_born_tests;
#[cfg(test)]
mod world_capacity_diagnostic_tests;
#[cfg(test)]
mod world_clear_tests;
#[cfg(test)]
mod world_coverage_gaps_tests;
#[cfg(test)]
mod world_cursor_tests;
#[cfg(test)]
mod world_float_boundary_tests;
#[cfg(test)]
mod world_float_membership_tests;
#[cfg(test)]
mod world_frame_fixture_tests;
#[cfg(test)]
mod world_frame_policy_tests;
#[cfg(test)]
mod world_gesture_tests;
#[cfg(test)]
mod world_global_swap_tests;
#[cfg(test)]
mod world_logical_contract_tests;
#[cfg(test)]
mod world_migration_refusal_tests;
#[cfg(test)]
mod world_no_focus_tests;
#[cfg(test)]
mod world_pager_presentation_tests;
#[cfg(test)]
mod world_restored_routing_tests;
#[cfg(test)]
mod world_sessionless_migration_tests;
#[cfg(test)]
mod world_toggle_tests;
