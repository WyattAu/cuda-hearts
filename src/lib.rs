//! CUDA HEARTS — a leather-jacket love simulator (unofficial parody).
//!
//! Library root: the pure story engine ([`vn`]) is always built; the bevy
//! presentation layer ([`game`]) is gated behind the `gui` feature.

pub mod vn;

#[cfg(feature = "gui")]
pub mod game;
