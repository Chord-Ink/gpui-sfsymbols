//! SF Symbol rendering to raw RGBA or GPUI RenderImage.

use crate::platform;

#[cfg(feature = "cache")]
use std::collections::HashMap;
#[cfg(feature = "cache")]
use std::sync::{Mutex, OnceLock};

/// Cache key for rendered symbols.
#[cfg(feature = "cache")]
#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    name: String,
    size_bits: u32,
    scale_bits: u32,
    color: u32,
    alpha_bits: u32,
    weight: u8,
    symbol_scale: u8,
    rendering_mode: u8,
}

#[cfg(feature = "cache")]
impl CacheKey {
    fn new(
        name: &str,
        size: f32,
        scale: f32,
        color: u32,
        alpha: f32,
        weight: SymbolWeight,
        symbol_scale: SymbolScale,
        rendering_mode: RenderingMode,
    ) -> Self {
        Self {
            name: name.to_string(),
            size_bits: size.to_bits(),
            scale_bits: scale.to_bits(),
            color,
            alpha_bits: alpha.to_bits(),
            weight: weight as u8,
            symbol_scale: symbol_scale as u8,
            rendering_mode: rendering_mode as u8,
        }
    }
}

/// Global cache for rendered symbols.
/// Stores (image, width, height) to preserve aspect ratio information.
#[cfg(feature = "cache")]
type SymbolCache = HashMap<CacheKey, (std::sync::Arc<gpui::RenderImage>, u32, u32)>;

#[cfg(feature = "cache")]
static SYMBOL_CACHE: OnceLock<Mutex<SymbolCache>> = OnceLock::new();

/// Maximum number of entries in the global symbol cache.
///
/// Each entry pins a full RGBA frame, so an unbounded cache lets long-running
/// apps grow memory without limit (the key includes caller-controlled name,
/// size, and scale). When the cap is reached, an arbitrary entry is evicted.
#[cfg(feature = "cache")]
const MAX_CACHE_ENTRIES: usize = 256;

#[cfg(feature = "cache")]
fn get_cache() -> &'static Mutex<SymbolCache> {
    SYMBOL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clear the global symbol cache.
///
/// Call this if you need to free memory or if system appearance changed.
#[cfg(feature = "cache")]
pub fn clear_cache() {
    if let Some(cache) = SYMBOL_CACHE.get()
        && let Ok(mut guard) = cache.lock()
    {
        guard.clear();
    }
}

/// Get the number of cached symbols.
#[cfg(feature = "cache")]
pub fn cache_size() -> usize {
    SYMBOL_CACHE
        .get()
        .and_then(|c| c.lock().ok())
        .map(|g| g.len())
        .unwrap_or(0)
}

/// Symbol weight for SF Symbols.
///
/// Corresponds to NSFontWeight values used in NSImageSymbolConfiguration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SymbolWeight {
    /// Ultra light weight (-0.8)
    UltraLight,
    /// Thin weight (-0.6)
    Thin,
    /// Light weight (-0.4)
    Light,
    /// Regular weight (0.0) - default
    #[default]
    Regular,
    /// Medium weight (0.23)
    Medium,
    /// Semibold weight (0.3)
    Semibold,
    /// Bold weight (0.4)
    Bold,
    /// Heavy weight (0.56)
    Heavy,
    /// Black weight (0.62)
    Black,
}

impl SymbolWeight {
    /// Convert to NSFontWeight value.
    pub fn to_ns_weight(self) -> f64 {
        match self {
            Self::UltraLight => -0.8,
            Self::Thin => -0.6,
            Self::Light => -0.4,
            Self::Regular => 0.0,
            Self::Medium => 0.23,
            Self::Semibold => 0.3,
            Self::Bold => 0.4,
            Self::Heavy => 0.56,
            Self::Black => 0.62,
        }
    }
}

/// Symbol scale for SF Symbols.
///
/// Corresponds to NSImageSymbolScale values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SymbolScale {
    /// Small scale (1)
    Small,
    /// Medium scale (2) - default
    #[default]
    Medium,
    /// Large scale (3)
    Large,
}

impl SymbolScale {
    /// Convert to NSImageSymbolScale value.
    pub fn to_ns_scale(self) -> i64 {
        match self {
            Self::Small => 1,
            Self::Medium => 2,
            Self::Large => 3,
        }
    }
}

