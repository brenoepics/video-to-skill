//! Library surface of the video-to-skill extractor.
//!
//! The binary in `main.rs` is a thin CLI over this crate.

pub mod asr;
pub mod bench;
pub mod bundle;
pub mod compile;
pub mod deps;
pub mod frames;
pub mod guardrails;
pub mod hashing;
pub mod merge;
pub mod rewatch;
pub mod timeline;
pub mod tools;
pub mod verify;
