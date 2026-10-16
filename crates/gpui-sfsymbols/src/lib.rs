//! # gpui-sfsymbols
//!
//! SF Symbols rendering for GPUI applications on macOS.
//!
//! **Platform support: macOS only.** SF Symbols are an Apple platform API and
//! are rendered through AppKit. This crate intentionally fails to compile on
//! any other target.
//!
//! This crate provides two levels of API:
//!
//! 1. **`SfSymbol`** - Low-level builder for rendering SF Symbols to raw RGBA pixels
//!    or GPUI `RenderImage`.
//!
//! 2. **`Icon`** (requires `component` feature) - High-level GPUI component that can
//!    be used directly in views, similar to GPUI Components.
//!
//! ## Features
//!
//! - `gpui` - Enable GPUI integration (converts to `RenderImage`)
//! - `component` - Enable high-level `Icon` component (implies `gpui`)
//!
//! ## Example: Using SfSymbol
//!
//! ```rust,ignore
//! use gpui_sfsymbols::SfSymbol;
//!
//! // Render to raw RGBA pixels
//! let (width, height, data) = SfSymbol::new("star.fill")
//!     .size(32.0)
//!     .color(0x000000)
//!     .render_rgba()
//!     .unwrap();
//!
//! // With GPUI feature enabled:
//! let image = SfSymbol::new("star.fill").render().unwrap();
//! ```
//!
//! ## Example: Using Icon Component
//!
//! ```rust,ignore
//! use gpui_sfsymbols::{Icon, define_icons};
//! use gpui::px;
//!
//! // Define your own icon enum
//! define_icons! {
//!     pub enum AppIcon {
//!         Star => "star.fill",
//!         Heart => "heart.fill",
//!         Settings => "gearshape.fill",
//!     }
//! }
//!
//! // Use in a view
//! fn view(window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div()
//!         .child(Icon::new("star.fill"))
//!         .child(Icon::from_name(AppIcon::Heart).color(gpui::rgb(0xFF0000)))
//! }
//! ```

#[cfg(not(target_os = "macos"))]
compile_error!(
    "gpui-sfsymbols only supports macOS: SF Symbols are an Apple platform API \
     rendered through AppKit and cannot be provided on this target."
);

mod color;
mod effect;
mod platform;
mod symbol;

#[cfg(feature = "component")]
mod icon;

// Re-export main types
pub use color::SymbolColor;
pub use effect::{Animate, Direction, Repeat, SymbolEffect, SymbolFrame};
pub use symbol::{RenderingMode, SfSymbol, SymbolScale, SymbolWeight};

/// Whether this macOS can draw system symbols (macOS 11 and later).
pub fn is_available() -> bool {
    platform::supports_system_symbols()
}

// Re-export cache functions
#[cfg(feature = "cache")]
pub use symbol::{cache_size, clear_cache};

#[cfg(feature = "component")]
pub use icon::{Icon, IconName};

// Re-export sfsymbols presets
#[cfg(feature = "presets")]
pub use sfsymbols;