/// Rendering mode for SF Symbols.
///
/// Determines how the symbol is colorized.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RenderingMode {
    /// Single color applied to entire symbol
    Monochrome,
    /// Hierarchical coloring with primary color and automatic opacity layers
    #[default]
    Hierarchical,
    /// Multiple colors (requires palette colors; with the single-color builder
    /// API this intentionally renders like [`RenderingMode::Hierarchical`])
    Palette,
    /// Original multicolor design of the symbol
    Multicolor,
}

#[cfg(feature = "gpui")]
use std::sync::Arc;

#[cfg(feature = "gpui")]
use gpui::RenderImage;
#[cfg(feature = "gpui")]
use image::{Frame, RgbaImage};
#[cfg(feature = "gpui")]
use smallvec::smallvec;

/// SF Symbol builder for rendering system symbols.
///
/// # Example
///
/// ```rust,ignore
/// use gpui_symbols::{SfSymbol, SymbolWeight, SymbolScale, RenderingMode};
///
/// // Render to raw RGBA
/// let (width, height, data) = SfSymbol::new("star.fill")
///     .render_rgba()
///     .unwrap();
///
/// // With gpui feature and advanced options:
/// let image = SfSymbol::new("heart.fill")
///     .size(48.0)
///     .color(0xFF0000)
///     .weight(SymbolWeight::Bold)
///     .symbol_scale(SymbolScale::Large)
///     .rendering_mode(RenderingMode::Hierarchical)
///     .render()
///     .unwrap();
/// ```
#[derive(Clone)]
pub struct SfSymbol {
    name: String,
    size: f32,
    scale: f32,
    color: u32,
    alpha: f32,
    weight: SymbolWeight,
    symbol_scale: SymbolScale,
    rendering_mode: RenderingMode,
}

