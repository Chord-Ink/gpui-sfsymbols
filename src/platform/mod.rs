//! Platform-specific implementations for SF Symbol rendering.
//!
//! This crate is macOS-only (see the `compile_error!` in `lib.rs`); SF Symbols
//! are an Apple platform API rendered through AppKit.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::render_sf_symbol;
