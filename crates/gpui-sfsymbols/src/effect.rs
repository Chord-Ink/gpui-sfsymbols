//! Symbol animation effects, mirroring Apple's `NSSymbolEffect` family.
//!
//! Two implementation strategies exist:
//!
//! - **Bitmap-sampled** ([`SymbolEffect::VariableColor`],
//!   [`SymbolEffect::Rotate`]): the core layer renders a sequence of bitmap
//!   frames with AppKit (variable value / transform), so the animation is
//!   faithful to Apple's rendering and needs no GPUI animation support.
//!
//! - **GPUI-driven** (all other variants): the [`Icon`](crate::Icon) component
//!   animates the rendered image at the element level (opacity, size, offset).
//!   These are close approximations — Apple animates individual symbol layers
//!   with spring physics, which is not reproducible from a flattened bitmap.
//!
//! ## Not supported
//!
//! `Draw On` / `Draw Off` (SF Symbols 5+) animate the symbol's stroke paths
//! being drawn, which requires per-layer vector data that a rendered bitmap
//! does not retain. They are intentionally absent.

use std::time::Duration;

/// One rendered bitmap frame of an animated symbol.
///
/// Pixel format matches [`SfSymbol::render_rgba`](crate::SfSymbol::render_rgba):
/// straight (non-premultiplied) alpha, sRGB.
#[derive(Clone)]
pub struct SymbolFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// RGBA pixel data, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
    /// How long this frame should be displayed.
    pub delay: Duration,
}

/// Symbol animation, mirroring Apple's `NSSymbolEffect` family.
///
/// Fidelity notes per variant:
///
/// | Variant | Strategy | Fidelity |
/// |---------|----------|----------|
/// | `VariableColor` | frame-sampled from AppKit `variableValue` | exact |
/// | `Rotate` | frame-sampled, rotated with `NSAffineTransform` | faithful |
/// | `Pulse` | element opacity | close (Apple's pulse is opacity-based) |
/// | `Breathe` | element opacity + size | close approximation |
/// | `Appear` / `Disappear` | element opacity | close |
/// | `Bounce` | element vertical offset | approximation (whole glyph) |
/// | `Wiggle` | element horizontal offset | approximation (Apple rotates layers) |
/// | `ScaleUp` / `ScaleDown` | element display size | approximation (whole glyph) |
/// | `Replace` | cross-fade on symbol change | approximation of non-magic replace |
///
/// `Draw On` / `Draw Off` are not supported — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolEffect {
    /// Variable Color: cycles the symbol's `variableValue` 0 → 1 → 0, lighting
    /// up successive layers. Rendered as a bitmap frame sequence; natively
    /// per-layer (macOS 14+).
    VariableColor,
    /// Rotates the symbol clockwise through a full turn. Rendered as a bitmap
    /// frame sequence (the canvas grows to fit the diagonal, so nothing clips).
    Rotate,
    /// Repeatedly dims and restores the symbol's opacity.
    Pulse,
    /// Gentle combined scale + opacity oscillation.
    Breathe,
    /// Bobs the symbol vertically.
    Bounce,
    /// Wags the symbol horizontally (approximation of Apple's rotation-based
    /// wiggle).
    Wiggle,
    /// Repeatedly scales the symbol up and back.
    ScaleUp,
    /// Repeatedly scales the symbol down and back.
    ScaleDown,
    /// One-shot fade-in.
    Appear,
    /// One-shot fade-out.
    Disappear,
    /// Cross-fade to the new symbol when the [`Icon`](crate::Icon)'s name changes.
    Replace,
}

impl SymbolEffect {
    /// Whether this effect is rendered by the core layer into bitmap frames.
    ///
    /// [`VariableColor`](Self::VariableColor) and [`Rotate`](Self::Rotate) are
    /// bitmap-sampled; the other effects are driven by GPUI at the element
    /// level (see module docs).
    pub fn is_bitmap_sampled(&self) -> bool {
        matches!(self, Self::VariableColor | Self::Rotate)
    }
}

/// How an animation repeats, mirroring the SF Symbols inspector's
/// `Once` / `Repeat with Delay` / `Continuously` (and AppKit's
/// `NSSymbolEffectOptions.repeatCount` / `repeatDelay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Repeat {
    /// Play once, then stop at the end state.
    Once,
    /// Loop without pausing (default for ongoing effects).
    #[default]
    Continuously,
    /// Loop, pausing between cycles.
    WithDelay(Duration),
}

/// Whether an effect animates the whole symbol or individual layers,
/// mirroring the SF Symbols inspector's `Animate` option.
///
/// Only [`SymbolEffect::VariableColor`] is natively per-layer here (that is
/// what `variableValue` rendering does). All other effects animate the whole
/// symbol regardless of this setting, since a rendered bitmap no longer
/// carries layer information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Animate {
    /// Animate the symbol as a whole (default).
    #[default]
    WholeSymbol,
    /// Animate layer by layer where the effect supports it
    /// ([`SymbolEffect::VariableColor`]); other effects fall back to
    /// [`Animate::WholeSymbol`].
    ByLayer,
}

/// Direction option for effects that support one, mirroring Apple's
/// `.up` / `.down` / `.left` / `.right` / `.clockwise` / `.counterClockwise`.
///
/// Effect support:
///
/// | Effect | Supported directions | Default |
/// |--------|----------------------|---------|
/// | `Bounce` | `Up`, `Down` | `Up` |
/// | `Wiggle` | `Left`, `Right`, `Up`, `Down` | `Right` |
/// | `Rotate` | `Clockwise`, `Counterclockwise` | `Clockwise` |
/// | `Appear` / `Disappear` | `Up`, `Down` (slide while fading) | no slide |
/// | `Replace` | `Up` (≈ `.offUp`), `Down` (≈ `.downUp`) | plain cross-fade |
/// | others | — (ignored) | — |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// The effect's default direction (see table).
    #[default]
    Default,
    Up,
    Down,
    Left,
    Right,
    Clockwise,
    Counterclockwise,
}