impl SfSymbol {
    /// Create a new SF Symbol with the given name.
    ///
    /// See [SF Symbols](https://developer.apple.com/sf-symbols/) for available symbol names.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 32.0,
            scale: 2.0,
            color: 0x000000, // black
            alpha: 1.0,
            weight: SymbolWeight::default(),
            symbol_scale: SymbolScale::default(),
            rendering_mode: RenderingMode::default(),
        }
    }

    /// Set the point size of the symbol (default: 32.0).
    ///
    /// Must be finite and within `0 < size <= 1024`; invalid values make
    /// rendering return `None`.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the scale factor for rendering (default: 2.0 for Retina).
    ///
    /// Must be finite and within `0 < scale <= 8`; invalid values make
    /// rendering return `None`.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Set the color as RGB hex value (default: 0x000000 black).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// SfSymbol::new("star.fill")
    ///     .color(0xFF0000)  // red
    ///     .render();
    /// ```
    pub fn color(mut self, color: u32) -> Self {
        self.color = color;
        self
    }

    /// Set the opacity of the symbol color (default: 1.0).
    ///
    /// Values are clamped to `0.0 - 1.0`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// SfSymbol::new("star.fill")
    ///     .color(0xFF0000)
    ///     .alpha(0.5)  // 50% opacity red
    ///     .render();
    /// ```
    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Set the symbol weight (default: Regular).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_symbols::{SfSymbol, SymbolWeight};
    ///
    /// SfSymbol::new("star.fill")
    ///     .weight(SymbolWeight::Bold)
    ///     .render();
    /// ```
    pub fn weight(mut self, weight: SymbolWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set the symbol scale (default: Medium).
    ///
    /// This controls the overall visual weight/size of the symbol
    /// independent of the point size.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_symbols::{SfSymbol, SymbolScale};
    ///
    /// SfSymbol::new("star.fill")
    ///     .symbol_scale(SymbolScale::Large)
    ///     .render();
    /// ```
    pub fn symbol_scale(mut self, symbol_scale: SymbolScale) -> Self {
        self.symbol_scale = symbol_scale;
        self
    }

    /// Set the rendering mode (default: Hierarchical).
    ///
    /// - `Monochrome`: Single color for the entire symbol
    /// - `Hierarchical`: Primary color with automatic opacity layers
    /// - `Palette`: Multiple distinct colors (requires palette support)
    /// - `Multicolor`: Original multicolor design
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_symbols::{SfSymbol, RenderingMode};
    ///
    /// SfSymbol::new("cloud.sun.fill")
    ///     .rendering_mode(RenderingMode::Multicolor)
    ///     .render();
    /// ```
    pub fn rendering_mode(mut self, mode: RenderingMode) -> Self {
        self.rendering_mode = mode;
        self
    }

    /// Render to raw RGBA pixel data.
    ///
    /// Pixels are straight (non-premultiplied) alpha in the sRGB color space.
    ///
    /// Returns `(width, height, rgba_data)` or `None` if rendering fails
    /// (unknown symbol or invalid parameters — see [`SfSymbol::size`] and
    /// [`SfSymbol::scale`]).
    pub fn render_rgba(&self) -> Option<(u32, u32, Vec<u8>)> {
        platform::render_sf_symbol(
            &self.name,
            self.size,
            self.scale,
            self.color,
            self.alpha,
            self.weight,
            self.symbol_scale,
            self.rendering_mode,
        )
    }

    /// Render the symbol to a GPUI RenderImage.
    ///
    /// With the `cache` feature enabled, results are cached globally
    /// to avoid redundant rendering. Use [`clear_cache`] to free memory if needed.
    ///
    /// Returns `None` if the symbol cannot be found or rendered.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", feature = "cache"))]
    pub fn render(&self) -> Option<Arc<RenderImage>> {
        self.render_with_size().map(|(image, _, _)| image)
    }

    /// Render the symbol to a GPUI RenderImage with its actual dimensions.
    ///
    /// Returns `(image, width, height)` where width and height are the actual
    /// rendered pixel dimensions. SF Symbols may not be square - use these
    /// dimensions to preserve the correct aspect ratio when displaying.
    ///
    /// With the `cache` feature enabled, results are cached globally
    /// to avoid redundant rendering. Use [`clear_cache`] to free memory if needed.
    ///
    /// Returns `None` if the symbol cannot be found or rendered.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", feature = "cache"))]
    pub fn render_with_size(&self) -> Option<(Arc<RenderImage>, u32, u32)> {
        let cache_key = CacheKey::new(
            &self.name,
            self.size,
            self.scale,
            self.color,
            self.alpha,
            self.weight,
            self.symbol_scale,
            self.rendering_mode,
        );

        // Check cache first
        {
            let cache = get_cache();
            if let Ok(guard) = cache.lock()
                && let Some((cached, w, h)) = guard.get(&cache_key)
            {
                return Some((Arc::clone(cached), *w, *h));
            }
        }

        // Render and cache
        let (width, height, mut data) = self.render_rgba()?;

        // Convert RGBA to BGRA for GPUI's Metal renderer
        rgba_to_bgra(&mut data);

        let rgba_image = RgbaImage::from_raw(width, height, data)?;
        let frame = Frame::new(rgba_image);
        let image = Arc::new(RenderImage::new(smallvec![frame]));

        // Store in cache, bounded: evict an arbitrary entry when full so the
        // cache (and the RenderImages it pins) cannot grow without limit.
        {
            let cache = get_cache();
            if let Ok(mut guard) = cache.lock() {
                if guard.len() >= MAX_CACHE_ENTRIES
                    && let Some(victim) = guard.keys().next().cloned()
                {
                    guard.remove(&victim);
                }
                guard.insert(cache_key, (Arc::clone(&image), width, height));
            }
        }

        Some((image, width, height))
    }

    /// Render the symbol to a GPUI RenderImage.
    ///
    /// Returns `None` if the symbol cannot be found or rendered.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", not(feature = "cache")))]
    pub fn render(&self) -> Option<Arc<RenderImage>> {
        self.render_with_size().map(|(image, _, _)| image)
    }

    /// Render the symbol to a GPUI RenderImage with its actual dimensions.
    ///
    /// Returns `(image, width, height)` where width and height are the actual
    /// rendered pixel dimensions. SF Symbols may not be square - use these
    /// dimensions to preserve the correct aspect ratio when displaying.
    ///
    /// Returns `None` if the symbol cannot be found or rendered.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", not(feature = "cache")))]
    pub fn render_with_size(&self) -> Option<(Arc<RenderImage>, u32, u32)> {
        let (width, height, mut data) = self.render_rgba()?;

        // Convert RGBA to BGRA for GPUI's Metal renderer
        rgba_to_bgra(&mut data);

        let rgba_image = RgbaImage::from_raw(width, height, data)?;
        let frame = Frame::new(rgba_image);

        Some((Arc::new(RenderImage::new(smallvec![frame])), width, height))
    }
}

