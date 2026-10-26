//! macOS platform implementation using AppKit (via objc2).
//!
//! Memory management:
//! - Every AppKit object is held in a [`Retained`], which releases on drop.
//! - The whole render runs inside an `NSAutoreleasePool`, so the autoreleased
//!   temporaries AppKit creates are drained on every call, on any thread —
//!   including plain `std::thread`s that have no pool of their own.

use std::marker::{PhantomData, PhantomPinned};
use std::sync::OnceLock;

use objc2::encode::{Encoding, RefEncode};
use objc2::rc::{AutoreleasePool, Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, ClassType, msg_send, sel};
use objc2_app_kit::{
    NSAffineTransformNSAppKitAdditions, NSBitmapFormat, NSBitmapImageRep, NSColor,
    NSCompositingOperation, NSGraphicsContext, NSImage, NSImageSymbolConfiguration,
    NSImageSymbolScale,
};
use objc2_foundation::{NSAffineTransform, NSArray, NSPoint, NSRect, NSSize, NSString};

use crate::color::SymbolColor;
use crate::symbol::{RenderingMode, SymbolScale, SymbolWeight};

/// Upper bounds for user-supplied rendering parameters.
///
/// SF Symbols are UI icons; anything beyond these is a bug in the caller and
/// would overflow buffer arithmetic or attempt enormous allocations.
const MAX_POINT_SIZE: f32 = 1024.0;
const MAX_SCALE: f32 = 8.0;
const MAX_PIXEL_DIMENSION: f64 = 16_384.0;

/// How many times finer a moved glyph is drawn: a quarter-pixel move is a
/// whole pixel there.
const FINE: usize = 4;

/// The pixels per point of the grid on which a glyph's outline is checked
/// against AppKit's drawing.
const CHECK_SCALE: f64 = 4.0;

/// How much of AppKit's ink an outline must share, once stretched to the
/// same proportions, to be drawn in its place.
const MIN_OVERLAP: f64 = 0.75;

/// Maximum number of palette colors Apple uses (primary, secondary, tertiary).
pub(crate) const MAX_PALETTE_COLORS: usize = 3;

/// A CoreGraphics path, `struct CGPath`.
#[repr(C)]
struct CgPath {
    _data: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

// SAFETY: a `CGPathRef` points to a `struct CGPath`.
unsafe impl RefEncode for CgPath {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGPath", &[]));
}

/// A CoreGraphics drawing context, `struct CGContext`.
#[repr(C)]
struct CgContext {
    _data: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

// SAFETY: a `CGContextRef` points to a `struct CGContext`.
unsafe impl RefEncode for CgContext {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGContext", &[]));
}

// SAFETY: these match the CoreGraphics headers, and a reference keeps its
// object alive for the call.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    safe fn CGPathGetPathBoundingBox(path: &CgPath) -> NSRect;
    safe fn CGContextAddPath(context: &CgContext, path: &CgPath);
    safe fn CGContextFillPath(context: &CgContext);
}

/// Parameters for one symbol render.
///
/// Grouped into a struct so the builder in `symbol.rs` and this platform
/// function stay in sync as the feature set grows.
pub(crate) struct RenderParams<'a> {
    /// The SF Symbol name (e.g., "star.fill").
    pub name: &'a str,
    /// Point size for rendering (0 < size <= 1024).
    pub point_size: f32,
    /// Scale factor, e.g. 2.0 for Retina (0 < scale <= 8).
    pub scale: f32,
    /// Primary color (used by Monochrome and Hierarchical, and as the
    /// Palette fallback when `palette` is empty).
    pub color: SymbolColor,
    /// Palette colors (primary, secondary, tertiary) used by
    /// [`RenderingMode::Palette`]; at most [`MAX_PALETTE_COLORS`] are used.
    pub palette: &'a [SymbolColor],
    /// Symbol weight (ultraLight to black).
    pub weight: SymbolWeight,
    /// Symbol scale (small, medium, large).
    pub symbol_scale: SymbolScale,
    /// Rendering mode (monochrome, hierarchical, palette, multicolor).
    pub rendering_mode: RenderingMode,
    /// Variable Color value (0.0 - 1.0, macOS 14+); `None` renders the
    /// symbol's default appearance.
    pub variable_value: Option<f64>,
    /// Rotation in degrees (clockwise) for bitmap-sampled animation frames.
    /// When set, the canvas grows to the glyph's diagonal so nothing clips.
    pub rotation_degrees: Option<f64>,
    /// How far the glyph moves right and down, in device pixels, from where
    /// an offset of zero draws it. Each value is taken modulo 1 and rounded
    /// to a quarter, and an axis that moves gets one more pixel so the ink
    /// never clips. `None` draws the glyph where AppKit draws it at `scale`.
    /// Rotated frames ignore it.
    pub offset: Option<(f32, f32)>,
}

/// Whether `+[NSImageSymbolConfiguration configurationWithHierarchicalColor:]`
/// is available (macOS 12.0+). Probed once, then cached.
fn supports_hierarchical_color() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImageSymbolConfiguration::class(),
            respondsToSelector: sel!(configurationWithHierarchicalColor:)
        ]
    })
}

/// Whether `+[NSImageSymbolConfiguration configurationWithPaletteColors:]`
/// is available (macOS 12.0+). Probed once, then cached.
fn supports_palette_colors() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImageSymbolConfiguration::class(),
            respondsToSelector: sel!(configurationWithPaletteColors:)
        ]
    })
}

/// Whether `+[NSImageSymbolConfiguration configurationPreferringMulticolor]`
/// is available (macOS 12.0+). Probed once, then cached.
fn supports_multicolor() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImageSymbolConfiguration::class(),
            respondsToSelector: sel!(configurationPreferringMulticolor)
        ]
    })
}

/// Whether `+[NSImageSymbolConfiguration configurationPreferringMonochrome]`
/// is available (macOS 13.0+). Probed once, then cached.
fn supports_monochrome() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImageSymbolConfiguration::class(),
            respondsToSelector: sel!(configurationPreferringMonochrome)
        ]
    })
}

