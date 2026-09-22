//! Shared constants and configuration values for the Riff workspace.
//!
//! This crate is the single source of truth for tunable values referenced
//! across crates. Constants live in one module per layer they configure, so
//! which subsystem owns a value is obvious and unrelated tunables do not
//! collect into a single flat list.
//!
//! Values that a user can change belong in GSettings
//! (`data/dev.diegovsky.Riff.gschema.xml`), not here. This crate is for
//! constants we tune during development.

pub mod api;