/// Convert RGBA to BGRA (required by GPUI's Metal renderer).
///
/// The pixel data from AppKit is straight (non-premultiplied) alpha — exactly
/// what GPUI expects — so only the channel order needs fixing.
#[cfg(feature = "gpui")]
fn rgba_to_bgra(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let symbol = SfSymbol::new("star.fill")
            .size(48.0)
            .scale(3.0)
            .color(0xFF0000)
            .alpha(0.5);
        assert_eq!(symbol.size, 48.0);
        assert_eq!(symbol.scale, 3.0);
        assert_eq!(symbol.color, 0xFF0000);
        assert_eq!(symbol.alpha, 0.5);
    }

    #[test]
    fn test_render_rgba() {
        let result = SfSymbol::new("star.fill").render_rgba();
        assert!(result.is_some());

        let (width, height, data) = result.unwrap();
        assert!(width > 0);
        assert!(height > 0);
        assert_eq!(data.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_invalid_symbol() {
        let result = SfSymbol::new("this.symbol.does.not.exist.12345").render_rgba();
        assert!(result.is_none());
    }

    #[test]
    fn test_custom_color() {
        let result = SfSymbol::new("star.fill").color(0xFF0000).render_rgba();
        assert!(result.is_some());
    }

    /// Regression test: absurd/invalid size and scale values must return
    /// `None`, not panic on integer overflow or trigger huge allocations.
    #[test]
    fn test_invalid_size_and_scale_rejected() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e9] {
            assert!(
                SfSymbol::new("star.fill").size(bad).render_rgba().is_none(),
                "size({bad}) should be rejected"
            );
            assert!(
                SfSymbol::new("star.fill").scale(bad).render_rgba().is_none(),
                "scale({bad}) should be rejected"
            );
        }
    }

    /// Regression test: pixel data must be straight (non-premultiplied) alpha.
    /// For a fully-saturated red symbol, translucent pixels keep full red
    /// (r=255) regardless of alpha; premultiplied data would have r <= a.
    #[test]
    fn test_straight_alpha_pixels() {
        let (_w, _h, data) = SfSymbol::new("star.fill")
            .size(64.0)
            .color(0xFF0000)
            .render_rgba()
            .unwrap();
        let has_translucent = data
            .chunks_exact(4)
            .any(|px| px[3] > 0 && px[3] < 255);
        assert!(has_translucent, "expected some translucent pixels");
        assert!(
            data.chunks_exact(4)
                .any(|px| px[3] > 0 && px[3] < 255 && px[0] > px[3]),
            "translucent pixels must not be premultiplied (r should exceed a)"
        );
    }

    /// The color alpha must reach the rendered pixels.
    #[test]
    fn test_alpha_applied() {
        let (_w, _h, data) = SfSymbol::new("star.fill")
            .color(0xFF0000)
            .alpha(0.5)
            .render_rgba()
            .unwrap();
        let max_alpha = data.chunks_exact(4).map(|px| px[3]).max().unwrap();
        assert!(
            max_alpha <= 140,
            "50% opacity should cap alpha near 128, got {max_alpha}"
        );
    }

    #[cfg(feature = "cache")]
    #[test]
    fn test_cache_is_bounded() {
        clear_cache();
        for i in 0..(MAX_CACHE_ENTRIES + 10) {
            let _ = SfSymbol::new("star.fill")
                .size(8.0 + i as f32 * 0.25)
                .render_with_size();
        }
        assert!(
            cache_size() <= MAX_CACHE_ENTRIES,
            "cache grew past its cap: {}",
            cache_size()
        );
        clear_cache();
    }

    #[test]
    #[cfg(feature = "gpui")]
    fn test_render_gpui() {
        let result = SfSymbol::new("star.fill").render();
        assert!(result.is_some());
    }
}
