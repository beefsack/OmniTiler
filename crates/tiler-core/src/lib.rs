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
pub mod directional;
pub mod engine;
pub mod geometry;
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
pub mod world_commands;
pub mod world_engine;
pub mod world_members;
pub mod world_model;
pub mod world_observe;
pub mod world_operations;
pub mod world_publish;
pub mod world_report;
pub mod world_sends;
pub mod world_settle;
pub mod world_sizing;
pub mod world_tiling_views;
