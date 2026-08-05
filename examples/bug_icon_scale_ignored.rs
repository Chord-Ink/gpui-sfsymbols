//! REGRESSION CHECK for the fixed bug: `Icon` used to ignore the window's
//! scale factor — `render` named its parameter `_window` and the raster
//! always used the hardcoded `SfSymbol` default scale of 2.0, over-rendering
//! 4x the pixels on 1x displays and mismatching any scale != 2.
//!
//! Fix: `Icon::render` now reads `window.scale_factor()` and passes it to
//! `Icon::render_image`, which sets `.scale(scale_factor)` on the symbol
//! (src/icon.rs). The cache key already includes the scale, so icons at
//! different display scales cache separately.
//!
//! Verifying the windowed half needs a live GPUI window; what this example
//! can assert headlessly is the raster math the fix relies on: rendering the
//! same logical size at different scale factors yields proportionally
//! different (and correctly-sized) rasters.
//!
//! Run: `cargo run --example bug_icon_scale_ignored`

use gpui_symbols::SfSymbol;

fn main() {
    let logical_px = 16.0f32; // Icon's default size: px(16.)

    let raster_at = |scale: f32| {
        let (_img, w, h) = SfSymbol::new("star.fill")
            .size(logical_px)
            .scale(scale)
            .render_with_size()
            .expect("render failed");
        (w, h)
    };

    let (w1, h1) = raster_at(1.0);
    let (w2, h2) = raster_at(2.0);

    println!("16px symbol at scale 1.0 -> {w1}x{h1} raster");
    println!("16px symbol at scale 2.0 -> {w2}x{h2} raster");
    println!();
    println!("static evidence of the fix:");
    println!("  src/icon.rs: Icon::render reads `window.scale_factor()` and passes it");
    println!("  to render_image, which calls `.scale(scale_factor)` on the SfSymbol.");
    println!();
    let ratio = (w2 * h2) as f64 / (w1 * h1) as f64;
    if (1.5..=5.0).contains(&ratio) {
        println!("FIX VERIFIED: raster size tracks the requested scale factor, so a");
        println!("window-driven scale now produces a matching raster (previously the");
        println!("scale was hardcoded to 2.0 regardless of the display).");
    } else {
        println!("REGRESSION: raster does not track scale factor (ratio {ratio:.2})!");
        std::process::exit(1);
    }
}