/// Whether `+[NSImage imageWithSystemSymbolName:accessibilityDescription:]`
/// is available (macOS 11.0+). Probed once, then cached.
pub(crate) fn supports_system_symbols() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImage::class(),
            respondsToSelector: sel!(imageWithSystemSymbolName:accessibilityDescription:)
        ]
    })
}

/// Whether `+[NSImage imageWithSystemSymbolName:variableValue:accessibilityDescription:]`
/// is available (macOS 14.0+, Variable Color). Probed once, then cached.
fn supports_variable_value() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe {
        msg_send![
            NSImage::class(),
            respondsToSelector: sel!(imageWithSystemSymbolName:variableValue:accessibilityDescription:)
        ]
    })
}

/// Convert a [`SymbolColor`] to an `NSColor` (sRGB components, straight alpha).
fn to_nscolor(color: SymbolColor) -> Retained<NSColor> {
    NSColor::colorWithRed_green_blue_alpha(
        color.r as f64 / 255.0,
        color.g as f64 / 255.0,
        color.b as f64 / 255.0,
        color.a as f64 / 255.0,
    )
}

/// Render SF Symbol to RGBA pixel buffer.
///
/// Pixels are straight (non-premultiplied) alpha, in the sRGB color space.
///
/// # Returns
///
/// Returns `Some((width, height, rgba_data))` on success, `None` on failure
/// (unknown symbol, invalid parameters, or allocation failure).
pub(crate) fn render_sf_symbol(params: &RenderParams) -> Option<(u32, u32, Vec<u8>)> {
    let plain =
        params.rendering_mode == RenderingMode::Monochrome && params.variable_value.is_none();
    render(params, if plain { Draw::Outline } else { Draw::Image })
}

/// What draws the glyph.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Draw {
    /// The glyph's outline at its own proportions in one color where it has
    /// the shape AppKit draws (see [`Outline::matches`]), and AppKit's image
    /// elsewhere.
    Outline,
    /// AppKit's image.
    Image,
}

fn render(params: &RenderParams, draw: Draw) -> Option<(u32, u32, Vec<u8>)> {
    if !supports_system_symbols() {
        return None;
    }
    let point_size = params.point_size;
    let scale = params.scale;

    // Reject non-finite or absurd parameters up front. Without this, huge or
    // infinite values saturate the float->int casts and overflow the buffer
    // arithmetic below, or request multi-terabyte bitmaps from AppKit.
    if !point_size.is_finite()
        || !scale.is_finite()
        || point_size <= 0.0
        || point_size > MAX_POINT_SIZE
        || scale <= 0.0
        || scale > MAX_SCALE
    {
        return None;
    }

    autoreleasepool(|pool| {
        let image = symbol_image(params)?;

        // 5. Pixel dimensions. Inputs are bounded above, so these are finite,
        //    positive, and small enough that all buffer math below is exact.
        //    Rotated frames use a square canvas sized to the diagonal so the
        //    glyph never clips mid-rotation.
        let ns_size: NSSize = image.size();

        let base_width = (ns_size.width * scale as f64).ceil();
        let base_height = (ns_size.height * scale as f64).ceil();
        if !base_width.is_finite()
            || !base_height.is_finite()
            || base_width < 1.0
            || base_height < 1.0
            || base_width > MAX_PIXEL_DIMENSION
            || base_height > MAX_PIXEL_DIMENSION
        {
            return None;
        }
        let rotation = params
            .rotation_degrees
            .filter(|deg| deg.is_finite())
            .map(|deg| deg.rem_euclid(360.0));
        let (width, height) = match rotation {
            Some(_) => {
                let diagonal = (base_width * base_width + base_height * base_height)
                    .sqrt()
                    .ceil();
                if diagonal > MAX_PIXEL_DIMENSION {
                    return None;
                }
                (diagonal, diagonal)
            }
            None => (base_width, base_height),
        };

        // 5a. AppKit fits a glyph to whole pixels one axis at a time, which
        //     squashes it, so a monochrome glyph fills its outline at its own
        //     proportions instead, centred where AppKit centres the ink, with
        //     its bottom left corner on whole pixels as AppKit keeps it.
        let glyph = (draw == Draw::Outline)
            .then(|| vector_glyph(&image))
            .flatten();
        let outline = glyph
            .as_deref()
            .and_then(|glyph| Outline::of(glyph, pool))
            .filter(|outline| outline.matches(&image))
            .and_then(|outline| {
                let pixels = scale as f64;
                let centre = ink_centre(&image, base_width as usize, base_height as usize, pixels)?;
                let size = outline.size();
                let snap = |middle: f64, extent: f64| {
                    ((middle - extent / 2.0) * pixels).round() / pixels + extent / 2.0
                };
                let centre = NSPoint::new(snap(centre.x, size.width), snap(centre.y, size.height));
                Some((outline, centre))
            });

        // 5b. Sub-pixel move. AppKit resamples a symbol it draws off the
        //     pixel grid, which blurs it, so a glyph with an offset is drawn
        //     at FINE times the scale, where the move is whole pixels, and
        //     each block is averaged back down: the area coverage of the
        //     moved outline. AppKit snaps the glyph to the grid it draws on,
        //     so moves count from the fine drawing at offset zero.
        let quarter = |value: f32| {
            if value.is_finite() {
                (f64::from(value) * FINE as f64)
                    .round()
                    .rem_euclid(FINE as f64)
                    / FINE as f64
            } else {
                0.0
            }
        };
        let offset = match (rotation, params.offset) {
            (None, Some((x, y))) => Some((quarter(x), quarter(y))),
            _ => None,
        };
        let fine = if offset.is_some() { FINE } else { 1 };
        let offset = offset.unwrap_or((0.0, 0.0));
        let grow = |moved: f64| if moved > 0.0 { 1.0 } else { 0.0 };
        let (width, height) = (width + grow(offset.0), height + grow(offset.1));
        if width * fine as f64 > MAX_PIXEL_DIMENSION || height * fine as f64 > MAX_PIXEL_DIMENSION {
            return None;
        }
        let pixel_width = width as u32;
        let pixel_height = height as u32;
        let (fine_width, fine_height) = (pixel_width as usize * fine, pixel_height as usize * fine);
        // The fine size is <= 16_384 a side, so this cannot overflow usize.
        let data_len = fine_width * 4 * fine_height;

        // 6. The bitmap, in points at `scale` (see `bitmap`).
        let bitmap_rep = bitmap(fine_width, fine_height, scale as f64 * fine as f64)?;
        let canvas_points = NSSize {
            width: width / scale as f64,
            height: height / scale as f64,
        };

        // 7. Draw. The bitmap starts transparent.
        //
        // For rotated frames, apply a rotate-about-center transform and draw
        // the glyph centered on the (square, diagonal-sized) canvas.
        // Every rect below is in POINTS now that the rep carries a point size:
        // the context maps them onto the device grid at `scale` for us.
        draw_into(&bitmap_rep, |context| {
            let draw_rect = match rotation {
                Some(deg) => {
                    let transform = NSAffineTransform::transform();
                    transform
                        .translateXBy_yBy(canvas_points.width / 2.0, canvas_points.height / 2.0);
                    // Negative degrees: clockwise on screen (AppKit's positive
                    // rotation is counterclockwise in its coordinate space).
                    transform.rotateByDegrees(-deg);
                    transform.concat();
                    NSRect {
                        origin: NSPoint {
                            x: -ns_size.width / 2.0,
                            y: -ns_size.height / 2.0,
                        },
                        size: ns_size,
                    }
                }
                // The glyph sits where AppKit draws it, at its own size, moved
                // right and down by the offset. AppKit's y axis points up, so
                // a grown canvas lifts the glyph by a pixel less the move.
                None => NSRect {
                    origin: NSPoint {
                        x: offset.0 / scale as f64,
                        y: (grow(offset.1) - offset.1) / scale as f64,
                    },
                    size: ns_size,
                },
            };
            if let Some((outline, centre)) = &outline {
                let origin = draw_rect.origin;
                let centre = NSPoint::new(origin.x + centre.x, origin.y + centre.y);
                return outline.fill(context, centre, (1.0, 1.0), params.color);
            }
            draw_image(&image, draw_rect.origin);

            // 7b. Monochrome tint: recolor the drawn glyph uniformly,
            // preserving only its alpha. SourceIn touches nothing outside the
            // glyph, and filling `draw_rect` covers the glyph in the current
            // (possibly rotated) coordinate space.
            if params.rendering_mode == RenderingMode::Monochrome {
                to_nscolor(params.color).setFill();
                objc2_app_kit::NSRectFillUsingOperation(
                    draw_rect,
                    NSCompositingOperation::SourceIn,
                );
            }
            Some(())
        })
        .flatten()?;

        // 8. Copy pixel data out.
        let mut buffer = pixels(&bitmap_rep, data_len)?;

        if fine > 1 {
            buffer = downsample(&buffer, pixel_width as usize, pixel_height as usize, fine);
        }

        // 9. Convert premultiplied -> straight alpha (GPUI's Metal renderer,
        //    and this function's documented contract, expect straight alpha).
        premultiplied_to_straight(&mut buffer);

        Some((pixel_width, pixel_height, buffer))
    })
}

