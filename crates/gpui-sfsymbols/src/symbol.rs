//! SF Symbol rendering to raw RGBA or GPUI RenderImage.

use std::time::Duration;

use crate::color::SymbolColor;
use crate::effect::{Direction, SymbolEffect, SymbolFrame};
use crate::platform::{self, MAX_PALETTE_COLORS, RenderParams};

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
    color: SymbolColor,
    palette: Vec<SymbolColor>,
    /// `Option<f64>` variable value: `u64::MAX` for `None`, else `to_bits()`
    /// (the setter rejects non-finite values, so all-ones NaN cannot collide).
    variable_bits: u64,
    /// Animation parameters for animated renders (zeroed = static render;
    /// effect stored as discriminant + 1 so `VariableColor` != static).
    animation_effect: u8,
    animation_direction: u8,
    animation_fps: u8,
    animation_duration_ms: u64,
    weight: u8,
    symbol_scale: u8,
    rendering_mode: u8,
}

#[cfg(feature = "cache")]
impl CacheKey {
    fn new(symbol: &SfSymbol, animation: Option<(SymbolEffect, Direction, u8, Duration)>) -> Self {
        let (animation_effect, animation_direction, animation_fps, animation_duration_ms) =
            animation
                .map(|(effect, direction, fps, d)| {
                    (effect as u8 + 1, direction as u8, fps, d.as_millis() as u64)
                })
                .unwrap_or((0, 0, 0, 0));
        Self {
            name: symbol.name.clone(),
            size_bits: symbol.size.to_bits(),
            scale_bits: symbol.scale.to_bits(),
            color: symbol.color,
            palette: symbol.palette.clone(),
            variable_bits: symbol.variable_value.map_or(u64::MAX, |v| v.to_bits()),
            animation_effect,
            animation_direction,
            animation_fps,
            animation_duration_ms,
            weight: symbol.weight as u8,
            symbol_scale: symbol.symbol_scale as u8,
            rendering_mode: symbol.rendering_mode as u8,
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
/// Determines how the symbol is colorized. Mirrors SwiftUI's
/// `SymbolRenderingMode` / AppKit's `NSImageSymbolConfiguration` color APIs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RenderingMode {
    /// Single color applied to entire symbol
    Monochrome,
    /// Hierarchical coloring with primary color and automatic opacity layers
    #[default]
    Hierarchical,
    /// Multiple distinct colors (primary, secondary, tertiary) supplied via
    /// [`SfSymbol::palette_colors`]; without palette colors it behaves like
    /// [`RenderingMode::Hierarchical`]
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
use smallvec::{SmallVec, smallvec};

/// SF Symbol builder for rendering system symbols.
///
/// # Example
///
/// ```rust,ignore
/// use gpui_sfsymbols::{SfSymbol, SymbolWeight, SymbolScale, RenderingMode};
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
    color: SymbolColor,
    palette: Vec<SymbolColor>,
    weight: SymbolWeight,
    symbol_scale: SymbolScale,
    rendering_mode: RenderingMode,
    variable_value: Option<f64>,
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
            color: SymbolColor::BLACK,
            palette: Vec::new(),
            weight: SymbolWeight::default(),
            symbol_scale: SymbolScale::default(),
            rendering_mode: RenderingMode::default(),
            variable_value: None,
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

    /// Set the symbol color (default: opaque black), mirroring SwiftUI's
    /// `.foregroundStyle(...)` for single-color modes.
    ///
    /// Accepts [`SymbolColor`], `u32` (`0xRRGGBB`, opaque), and with the
    /// `gpui` feature `gpui::Hsla` / `gpui::Rgba`. The value's own alpha is
    /// honored; adjust opacity afterwards with [`SfSymbol::alpha`] if needed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{SfSymbol, SymbolColor};
    ///
    /// SfSymbol::new("star.fill")
    ///     .color(0xFF0000)  // red
    ///     .render();
    ///
    /// SfSymbol::new("star.fill")
    ///     .color(SymbolColor::rgb(0xFF0000).with_alpha(0.5))
    ///     .render();
    /// ```
    pub fn color(mut self, color: impl Into<SymbolColor>) -> Self {
        self.color = color.into();
        self
    }

    /// Set the palette colors used with [`RenderingMode::Palette`].
    ///
    /// Up to 3 colors are used — primary, secondary, tertiary — mirroring
    /// SwiftUI's `.foregroundStyle(c1, c2, c3)`; extra colors are ignored.
    /// Each color's own alpha is honored.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{SfSymbol, RenderingMode, SymbolColor};
    ///
    /// SfSymbol::new("cloud.sun.fill")
    ///     .rendering_mode(RenderingMode::Palette)
    ///     .palette_colors([SymbolColor::rgb(0x007AFF), SymbolColor::rgb(0xFFCC00)])
    ///     .render();
    /// ```
    pub fn palette_colors<I, C>(mut self, colors: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<SymbolColor>,
    {
        self.palette = colors
            .into_iter()
            .take(MAX_PALETTE_COLORS)
            .map(Into::into)
            .collect();
        self
    }

    /// Set the Variable Color value for the symbol (macOS 14+).
    ///
    /// `0.0` shows no emphasized layers, `1.0` shows all of them (for symbols
    /// that support Variable Color). Non-finite values unset the value;
    /// finite values are clamped to `0.0 - 1.0`. On macOS < 14 the symbol
    /// renders in its default appearance instead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// SfSymbol::new("wifi")
    ///     .variable_value(0.5)
    ///     .render();
    /// ```
    pub fn variable_value(mut self, value: impl Into<Option<f64>>) -> Self {
        self.variable_value = value
            .into()
            .and_then(|v| v.is_finite().then(|| v.clamp(0.0, 1.0)));
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
        self.color = self.color.with_alpha(alpha);
        self
    }

    /// Set the symbol weight (default: Regular).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{SfSymbol, SymbolWeight};
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
    /// use gpui_sfsymbols::{SfSymbol, SymbolScale};
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
    /// use gpui_sfsymbols::{SfSymbol, RenderingMode};
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
        platform::render_sf_symbol(&RenderParams {
            name: &self.name,
            point_size: self.size,
            scale: self.scale,
            color: self.color,
            palette: &self.palette,
            weight: self.weight,
            symbol_scale: self.symbol_scale,
            rendering_mode: self.rendering_mode,
            variable_value: self.variable_value,
            rotation_degrees: None,
        })
    }

    /// Render one rotated frame (bitmap-sampled animation support).
    pub(crate) fn render_rgba_rotated(&self, degrees: f64) -> Option<(u32, u32, Vec<u8>)> {
        platform::render_sf_symbol(&RenderParams {
            name: &self.name,
            point_size: self.size,
            scale: self.scale,
            color: self.color,
            palette: &self.palette,
            weight: self.weight,
            symbol_scale: self.symbol_scale,
            rendering_mode: self.rendering_mode,
            variable_value: self.variable_value,
            rotation_degrees: Some(degrees),
        })
    }

    /// Render an animation cycle as a sequence of bitmap frames.
    ///
    /// The bitmap-sampled effects are [`SymbolEffect::VariableColor`] (the
    /// symbol's `variableValue` cycles 0 → 1 → 0; macOS 14+ — on older
    /// systems every frame renders the default appearance) and
    /// [`SymbolEffect::Rotate`] (a full turn on a square canvas sized to the
    /// glyph's diagonal; [`Direction::Clockwise`] by default, or
    /// [`Direction::Counterclockwise`]). Returns `None` for other effects,
    /// which are GPUI-driven at the element level — see [`SymbolEffect`].
    ///
    /// `fps` is clamped to `1 - 60`, `duration` to `50ms - 10s`. Every frame
    /// gets an equal `delay` of `duration / frame_count`.
    pub fn render_frames(
        &self,
        effect: SymbolEffect,
        direction: Direction,
        fps: u8,
        duration: Duration,
    ) -> Option<Vec<SymbolFrame>> {
        if !effect.is_bitmap_sampled() {
            return None;
        }

        const MIN_DURATION: Duration = Duration::from_millis(50);
        const MAX_DURATION: Duration = Duration::from_secs(10);
        let fps = fps.clamp(1, 60);
        let duration = duration.clamp(MIN_DURATION, MAX_DURATION);

        let frame_count = ((duration.as_secs_f64() * fps as f64).round() as u32).max(2);
        let frame_delay = duration / frame_count;

        let mut frames = Vec::with_capacity(frame_count as usize);
        for i in 0..frame_count {
            let t = i as f64 / frame_count as f64;
            let rendered = match effect {
                // Triangle wave 0 -> 1 -> 0 across the cycle.
                SymbolEffect::VariableColor => self
                    .clone()
                    .variable_value(1.0 - (2.0 * t - 1.0).abs())
                    .render_rgba(),
                // Full turn; the final 360° frame is skipped because it
                // duplicates the first. Positive degrees rotate clockwise
                // (see `render_rgba_rotated`), negative counterclockwise.
                SymbolEffect::Rotate => {
                    let degrees = if direction == Direction::Counterclockwise {
                        -360.0 * t
                    } else {
                        360.0 * t
                    };
                    self.render_rgba_rotated(degrees)
                }
                _ => unreachable!("checked is_bitmap_sampled above"),
            }?;
            frames.push(SymbolFrame {
                width: rendered.0,
                height: rendered.1,
                rgba: rendered.2,
                delay: frame_delay,
            });
        }
        Some(frames)
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
        let cache_key = CacheKey::new(self, None);

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

    /// Render an animation cycle to a multi-frame GPUI `RenderImage`, which
    /// `img()` plays back (looped) automatically.
    ///
    /// The bitmap-sampled effects are [`SymbolEffect::VariableColor`] and
    /// [`SymbolEffect::Rotate`]; returns `None` for other effects (they are
    /// animated at the element level — see [`SymbolEffect`]) and when
    /// rendering fails.
    ///
    /// Returns `(image, width, height)`. With the `cache` feature enabled,
    /// results are cached globally, keyed on the symbol configuration plus
    /// `fps` and `duration`.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", feature = "cache"))]
    pub fn render_animated(
        &self,
        effect: SymbolEffect,
        direction: Direction,
        fps: u8,
        duration: Duration,
    ) -> Option<(Arc<RenderImage>, u32, u32)> {
        let cache_key = CacheKey::new(self, Some((effect, direction, fps, duration)));

        // Check cache first
        {
            let cache = get_cache();
            if let Ok(guard) = cache.lock()
                && let Some((cached, w, h)) = guard.get(&cache_key)
            {
                return Some((Arc::clone(cached), *w, *h));
            }
        }

        let frames = self.render_frames(effect, direction, fps, duration)?;
        let (image, width, height) = frames_to_render_image(frames)?;

        // Store in cache, bounded (see `render_with_size`).
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

    /// Render an animation cycle to a multi-frame GPUI `RenderImage`, which
    /// `img()` plays back (looped) automatically.
    ///
    /// The bitmap-sampled effects are [`SymbolEffect::VariableColor`] and
    /// [`SymbolEffect::Rotate`]; returns `None` for other effects (they are
    /// animated at the element level — see [`SymbolEffect`]) and when
    /// rendering fails.
    ///
    /// Requires the `gpui` feature.
    #[cfg(all(feature = "gpui", not(feature = "cache")))]
    pub fn render_animated(
        &self,
        effect: SymbolEffect,
        direction: Direction,
        fps: u8,
        duration: Duration,
    ) -> Option<(Arc<RenderImage>, u32, u32)> {
        let frames = self.render_frames(effect, direction, fps, duration)?;
        frames_to_render_image(frames)
    }
}

/// Pack sampled bitmap frames into a multi-frame `RenderImage`
/// (converting RGBA to BGRA, like the static render path).
///
/// Returns `(image, width, height)` with the dimensions of the first frame.
#[cfg(feature = "gpui")]
fn frames_to_render_image(frames: Vec<SymbolFrame>) -> Option<(Arc<RenderImage>, u32, u32)> {
    let (first_width, first_height) = frames.first().map(|f| (f.width, f.height))?;

    let mut image_frames: SmallVec<[Frame; 1]> = SmallVec::new();
    for frame in frames {
        let mut data = frame.rgba;
        rgba_to_bgra(&mut data);
        let rgba_image = RgbaImage::from_raw(frame.width, frame.height, data)?;
        image_frames.push(Frame::from_parts(
            rgba_image,
            0,
            0,
            image::Delay::from_saturating_duration(frame.delay),
        ));
    }

    Some((
        Arc::new(RenderImage::new(image_frames)),
        first_width,
        first_height,
    ))
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
        assert_eq!(symbol.color.to_rgb_hex(), 0xFF0000);
        assert!((symbol.color.alpha() - 0.5).abs() < 0.01);
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
                SfSymbol::new("star.fill")
                    .scale(bad)
                    .render_rgba()
                    .is_none(),
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
        let has_translucent = data.chunks_exact(4).any(|px| px[3] > 0 && px[3] < 255);
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

    /// Palette colors are honored: a two-color palette on a multi-level
    /// symbol must produce both hues, unlike single-color hierarchical.
    #[test]
    fn test_palette_colors_render() {
        let blue = SymbolColor::rgb(0x007AFF);
        let yellow = SymbolColor::rgb(0xFFCC00);
        let (_w, _h, data) = SfSymbol::new("cloud.sun.fill")
            .size(64.0)
            .rendering_mode(RenderingMode::Palette)
            .palette_colors([blue, yellow])
            .render_rgba()
            .unwrap();

        let has_blueish = data
            .chunks_exact(4)
            .any(|px| px[3] > 128 && px[2] > 150 && px[0] < 100);
        let has_yellowish = data
            .chunks_exact(4)
            .any(|px| px[3] > 128 && px[0] > 150 && px[1] > 120 && px[2] < 100);
        assert!(has_blueish, "palette primary (blue) should appear");
        assert!(has_yellowish, "palette secondary (yellow) should appear");
    }

    /// More than 3 palette colors are truncated (Apple uses at most
    /// primary/secondary/tertiary) without panicking.
    #[test]
    fn test_palette_truncated_to_three() {
        let symbol = SfSymbol::new("star.fill").palette_colors([
            SymbolColor::rgb(0xFF0000),
            SymbolColor::rgb(0x00FF00),
            SymbolColor::rgb(0x0000FF),
            SymbolColor::rgb(0xFFFFFF),
        ]);
        assert_eq!(symbol.palette.len(), 3);
        assert!(symbol.render_rgba().is_some());
    }

    /// Multicolor uses the symbol's intrinsic colors: `cloud.sun.fill` shows
    /// both gray (cloud) and yellow/orange (sun) regardless of `color()`.
    #[test]
    fn test_multicolor_distinct_hues() {
        let (_w, _h, data) = SfSymbol::new("cloud.sun.fill")
            .size(64.0)
            .rendering_mode(RenderingMode::Multicolor)
            .color(0xFF0000) // ignored by design
            .render_rgba()
            .unwrap();
        let has_warm = data
            .chunks_exact(4)
            .any(|px| px[3] > 128 && px[0] > 150 && px[1] > 100 && px[2] < 120);
        let has_cool = data.chunks_exact(4).any(|px| {
            px[3] > 128 && (px[0].max(px[1]).max(px[2]) - px[0].min(px[1]).min(px[2]) < 60)
        });
        assert!(has_warm, "multicolor should render the warm sun hue");
        assert!(has_cool, "multicolor should render the neutral cloud");
    }

    /// Variable value is clamped/validated, and renders on supported systems.
    #[test]
    fn test_variable_value() {
        let symbol = SfSymbol::new("wifi").variable_value(2.5);
        assert_eq!(symbol.variable_value, Some(1.0));

        let symbol = SfSymbol::new("wifi").variable_value(-1.0);
        assert_eq!(symbol.variable_value, Some(0.0));

        let symbol = SfSymbol::new("wifi").variable_value(f64::NAN);
        assert_eq!(symbol.variable_value, None);

        assert!(
            SfSymbol::new("wifi")
                .variable_value(0.5)
                .render_rgba()
                .is_some()
        );
    }

    /// Variable Color frames: count, delays, and differing content.
    #[test]
    fn test_render_frames_variable_color() {
        let frames = SfSymbol::new("wifi")
            .size(32.0)
            .render_frames(
                SymbolEffect::VariableColor,
                Direction::Default,
                10,
                Duration::from_millis(500),
            )
            .expect("VariableColor frames should render");

        assert_eq!(frames.len(), 5);
        for frame in &frames {
            assert_eq!(frame.delay, Duration::from_millis(100));
            assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
        }
        // First frame (value ~0) and middle frame (value ~1) should differ
        // for a variable-color-capable symbol like `wifi` on macOS 14+.
        assert_ne!(frames[0].rgba, frames[2].rgba);
    }

    /// Non-bitmap-sampled effects are GPUI-driven and yield no frames.
    #[test]
    fn test_render_frames_rejects_element_effects() {
        assert!(
            SfSymbol::new("star.fill")
                .render_frames(
                    SymbolEffect::Pulse,
                    Direction::Default,
                    30,
                    Duration::from_secs(1)
                )
                .is_none()
        );
    }

    /// Rotated frames are square (diagonal canvas) and turn clockwise:
    /// `arrow.up` at 90° must have its (heavier) head on the right.
    #[test]
    fn test_rotate_is_clockwise_on_square_canvas() {
        let symbol = SfSymbol::new("arrow.up").size(64.0);
        let (base_w, base_h, _) = symbol.render_rgba().unwrap();
        let (w, h, data) = symbol.render_rgba_rotated(90.0).unwrap();

        assert_eq!(w, h, "rotated frames should be square");
        let diagonal = ((base_w as f64).hypot(base_h as f64)).ceil() as u32;
        assert!(
            w >= diagonal,
            "canvas {w} should fit the diagonal {diagonal}"
        );

        let ink = |x_start: u32, x_end: u32| -> u64 {
            let mut total = 0;
            for row in data.chunks_exact(w as usize * 4) {
                for px in row[(x_start as usize * 4)..(x_end as usize * 4)].chunks_exact(4) {
                    total += px[3] as u64;
                }
            }
            total
        };
        let left = ink(0, w / 2);
        let right = ink(w / 2, w);
        assert!(
            right > left,
            "90° clockwise should move arrow.up's head to the right (left={left}, right={right})"
        );
    }

    /// Counterclockwise rotation goes the other way: `arrow.up` at 90° CCW
    /// must have its head on the LEFT (mirror of the clockwise test).
    #[test]
    fn test_rotate_counterclockwise() {
        let symbol = SfSymbol::new("arrow.up").size(64.0);
        let frames = symbol
            .render_frames(
                SymbolEffect::Rotate,
                Direction::Counterclockwise,
                10,
                Duration::from_millis(400),
            )
            .expect("Rotate frames should render");
        // 4 frames: 0°, -90°, -180°, -270°. Frame 1 is the 90° CCW view.
        assert_eq!(frames.len(), 4);
        let frame = &frames[1];
        let (w, data) = (frame.width, &frame.rgba);
        let ink = |x_start: u32, x_end: u32| -> u64 {
            let mut total = 0;
            for row in data.chunks_exact(w as usize * 4) {
                for px in row[(x_start as usize * 4)..(x_end as usize * 4)].chunks_exact(4) {
                    total += px[3] as u64;
                }
            }
            total
        };
        let left = ink(0, w / 2);
        let right = ink(w / 2, w);
        assert!(
            left > right,
            "90° counterclockwise should move arrow.up's head to the left (left={left}, right={right})"
        );
    }

    /// Rotate frames: count, square dims, delays, and differing content.
    #[test]
    fn test_render_frames_rotate() {
        let frames = SfSymbol::new("gearshape")
            .size(32.0)
            .render_frames(
                SymbolEffect::Rotate,
                Direction::Default,
                12,
                Duration::from_millis(500),
            )
            .expect("Rotate frames should render");

        assert_eq!(frames.len(), 6);
        for frame in &frames {
            assert_eq!(frame.delay, Duration::from_millis(500) / 6);
            assert_eq!(frame.width, frame.height, "rotated frames are square");
            assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
        }
        // 0° vs 60° (gearshape has 45° symmetry, so 60° must differ).
        assert_ne!(frames[0].rgba, frames[1].rgba);
    }

    #[test]
    #[cfg(feature = "gpui")]
    fn test_render_animated_variable_color() {
        let (image, width, height) = SfSymbol::new("wifi")
            .size(32.0)
            .render_animated(
                SymbolEffect::VariableColor,
                Direction::Default,
                10,
                Duration::from_millis(500),
            )
            .expect("animated render should succeed");
        assert!(width > 0 && height > 0);
        assert_eq!(image.frame_count(), 5);
    }

    #[test]
    #[cfg(feature = "gpui")]
    fn test_render_animated_rotate() {
        let (image, width, height) = SfSymbol::new("gearshape")
            .size(32.0)
            .render_animated(
                SymbolEffect::Rotate,
                Direction::Default,
                12,
                Duration::from_millis(500),
            )
            .expect("animated render should succeed");
        assert_eq!(width, height);
        assert_eq!(image.frame_count(), 6);
    }
}
