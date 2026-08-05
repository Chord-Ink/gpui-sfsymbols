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
    NSBitmapFormat, NSBitmapImageRep, NSColor, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSImageSymbolConfiguration, NSImageSymbolScale,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::symbol::{RenderingMode, SymbolScale, SymbolWeight};

/// Upper bounds for user-supplied rendering parameters.
///
/// SF Symbols are UI icons; anything beyond these is a bug in the caller and
/// would overflow buffer arithmetic or attempt enormous allocations.
const MAX_POINT_SIZE: f32 = 1024.0;
const MAX_SCALE: f32 = 8.0;
const MAX_PIXEL_DIMENSION: f64 = 16_384.0;

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

/// Render SF Symbol to RGBA pixel buffer.
///
/// Pixels are straight (non-premultiplied) alpha, in the sRGB color space.
///
/// # Arguments
///
/// * `name` - The SF Symbol name (e.g., "star.fill")
/// * `point_size` - The point size for rendering (0 < size <= 1024)
/// * `scale` - The scale factor, e.g. 2.0 for Retina (0 < scale <= 8)
/// * `color` - RGB hex color value
/// * `alpha` - Opacity applied to the color (0.0 - 1.0)
/// * `weight` - Symbol weight (ultraLight to black)
/// * `symbol_scale` - Symbol scale (small, medium, large)
/// * `rendering_mode` - Rendering mode (monochrome, hierarchical, palette, multicolor)
///
/// # Returns
///
/// Returns `Some((width, height, rgba_data))` on success, `None` on failure
/// (unknown symbol, invalid parameters, or allocation failure).
pub fn render_sf_symbol(
    name: &str,
    point_size: f32,
    scale: f32,
    color: u32,
    alpha: f32,
    weight: SymbolWeight,
    symbol_scale: SymbolScale,
    rendering_mode: RenderingMode,
) -> Option<(u32, u32, Vec<u8>)> {
    // Reject non-finite or absurd parameters up front. Without this, huge or
    // infinite values saturate the float->int casts and overflow the buffer
    // arithmetic below, or request multi-terabyte bitmaps from AppKit.
    if !point_size.is_finite()
        || !scale.is_finite()
        || !alpha.is_finite()
        || point_size <= 0.0
        || point_size > MAX_POINT_SIZE
        || scale <= 0.0
        || scale > MAX_SCALE
    {
        return None;
    }

    autoreleasepool(|_pool| {
        // 1. Create NSImage with system symbol name
        let ns_name = NSString::from_str(name);
        let symbol_image =
            NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns_name, None)?;

        // 2. Create color from RGB + alpha
        let r = ((color >> 16) & 0xFF) as f64 / 255.0;
        let g = ((color >> 8) & 0xFF) as f64 / 255.0;
        let b = (color & 0xFF) as f64 / 255.0;
        let symbol_color =
            NSColor::colorWithRed_green_blue_alpha(r, g, b, alpha.clamp(0.0, 1.0) as f64);

        // 3. Size/weight/scale configuration (available since macOS 11.0)
        let size_config = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(
            point_size as f64,
            weight.to_ns_weight(),
            NSImageSymbolScale(symbol_scale.to_ns_scale() as isize),
        );

        // 4. Color configuration, guarded by API availability so older macOS
        //    degrades gracefully (uncolored) instead of crashing on an
        //    unrecognized selector.
        let color_config: Option<Retained<NSImageSymbolConfiguration>> = match rendering_mode {
            RenderingMode::Monochrome => {
                if supports_monochrome() && supports_hierarchical_color() {
                    // Prefer the monochrome variant, tinted by the color.
                    let mono = NSImageSymbolConfiguration::configurationPreferringMonochrome();
                    let tint =
                        NSImageSymbolConfiguration::configurationWithHierarchicalColor(&symbol_color);
                    Some(mono.configurationByApplyingConfiguration(&tint))
                } else if supports_hierarchical_color() {
                    // macOS 12: no monochrome-preferring configuration yet.
                    Some(NSImageSymbolConfiguration::configurationWithHierarchicalColor(
                        &symbol_color,
                    ))
                } else {
                    None
                }
            }
            RenderingMode::Hierarchical | RenderingMode::Palette => {
                // Palette requires multiple colors; with a single color it
                // intentionally behaves like Hierarchical.
                supports_hierarchical_color().then(|| {
                    NSImageSymbolConfiguration::configurationWithHierarchicalColor(&symbol_color)
                })
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

        // 5. Apply configuration
        let image = symbol_image
            .imageWithSymbolConfiguration(&combined_config)
            .unwrap_or(symbol_image);

        // 6. Pixel dimensions. Inputs are bounded above, so these are finite,
        //    positive, and small enough that all buffer math below is exact.
        let ns_size: NSSize = image.size();
        let width = (ns_size.width * scale as f64).ceil();
        let height = (ns_size.height * scale as f64).ceil();
        if !width.is_finite()
            || !height.is_finite()
            || width < 1.0
            || height < 1.0
            || width > MAX_PIXEL_DIMENSION
            || height > MAX_PIXEL_DIMENSION
        {
            return None;
        }
        let pixel_width = width as u32;
        let pixel_height = height as u32;
        let bytes_per_row = pixel_width as usize * 4;
        // pixel_width/pixel_height <= 16_384, so this cannot overflow usize.
        let data_len = bytes_per_row * pixel_height as usize;

        // 7. Create NSBitmapImageRep for drawing.
        //
        // Note on alpha: AppKit's graphics contexts cannot draw into
        // non-premultiplied bitmaps (context creation fails), so the bitmap
        // uses the default *premultiplied* format and we convert to straight
        // alpha ourselves after copying the pixels out (see step 10).
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

        // 8. Create NSGraphicsContext and draw
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap_rep)?;
        let old_context = NSGraphicsContext::currentContext();
        NSGraphicsContext::setCurrentContext(Some(&context));

        // Transparent background - the bitmap is zero-initialized.
        let draw_rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: pixel_width as f64,
                height: pixel_height as f64,
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

        // 9. Restore context
        NSGraphicsContext::setCurrentContext(old_context.as_deref());

        // 10. Copy pixel data out. `bitmap_rep` is a Retained held across the
        //     copy, so the pointer is valid; it is released on scope exit.
        let bitmap_data = bitmap_rep.bitmapData();
        if bitmap_data.is_null() {
            return None;
        }
        let mut buffer = vec![0u8; data_len];
        unsafe {
            std::ptr::copy_nonoverlapping(bitmap_data, buffer.as_mut_ptr(), data_len);
        }

        // 11. Convert premultiplied -> straight alpha (GPUI's Metal renderer,
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