/// The symbol image `params` names, configured with its Variable Color
/// value, point size, weight, scale and colors.
fn symbol_image(params: &RenderParams) -> Option<Retained<NSImage>> {
    // 1. Create NSImage with system symbol name, honoring Variable Color
    //    (macOS 14+) when requested and available; otherwise fall back to
    //    the plain symbol.
    let ns_name = NSString::from_str(params.name);
    let symbol_image = match params.variable_value {
        Some(value) if supports_variable_value() => {
            NSImage::imageWithSystemSymbolName_variableValue_accessibilityDescription(
                &ns_name, value, None,
            )?
        }
        _ => NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns_name, None)?,
    };

    // 2. Size/weight/scale configuration (available since macOS 11.0)
    let size_config = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(
        params.point_size as f64,
        params.weight.to_ns_weight(),
        NSImageSymbolScale(params.symbol_scale.to_ns_scale() as isize),
    );

    // 3. Color configuration, guarded by API availability so older macOS
    //    degrades gracefully (uncolored) instead of crashing on an
    //    unrecognized selector.
    let color_config: Option<Retained<NSImageSymbolConfiguration>> = match params.rendering_mode {
        RenderingMode::Monochrome => {
            // No color configuration at all: composing a hierarchical
            // tint over configurationPreferringMonochrome re-enables
            // hierarchical rendering (the applied configuration wins), so
            // secondary layers — person.2's back figure, gearshape.2's
            // second gear — came out faded instead of flat. The symbol is
            // drawn in its default (black) monochrome form and tinted
            // AFTER drawing with a SourceIn fill (step 7b), which
            // recolors every layer uniformly, matching how AppKit treats
            // template images.
            supports_monochrome()
                .then(NSImageSymbolConfiguration::configurationPreferringMonochrome)
        }
        RenderingMode::Hierarchical => supports_hierarchical_color().then(|| {
            NSImageSymbolConfiguration::configurationWithHierarchicalColor(&to_nscolor(
                params.color,
            ))
        }),
        RenderingMode::Palette => {
            if !params.palette.is_empty() && supports_palette_colors() {
                // Up to 3 palette colors: primary, secondary, tertiary.
                let colors: Vec<Retained<NSColor>> = params
                    .palette
                    .iter()
                    .take(MAX_PALETTE_COLORS)
                    .map(|&c| to_nscolor(c))
                    .collect();
                let palette = NSArray::from_retained_slice(&colors);
                Some(NSImageSymbolConfiguration::configurationWithPaletteColors(
                    &palette,
                ))
            } else {
                // Without palette colors, Palette intentionally behaves
                // like Hierarchical (single primary color).
                supports_hierarchical_color().then(|| {
                    NSImageSymbolConfiguration::configurationWithHierarchicalColor(&to_nscolor(
                        params.color,
                    ))
                })
            }
        }
        RenderingMode::Multicolor => {
            // Multicolor uses the symbol's built-in colors; `color` is ignored.
            supports_multicolor()
                .then(NSImageSymbolConfiguration::configurationPreferringMulticolor)
        }
    };

    let combined_config = match &color_config {
        Some(config) => size_config.configurationByApplyingConfiguration(config),
        None => size_config,
    };

    // 4. Apply configuration
    Some(
        symbol_image
            .imageWithSymbolConfiguration(&combined_config)
            .unwrap_or(symbol_image),
    )
}

