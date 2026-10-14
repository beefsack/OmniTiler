#![forbid(unsafe_code)]
//! Portable tiling model over adapter-normalized integer geometry units.
//! Adapters convert host coordinates into these units and keep frame insets
//! outside the core; a core rectangle never encodes host-specific frame insets.

pub mod active_group;
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
pub mod size_hints;
pub mod visual;
pub mod workspace;
pub mod workspace_scope;
