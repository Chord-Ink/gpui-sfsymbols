//! Color type for SF Symbol rendering.

/// An sRGB color with straight (non-premultiplied) alpha, used to tint symbols.
///
/// This is the crate's currency color type: it is small, `Copy`, and
/// hash-friendly, so it can be part of render cache keys.
///
/// # Example
///
/// ```rust,ignore
/// use gpui_sfsymbols::SymbolColor;
///
/// let red = SymbolColor::rgb(0xFF0000);
/// let translucent_blue = SymbolColor::rgb(0x007AFF).with_alpha(0.5);
/// let exact = SymbolColor::rgba(0x007AFF80); // 0xRRGGBBAA
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolColor {
    /// Red channel (sRGB).
    pub r: u8,
    /// Green channel (sRGB).
    pub g: u8,
    /// Blue channel (sRGB).
    pub b: u8,
    /// Alpha (opacity) channel; 255 is fully opaque.
    pub a: u8,
}

impl SymbolColor {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0x000000);

    /// Create an opaque color from a `0xRRGGBB` hex value.
    pub const fn rgb(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }

    /// Create a color from a `0xRRGGBBAA` hex value.
    pub const fn rgba(hex: u32) -> Self {
        Self {
            r: ((hex >> 24) & 0xFF) as u8,
            g: ((hex >> 16) & 0xFF) as u8,
            b: ((hex >> 8) & 0xFF) as u8,
            a: (hex & 0xFF) as u8,
        }
    }

    /// Return this color with its alpha replaced by `alpha` (clamped to `0.0 - 1.0`).
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        let clamped = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.a = (clamped * 255.0).round() as u8;
        self
    }

    /// The `0xRRGGBB` hex value of this color (alpha discarded).
    pub fn to_rgb_hex(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// The alpha of this color as `0.0 - 1.0`.
    pub fn alpha(self) -> f32 {
        self.a as f32 / 255.0
    }
}

impl Default for SymbolColor {
    fn default() -> Self {
        Self::BLACK
    }
}

impl From<u32> for SymbolColor {
    /// Interprets the value as `0xRRGGBB` (matches the historical
    /// `SfSymbol::color(u32)` behavior).
    fn from(hex: u32) -> Self {
        Self::rgb(hex)
    }
}

#[cfg(feature = "gpui")]
impl From<gpui::Rgba> for SymbolColor {
    fn from(rgba: gpui::Rgba) -> Self {
        // GPUI's Rgba -> u32 conversion yields 0xRRGGBBAA.
        let hex: u32 = rgba.into();
        Self::rgba(hex)
    }
}

#[cfg(feature = "gpui")]
impl From<gpui::Hsla> for SymbolColor {
    fn from(hsla: gpui::Hsla) -> Self {
        let rgba: gpui::Rgba = hsla.into();
        rgba.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_hex_roundtrip() {
        let c = SymbolColor::rgb(0x007AFF);
        assert_eq!((c.r, c.g, c.b, c.a), (0x00, 0x7A, 0xFF, 255));
        assert_eq!(c.to_rgb_hex(), 0x007AFF);
    }

    #[test]
    fn rgba_hex() {
        let c = SymbolColor::rgba(0x007AFF80);
        assert_eq!((c.r, c.g, c.b, c.a), (0x00, 0x7A, 0xFF, 0x80));
    }

    #[test]
    fn with_alpha_clamps() {
        assert_eq!(SymbolColor::BLACK.with_alpha(0.5).a, 128);
        assert_eq!(SymbolColor::BLACK.with_alpha(-1.0).a, 0);
        assert_eq!(SymbolColor::BLACK.with_alpha(2.0).a, 255);
        assert_eq!(SymbolColor::BLACK.with_alpha(f32::NAN).a, 255);
    }
}