/// The vector glyph behind a symbol image, when it answers `CGPath`,
/// `scale` and `monochromeLayers`.
///
/// These are private API: `-[NSSymbolImageRep vectorGlyph]`, CoreUI's
/// `CUINamedVectorGlyph` (`CGPath`, `scale`, `monochromeLayers`) and
/// `CUIVectorGlyphLayer` (`isEraserLayer`), for macOS 11, where AppKit
/// gained symbol images, to macOS 27. Only macOS 27 is checked. Where one
/// is missing, or the path is empty, the symbol keeps AppKit's image draw.
fn vector_glyph(image: &NSImage) -> Option<Retained<AnyObject>> {
    let rep = image.representations().firstObject()?;
    if !rep.class().responds_to(sel!(vectorGlyph)) {
        return None;
    }
    // SAFETY: `vectorGlyph` takes no arguments and returns an object or nil.
    let glyph: Option<Retained<AnyObject>> = unsafe { msg_send![&*rep, vectorGlyph] };
    let glyph = glyph?;
    let class = glyph.class();
    [sel!(CGPath), sel!(scale), sel!(monochromeLayers)]
        .into_iter()
        .all(|selector| class.responds_to(selector))
        .then_some(glyph)
}

/// A glyph's outline: the union of its layers, in pixels at the glyph's own
/// `scale` with y pointing down.
struct Outline<'a> {
    glyph: &'a AnyObject,
    path: &'a CgPath,
    bounds: NSRect,
    scale: f64,
}

impl<'a> Outline<'a> {
    /// The outline of a glyph from [`vector_glyph`], when it is not empty.
    fn of(glyph: &'a AnyObject, _pool: AutoreleasePool<'a>) -> Option<Self> {
        // SAFETY: `vector_glyph` checked that the glyph answers `CGPath` and
        // `scale`, which take no arguments and return a path or null and a
        // double. The path is autoreleased into `pool`, which outlives 'a.
        let (path, scale) = unsafe {
            let path: *const CgPath = msg_send![glyph, CGPath];
            let scale: f64 = msg_send![glyph, scale];
            (path.as_ref()?, scale)
        };
        let bounds = CGPathGetPathBoundingBox(path);
        let positive = |value: f64| value.is_finite() && value > 0.0;
        (positive(scale)
            && positive(bounds.size.width)
            && positive(bounds.size.height)
            && bounds.origin.x.is_finite()
            && bounds.origin.y.is_finite())
        .then_some(Self {
            glyph,
            path,
            bounds,
            scale,
        })
    }

    /// The size of the outline's bounds in points.
    fn size(&self) -> NSSize {
        NSSize::new(
            self.bounds.size.width / self.scale,
            self.bounds.size.height / self.scale,
        )
    }

    /// Whether the outline has the shape AppKit draws: no layer erases
    /// another, and once stretched to the proportions of AppKit's ink at
    /// [`CHECK_SCALE`], it shares at least [`MIN_OVERLAP`] of it.
    fn matches(&self, image: &NSImage) -> bool {
        !self.has_eraser_layers()
            && self
                .overlap(image)
                .is_some_and(|share| share >= MIN_OVERLAP)
    }

    /// Whether any monochrome layer erases the layers under it, which the
    /// outline does not show. Unknown layers count as erasers.
    fn has_eraser_layers(&self) -> bool {
        // SAFETY: `vector_glyph` checked that the glyph answers
        // `monochromeLayers`, which takes no arguments and returns an array
        // or nil.
        let layers: Option<Retained<NSArray<AnyObject>>> =
            unsafe { msg_send![self.glyph, monochromeLayers] };
        let Some(layers) = layers else {
            return true;
        };
        (0..layers.count()).any(|index| {
            let layer = layers.objectAtIndex(index);
            if !layer.class().responds_to(sel!(isEraserLayer)) {
                return true;
            }
            // SAFETY: the layer answers `isEraserLayer`, which takes no
            // arguments and returns a BOOL.
            unsafe { msg_send![&*layer, isEraserLayer] }
        })
    }

    /// How much ink AppKit's drawing of `image` and the outline share, as
    /// intersection over union, once the outline has the mass, middle and
    /// spread of AppKit's ink on each axis.
    fn overlap(&self, image: &NSImage) -> Option<f64> {
        let size = image.size();
        let margin = 1.0;
        let width = ((size.width + 2.0 * margin) * CHECK_SCALE).ceil() as usize;
        let height = ((size.height + 2.0 * margin) * CHECK_SCALE).ceil() as usize;
        let top = height as f64 / CHECK_SCALE;
        let canvas = |draw: &dyn Fn(&NSGraphicsContext) -> Option<()>| {
            let rep = bitmap(width, height, CHECK_SCALE)?;
            draw_into(&rep, draw).flatten()?;
            pixels(&rep, width * height * 4)
        };
        let black = SymbolColor {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };

        let native = canvas(&|_| {
            draw_image(image, NSPoint::new(margin, margin));
            Some(())
        })?;
        let middle = NSPoint::new(width as f64 / CHECK_SCALE / 2.0, top / 2.0);
        let plain = canvas(&|context| self.fill(context, middle, (1.0, 1.0), black))?;

        let [_, x, y, spread_x, spread_y] = moments(&native, width)?;
        let [_, plain_x, plain_y, plain_spread_x, plain_spread_y] = moments(&plain, width)?;
        let stretch = (spread_x / plain_spread_x, spread_y / plain_spread_y);
        if !stretch.0.is_finite() || !stretch.1.is_finite() {
            return None;
        }
        let (middle_x, middle_y) = (middle.x * CHECK_SCALE, (top - middle.y) * CHECK_SCALE);
        let centre = NSPoint::new(
            (x - (plain_x - middle_x) * stretch.0) / CHECK_SCALE,
            top - (y - (plain_y - middle_y) * stretch.1) / CHECK_SCALE,
        );
        let fitted = canvas(&|context| self.fill(context, centre, stretch, black))?;
        Some(overlap(&native, &fitted))
    }

