//! macOS platform implementation using AppKit (via objc2).
//!
//! Memory management:
//! - Every AppKit object is held in a [`Retained`], which releases on drop.
//! - The whole render runs inside an `NSAutoreleasePool`, so the autoreleased
//!   temporaries AppKit creates are drained on every call, on any thread —
//!   including plain `std::thread`s that have no pool of their own.

use std::sync::OnceLock;

use objc2::rc::{Retained, autoreleasepool};
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

/// Maximum number of palette colors Apple uses (primary, secondary, tertiary).
pub(crate) const MAX_PALETTE_COLORS: usize = 3;

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

    autoreleasepool(|_pool| {
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
            point_size as f64,
            params.weight.to_ns_weight(),
            NSImageSymbolScale(params.symbol_scale.to_ns_scale() as isize),
        );

        // 3. Color configuration, guarded by API availability so older macOS
        //    degrades gracefully (uncolored) instead of crashing on an
        //    unrecognized selector.
        let color_config: Option<Retained<NSImageSymbolConfiguration>> = match params.rendering_mode
        {
            RenderingMode::Monochrome => {
                let symbol_color = to_nscolor(params.color);
                if supports_monochrome() && supports_hierarchical_color() {
                    // Prefer the monochrome variant, tinted by the color.
                    let mono = NSImageSymbolConfiguration::configurationPreferringMonochrome();
                    let tint = NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                        &symbol_color,
                    );
                    Some(mono.configurationByApplyingConfiguration(&tint))
                } else if supports_hierarchical_color() {
                    // macOS 12: no monochrome-preferring configuration yet.
                    Some(
                        NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                            &symbol_color,
                        ),
                    )
                } else {
                    None
                }
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
        let image = symbol_image
            .imageWithSymbolConfiguration(&combined_config)
            .unwrap_or(symbol_image);

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
        let pixel_width = width as u32;
        let pixel_height = height as u32;
        let bytes_per_row = pixel_width as usize * 4;
        // pixel_width/pixel_height <= 16_384, so this cannot overflow usize.
        let data_len = bytes_per_row * pixel_height as usize;

        // 6. Create NSBitmapImageRep for drawing.
        //
        // Note on alpha: AppKit's graphics contexts cannot draw into
        // non-premultiplied bitmaps (context creation fails), so the bitmap
        // uses the default *premultiplied* format and we convert to straight
        // alpha ourselves after copying the pixels out (see step 9).
        let color_space_name = unsafe { objc2_app_kit::NSDeviceRGBColorSpace };
        let bitmap_rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                pixel_width as isize,
                pixel_height as isize,
                8,
                4,
                true,
                false,
                color_space_name,
                NSBitmapFormat::empty(),
                bytes_per_row as isize,
                32,
            )
        }?;

        // 7. Create NSGraphicsContext and draw
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap_rep)?;
        let old_context = NSGraphicsContext::currentContext();
        NSGraphicsContext::setCurrentContext(Some(&context));

        // Transparent background - the bitmap is zero-initialized.
        //
        // For rotated frames, apply a rotate-about-center transform and draw
        // the glyph centered on the (square, diagonal-sized) canvas.
        let draw_rect = match rotation {
            Some(deg) => {
                let transform = NSAffineTransform::transform();
                transform.translateXBy_yBy(pixel_width as f64 / 2.0, pixel_height as f64 / 2.0);
                // Negative degrees: clockwise on screen (AppKit's positive
                // rotation is counterclockwise in its coordinate space).
                transform.rotateByDegrees(-deg);
                transform.concat();
                NSRect {
                    origin: NSPoint {
                        x: -base_width / 2.0,
                        y: -base_height / 2.0,
                    },
                    size: NSSize {
                        width: base_width,
                        height: base_height,
                    },
                }
            }
            None => NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: pixel_width as f64,
                    height: pixel_height as f64,
                },
            },
        };
        let from_rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: ns_size,
        };
        image.drawInRect_fromRect_operation_fraction(
            draw_rect,
            from_rect,
            NSCompositingOperation::SourceOver,
            1.0,
        );

        // 8. Restore context
        NSGraphicsContext::setCurrentContext(old_context.as_deref());

        // 9. Copy pixel data out. `bitmap_rep` is a Retained held across the
        //    copy, so the pointer is valid; it is released on scope exit.
        let bitmap_data = bitmap_rep.bitmapData();
        if bitmap_data.is_null() {
            return None;
        }
        let mut buffer = vec![0u8; data_len];
        unsafe {
            std::ptr::copy_nonoverlapping(bitmap_data, buffer.as_mut_ptr(), data_len);
        }

        // 10. Convert premultiplied -> straight alpha (GPUI's Metal renderer,
        //     and this function's documented contract, expect straight alpha).
        premultiplied_to_straight(&mut buffer);

        Some((pixel_width, pixel_height, buffer))
    })
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
