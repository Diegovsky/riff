//! User and app configuration for the Riff workspace.
//!
//! This crate is the single source of truth for configuration referenced
//! across crates: app constants we tune during development, and the code that
//! reads and exposes user settings stored in GSettings
//! (`data/dev.diegovsky.Riff.gschema.xml`). Each module covers one area, so
//! which subsystem owns a value is obvious and unrelated configuration does
//! not collect into a single flat list.

pub mod api;
pub mod feature_flags;
pub mod settings;