    /// Fills the outline in `color` with the middle of its bounds at
    /// `centre`, in points of the current context with y pointing up, at
    /// its own size times `stretch` across and down.
    fn fill(
        &self,
        context: &NSGraphicsContext,
        centre: NSPoint,
        stretch: (f64, f64),
        color: SymbolColor,
    ) -> Option<()> {
        // SAFETY: `CGContext` takes no arguments and returns the context's
        // CoreGraphics context or null, which lives as long as `context`.
        let cg = unsafe {
            let cg: *const CgContext = msg_send![context, CGContext];
            cg.as_ref()
        }?;
        let bounds = self.bounds;
        let transform = NSAffineTransform::transform();
        transform.translateXBy_yBy(centre.x, centre.y);
        transform.scaleXBy_yBy(stretch.0 / self.scale, -stretch.1 / self.scale);
        transform.translateXBy_yBy(
            -(bounds.origin.x + bounds.size.width / 2.0),
            -(bounds.origin.y + bounds.size.height / 2.0),
        );

        context.saveGraphicsState();
        transform.concat();
        to_nscolor(color).setFill();
        CGContextAddPath(cg, self.path);
        CGContextFillPath(cg);
        context.restoreGraphicsState();
        Some(())
    }
}

/// A transparent RGBA bitmap `width` by `height` pixels that AppKit draws
/// into in points, at `scale` pixels per point.
///
/// The point size matters: a rep made from pixel sizes alone has a backing
/// scale of 1, so a 13pt symbol asked for at scale 2 would be drawn as a
/// 26pt symbol at 1x. SF Symbols are optically sized, so those are not the
/// same picture: strokes, terminals and counters are redrawn for the
/// larger size.
///
/// AppKit's contexts cannot draw into non-premultiplied bitmaps, so the
/// pixels are premultiplied; [`premultiplied_to_straight`] converts them.
fn bitmap(width: usize, height: usize, scale: f64) -> Option<Retained<NSBitmapImageRep>> {
    // SAFETY: `NSDeviceRGBColorSpace` is an immutable string constant.
    let color_space_name = unsafe { objc2_app_kit::NSDeviceRGBColorSpace };
    // SAFETY: with null planes the rep allocates and owns zeroed pixels of
    // the size the other arguments describe.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            color_space_name,
            NSBitmapFormat::empty(),
            (width * 4) as isize,
            32,
        )
    }?;
    rep.setSize(NSSize {
        width: width as f64 / scale,
        height: height as f64 / scale,
    });
    Some(rep)
}

/// Runs `draw` with a context on `rep` as the current graphics context.
fn draw_into<R>(rep: &NSBitmapImageRep, draw: impl FnOnce(&NSGraphicsContext) -> R) -> Option<R> {
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(rep)?;
    let old_context = NSGraphicsContext::currentContext();
    NSGraphicsContext::setCurrentContext(Some(&context));
    let result = draw(&context);
    NSGraphicsContext::setCurrentContext(old_context.as_deref());
    Some(result)
}

/// Draws `image` at its own size with its bottom left corner at `origin`.
fn draw_image(image: &NSImage, origin: NSPoint) {
    let size = image.size();
    image.drawInRect_fromRect_operation_fraction(
        NSRect { origin, size },
        NSRect {
            origin: NSPoint::new(0.0, 0.0),
            size,
        },
        NSCompositingOperation::SourceOver,
        1.0,
    );
}

/// A copy of the first `len` bytes of pixels in `rep`.
fn pixels(rep: &NSBitmapImageRep, len: usize) -> Option<Vec<u8>> {
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: the rep owns at least `len` bytes at `data` and outlives the
    // copy.
    Some(unsafe { std::slice::from_raw_parts(data, len) }.to_vec())
}

/// Where AppKit centres the ink of `image` drawn at `scale` on a `width` by
/// `height` pixel canvas: the middle of [`ink_box`], in points from the
/// bottom left.
fn ink_centre(image: &NSImage, width: usize, height: usize, scale: f64) -> Option<NSPoint> {
    let rep = bitmap(width, height, scale)?;
    draw_into(&rep, |_| draw_image(image, NSPoint::new(0.0, 0.0)))?;
    let [left, top, right, bottom] = ink_box(&pixels(&rep, width * height * 4)?, width, height)?;
    Some(NSPoint::new(
        (left + right) / 2.0 / scale,
        (height as f64 - (top + bottom) / 2.0) / scale,
    ))
}

/// The edges of the ink in a `width` by `height` RGBA raster as `[left,
/// top, right, bottom]`, in pixels from the top left. Each edge moves in by
/// how faint the outermost line of ink is.
fn ink_box(data: &[u8], width: usize, height: usize) -> Option<[f64; 4]> {
    let alpha = |x: usize, y: usize| data[(y * width + x) * 4 + 3] as f64 / 255.0;
    let column = |x: usize| (0..height).map(|y| alpha(x, y)).fold(0.0, f64::max);
    let row = |y: usize| (0..width).map(|x| alpha(x, y)).fold(0.0, f64::max);
    let left = (0..width).find(|x| column(*x) > 0.0)?;
    let top = (0..height).find(|y| row(*y) > 0.0)?;
    let right = (0..width).rev().find(|x| column(*x) > 0.0)?;
    let bottom = (0..height).rev().find(|y| row(*y) > 0.0)?;
    Some([
        left as f64 + 1.0 - column(left),
        top as f64 + 1.0 - row(top),
        right as f64 + column(right),
        bottom as f64 + row(bottom),
    ])
}

