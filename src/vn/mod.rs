//! Visual-novel core: script parsing, the playhead state machine,
//! character registry, and saves.
//!
//! This module is pure Rust — no bevy, no I/O beyond saves — so the whole
//! story engine is unit-testable anywhere. The bevy presentation layer lives
//! in [`crate::game`] behind the `gui` feature.

pub mod cast;
pub mod save;
pub mod script;
pub mod state;