/// The ink of a `width` pixels wide RGBA raster as `[mass, x, y, spread x,
/// spread y]`: its alpha sum, middle and standard deviation in pixels from
/// the top left, each pixel weighted by its alpha.
fn moments(data: &[u8], width: usize) -> Option<[f64; 5]> {
    let mut sums = [0.0; 5];
    for (index, pixel) in data.chunks_exact(4).enumerate() {
        let alpha = f64::from(pixel[3]);
        let (x, y) = ((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
        for (sum, value) in sums.iter_mut().zip([1.0, x, y, x * x, y * y]) {
            *sum += alpha * value;
        }
    }
    let [mass, x, y, xx, yy] = sums;
    if mass <= 0.0 {
        return None;
    }
    let (x, y) = (x / mass, y / mass);
    let spread = |squares: f64, middle: f64| (squares / mass - middle * middle).max(0.0).sqrt();
    Some([mass, x, y, spread(xx, x), spread(yy, y)])
}

/// The alpha two same-sized RGBA rasters share over the alpha either has.
fn overlap(a: &[u8], b: &[u8]) -> f64 {
    let (both, either) =
        a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .fold((0u64, 0u64), |(both, either), (a, b)| {
                (
                    both + u64::from(a[3].min(b[3])),
                    either + u64::from(a[3].max(b[3])),
                )
            });
    if either == 0 {
        return 0.0;
    }
    both as f64 / either as f64
}

/// Averages each `fine` by `fine` block of a premultiplied raster `fine`
/// times as wide and tall as `width` by `height`.
fn downsample(data: &[u8], width: usize, height: usize, fine: usize) -> Vec<u8> {
    let row = width * fine * 4;
    let count = (fine * fine) as u32;
    let mut out = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            for dy in 0..fine {
                let start = (y * fine + dy) * row + x * fine * 4;
                for pixel in data[start..start + fine * 4].chunks_exact(4) {
                    for (total, value) in sum.iter_mut().zip(pixel) {
                        *total += u32::from(*value);
                    }
                }
            }
            for (channel, total) in out[(y * width + x) * 4..][..4].iter_mut().zip(sum) {
                *channel = ((total + count / 2) / count) as u8;
            }
        }
    }
    out
}

/// Convert premultiplied RGBA to straight (non-premultiplied) RGBA in place,
/// dividing the color channels out by alpha (same approach as GPUI's own
/// `swap_rgba_pa_to_bgra` helper).
fn premultiplied_to_straight(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let a = pixel[3];
        if a > 0 && a < 255 {
            let a = a as f32 / 255.0;
            pixel[0] = (pixel[0] as f32 / a).min(255.0) as u8;
            pixel[1] = (pixel[1] as f32 / a).min(255.0) as u8;
            pixel[2] = (pixel[2] as f32 / a).min(255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 3] = ["chevron.down", "checkmark", "xmark"];

    const SIZES: [f32; 6] = [7.5, 9.0, 10.5, 12.0, 13.5, 16.0];

    const WHITE: SymbolColor = SymbolColor {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    type Raster = (Vec<u8>, (usize, usize));

    fn params(name: &str, point_size: f32, scale: f32) -> RenderParams<'_> {
        RenderParams {
            name,
            point_size,
            scale,
            color: WHITE,
            palette: &[],
            weight: SymbolWeight::Regular,
            symbol_scale: SymbolScale::Medium,
            rendering_mode: RenderingMode::Monochrome,
            variable_value: None,
            rotation_degrees: None,
            offset: None,
        }
    }

    fn raster(params: &RenderParams) -> Raster {
        let (width, height, data) = render_sf_symbol(params).unwrap();
        (data, (width as usize, height as usize))
    }

    fn image_raster(params: &RenderParams) -> Raster {
        let (width, height, data) = render(params, Draw::Image).unwrap();
        (data, (width as usize, height as usize))
    }

    /// AppKit's own drawing of the symbol, untinted.
    fn native(params: &RenderParams) -> Raster {
        autoreleasepool(|_| {
            let image = symbol_image(params).unwrap();
            let size = image.size();
            let scale = f64::from(params.scale);
            let width = (size.width * scale).ceil() as usize;
            let height = (size.height * scale).ceil() as usize;
            let rep = bitmap(width, height, scale).unwrap();
            draw_into(&rep, |_| draw_image(&image, NSPoint::new(0.0, 0.0))).unwrap();
            (pixels(&rep, width * height * 4).unwrap(), (width, height))
        })
    }

    /// The width and height of the symbol's outline in device pixels.
    fn outline_size(params: &RenderParams) -> (f64, f64) {
        autoreleasepool(|pool| {
            let image = symbol_image(params).unwrap();
            let glyph = vector_glyph(&image).unwrap();
            let outline = Outline::of(&glyph, pool).unwrap();
            let pixels = f64::from(params.scale) / outline.scale;
            (
                outline.bounds.size.width * pixels,
                outline.bounds.size.height * pixels,
            )
        })
    }

    fn ink(raster: &Raster) -> [f64; 4] {
        let (data, (width, height)) = raster;
        ink_box(data, *width, *height).unwrap()
    }

    /// The middle of the ink, each pixel weighted by its alpha.
    fn ink_centroid(raster: &Raster) -> (f64, f64) {
        let [_, x, y, _, _] = moments(&raster.0, raster.1.0).unwrap();
        (x, y)
    }

    /// How far ink spreads from its middle across and down: its standard
    /// deviation in pixels, each pixel weighted by its alpha. A pixel holds
    /// the cover of its whole square, which adds a twelfth to the variance.
    fn spread(data: &[u8], width: usize) -> (f64, f64) {
        let [_, _, _, x, y] = moments(data, width).unwrap();
        let unblur = |spread: f64| (spread * spread - 1.0 / 12.0).sqrt();
        (unblur(x), unblur(y))
    }

    /// The [`spread`] of the symbol's outline in device pixels, measured on
    /// a fill at eight times the scale.
    fn outline_spread(params: &RenderParams) -> (f64, f64) {
        autoreleasepool(|pool| {
            let image = symbol_image(params).unwrap();
            let glyph = vector_glyph(&image).unwrap();
            let outline = Outline::of(&glyph, pool).unwrap();
            let fine = 8.0;
            let scale = f64::from(params.scale) * fine;
            let size = outline.size();
            let width = ((size.width + 2.0) * scale).ceil() as usize;
            let height = ((size.height + 2.0) * scale).ceil() as usize;
            let rep = bitmap(width, height, scale).unwrap();
            let middle = NSPoint::new(width as f64 / scale / 2.0, height as f64 / scale / 2.0);
            draw_into(&rep, |context| {
                outline.fill(context, middle, (1.0, 1.0), WHITE)
            })
            .flatten()
            .unwrap();
            let (x, y) = spread(&pixels(&rep, width * height * 4).unwrap(), width);
            (x / fine, y / fine)
        })
    }

    /// An offset moves the ink right and down by that fraction of a device
    /// pixel from where an offset of zero draws it, without blurring it, and
    /// the canvas grows by a pixel on each axis that moves.
    #[test]
    fn an_offset_moves_the_ink_by_a_fraction_of_a_pixel() {
        if !supports_system_symbols() {
            return;
        }
        let sharpness = |data: &[u8]| {
            let alpha = data.chunks_exact(4).map(|p| f64::from(p[3]) / 255.0);
            let (sum, squares) = alpha.fold((0.0, 0.0), |(sum, sq), a| (sum + a, sq + a * a));
            squares / sum
        };
        for name in NAMES {
            for scale in [1.0, 2.0] {
                let raster = |offset| {
                    raster(&RenderParams {
                        offset,
                        ..params(name, 13.0, scale)
                    })
                };
                let (direct, direct_size) = raster(None);
                let base = raster(Some((0.0, 0.0)));
                let (width, height) = base.1;
                assert_eq!(
                    (width, height),
                    direct_size,
                    "{name} {scale}x: canvas at zero"
                );
                for (x, y) in [(0.25, 0.0), (0.0, 0.5), (0.75, 0.25), (1.5, -0.25)] {
                    let moved = raster(Some((x, y)));
                    let (dx, dy) = (f64::from(x).rem_euclid(1.0), f64::from(y).rem_euclid(1.0));
                    let grown = |moved: f64| usize::from(moved > 0.0);
                    assert_eq!(
                        moved.1,
                        (width + grown(dx), height + grown(dy)),
                        "{name} {scale}x {x},{y}: canvas"
                    );
                    let (bx, by) = ink_centroid(&base);
                    let (mx, my) = ink_centroid(&moved);
                    assert!(
                        (mx - bx - dx).abs() <= 0.05 && (my - by - dy).abs() <= 0.05,
                        "{name} {scale}x {x},{y}: the ink moved by {}, {}",
                        mx - bx,
                        my - by
                    );
                    assert!(
                        (sharpness(&moved.0) - sharpness(&direct)).abs() <= 0.04,
                        "{name} {scale}x {x},{y}: the moved edges are as sharp, {} against {}",
                        sharpness(&moved.0),
                        sharpness(&direct)
                    );
                }
            }
        }
    }

    /// The ink has the size of the symbol's outline, in the canvas AppKit
    /// draws on, the right way up, and centred within 0.6 device pixels of
    /// AppKit's ink: putting the outline's corner on whole pixels moves it
    /// by up to half a pixel.
    #[test]
    fn ink_is_the_outline_at_the_native_centre() {
        if !supports_system_symbols() {
            return;
        }
        let sizes = [9.0, 10.0, 10.5, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0];
        for symbol_scale in [SymbolScale::Medium, SymbolScale::Small] {
            for name in NAMES {
                for scale in [1.0, 2.0] {
                    for size in sizes {
                        let params = RenderParams {
                            symbol_scale,
                            ..params(name, size, scale)
                        };
                        let label = format!("{name} {size}pt {scale}x {symbol_scale:?}");
                        let (ours, native) = (raster(&params), native(&params));
                        assert_eq!(ours.1, native.1, "{label}: canvas");

                        let (width, height) = outline_size(&params);
                        let (outline_x, outline_y) = outline_spread(&params);
                        let (ink_x, ink_y) = spread(&ours.0, ours.1.0);
                        let (dw, dh) = (
                            width * (ink_x / outline_x - 1.0),
                            height * (ink_y / outline_y - 1.0),
                        );
                        assert!(
                            dw.abs() <= 0.2 && dh.abs() <= 0.2,
                            "{label}: the ink is {dw:.2}, {dh:.2} off the outline's size"
                        );

                        let ([l, t, r, b], [nl, nt, nr, nb]) = (ink(&ours), ink(&native));
                        let (dx, dy) = ((l + r - nl - nr) / 2.0, (t + b - nt - nb) / 2.0);
                        assert!(
                            dx.abs() <= 0.6 && dy.abs() <= 0.6,
                            "{label}: the ink centre moved by {dx:.2}, {dy:.2}"
                        );

                        if name != "xmark" {
                            let rows = ours.0.chunks_exact(ours.1.0 * 4);
                            let flipped: Vec<u8> = rows.rev().flatten().copied().collect();
                            assert!(
                                overlap(&ours.0, &native.0) > overlap(&flipped, &native.0),
                                "{label}: upside down"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The xmark's ink is as wide as it is tall, as its outline is, at the
    /// sizes and scales inkui draws it at.
    #[test]
    fn xmark_ink_is_square() {
        if !supports_system_symbols() {
            return;
        }
        for weight in [SymbolWeight::Regular, SymbolWeight::Medium] {
            for size in SIZES {
                for scale in [1.0, 1.5, 2.0] {
                    let [l, t, r, b] = ink(&raster(&RenderParams {
                        weight,
                        ..params("xmark", size, scale)
                    }));
                    assert!(
                        ((r - l) - (b - t)).abs() <= 0.25,
                        "xmark {size}pt {scale}x {weight:?}: ink {:.2}x{:.2}",
                        r - l,
                        b - t
                    );
                }
            }
        }
    }

    /// The ink has the proportions of the symbol's outline: the height its
    /// [`spread`] gives for its width is within 0.25 device pixels of the
    /// outline's height. The spread, unlike the ink box, does not change
    /// with where the edges fall inside a pixel.
    #[test]
    fn ink_keeps_the_outline_proportions() {
        if !supports_system_symbols() {
            return;
        }
        let names = [
            "chevron.up",
            "chevron.down",
            "magnifyingglass",
            "xmark.circle.fill",
            "checkmark",
            "plus",
        ];
        for weight in [SymbolWeight::Regular, SymbolWeight::Medium] {
            for name in names {
                for size in SIZES {
                    for scale in [1.0, 1.5, 2.0] {
                        let params = RenderParams {
                            weight,
                            ..params(name, size, scale)
                        };
                        let ours = raster(&params);
                        let (ink_x, ink_y) = spread(&ours.0, ours.1.0);
                        let (outline_x, outline_y) = outline_spread(&params);
                        let (_, height) = outline_size(&params);
                        let off = height * ((ink_y / ink_x) / (outline_y / outline_x) - 1.0);
                        assert!(
                            off.abs() <= 0.25,
                            "{name} {size}pt {scale}x {weight:?}: the ink is {off:.2} too tall"
                        );
                    }
                }
            }
        }
    }

    /// The same inputs give the same bytes, whatever was drawn before and
    /// on whichever thread.
    #[test]
    fn same_inputs_give_same_bytes() {
        if !supports_system_symbols() {
            return;
        }
        let cases = [
            params("xmark", 9.0, 1.5),
            params("magnifyingglass", 13.5, 2.0),
            RenderParams {
                offset: Some((0.25, 0.5)),
                ..params("chevron.down", 12.0, 1.0)
            },
        ];
        let first: Vec<_> = cases.iter().map(raster).collect();
        for case in &cases {
            for scale in [1.0, 2.0, 3.0] {
                for point_size in [case.point_size - 1.0, case.point_size + 1.0] {
                    raster(&RenderParams {
                        scale,
                        point_size,
                        ..*case
                    });
                }
            }
        }
        for (case, first) in cases.iter().zip(&first) {
            assert!(raster(case) == *first, "{} again", case.name);
            let name = case.name.to_owned();
            let (size, scale, offset) = (case.point_size, case.scale, case.offset);
            let elsewhere = std::thread::spawn(move || {
                raster(&RenderParams {
                    offset,
                    ..params(&name, size, scale)
                })
            });
            assert!(
                elsewhere.join().unwrap() == *first,
                "{} on another thread",
                case.name
            );
        }
    }

    /// An image without a vector glyph has no outline, a symbol whose
    /// outline lacks the shape AppKit draws keeps AppKit's image draw, and
    /// that draw still puts the ink where AppKit does.
    #[test]
    fn symbols_without_a_matching_outline_keep_the_image_draw() {
        if !supports_system_symbols() {
            return;
        }
        autoreleasepool(|_| {
            let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(4.0, 4.0));
            image.addRepresentation(&bitmap(4, 4, 1.0).unwrap());
            assert!(vector_glyph(&image).is_none());
        });
        let names = [
            "xmark.circle.fill",
            "bell.badge",
            "circle.lefthalf.filled",
            "wand.and.stars",
        ];
        for name in names {
            let params = params(name, 12.0, 2.0);
            assert!(raster(&params) == image_raster(&params), "{name}");
        }
        for name in NAMES {
            for scale in [1.0, 1.5, 2.0] {
                let params = params(name, 12.0, scale);
                let (image, native) = (image_raster(&params), native(&params));
                assert_eq!(image.1, native.1, "{name} {scale}x: canvas");
                assert_eq!(ink(&image), ink(&native), "{name} {scale}x: ink");
            }
        }
    }

    /// The outline keeps the weight, symbol scale and color of the image
    /// draw: about as much ink, in its color up to the rounding of 8-bit
    /// premultiplied pixels.
    #[test]
    fn weight_scale_and_color_carry_over() {
        if !supports_system_symbols() {
            return;
        }
        let color = SymbolColor {
            r: 200,
            g: 40,
            b: 10,
            a: 128,
        };
        let weights = [
            SymbolWeight::UltraLight,
            SymbolWeight::Regular,
            SymbolWeight::Black,
        ];
        let scales = [SymbolScale::Small, SymbolScale::Medium, SymbolScale::Large];
        for weight in weights {
            for symbol_scale in scales {
                let params = RenderParams {
                    weight,
                    symbol_scale,
                    color,
                    ..params("xmark", 13.5, 2.0)
                };
                let label = format!("{weight:?} {symbol_scale:?}");
                let (ours, image) = (raster(&params), image_raster(&params));
                let mass = |raster: &Raster| moments(&raster.0, raster.1.0).unwrap()[0];
                let ratio = mass(&ours) / mass(&image);
                assert!(
                    (ratio - 1.0).abs() <= 0.1,
                    "{label}: ink mass ratio {ratio:.3}"
                );

                let alpha = ours.0.chunks_exact(4).map(|p| p[3]).max().unwrap();
                assert!(alpha <= color.a, "{label}: alpha {alpha}");
                for pixel in ours.0.chunks_exact(4).filter(|p| p[3] >= 64) {
                    let rounding = 1 + 255 / pixel[3];
                    let off = [color.r, color.g, color.b]
                        .iter()
                        .zip(pixel)
                        .any(|(want, got)| want.abs_diff(*got) > rounding);
                    assert!(!off, "{label}: pixel {pixel:?} against {color:?}");
                }
            }
        }
    }
}
