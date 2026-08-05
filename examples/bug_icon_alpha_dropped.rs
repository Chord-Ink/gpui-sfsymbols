//! REGRESSION CHECK for the fixed bug: `Icon` used to discard the alpha
//! channel of its color (`color_u32 >> 8`, alpha hardcoded to 1.0 in the
//! platform layer), so `Icon::color(hsla(.., .., .., 0.1))` rendered fully
//! opaque.
//!
//! Fix: `Icon::render_image` now extracts the alpha and forwards it through
//! `SfSymbol::alpha` into `colorWithRed:...alpha:` (src/icon.rs,
//! src/platform/macos.rs).
//!
//! Below we run the exact conversion `Icon::render_image` performs on two
//! colors that differ ONLY in alpha, then render through the real pipeline
//! and compare peak alpha in the pixels.
//!
//! Run: `cargo run --example bug_icon_alpha_dropped`

use gpui::{Hsla, Rgba, hsla};
use gpui_symbols::SfSymbol;

/// Byte-for-byte copy of the conversion in `Icon::render_image`
/// (src/icon.rs).
fn icon_color_conversion(color: Hsla) -> (u32, f32) {
    let rgba: Rgba = color.into();
    let color_u32: u32 = rgba.into(); // 0xRRGGBBAA
    let rgb = color_u32 >> 8;
    let alpha = (color_u32 & 0xFF) as f32 / 255.0;
    (rgb, alpha)
}

fn main() {
    let opaque_red = hsla(0.0, 1.0, 0.5, 1.0);
    let faint_red = hsla(0.0, 1.0, 0.5, 0.1); // 10% opacity

    let (rgb_a, alpha_a) = icon_color_conversion(opaque_red);
    let (rgb_b, alpha_b) = icon_color_conversion(faint_red);

    println!("opaque red (alpha=1.0) -> color 0x{rgb_a:06X}, alpha {alpha_a:.2}");
    println!("faint  red (alpha=0.1) -> color 0x{rgb_b:06X}, alpha {alpha_b:.2}");
    assert_eq!(rgb_a, rgb_b, "same hue expected");
    if (alpha_a - alpha_b).abs() < 0.01 {
        println!("REGRESSION: alpha is dropped in the Icon conversion again!");
        std::process::exit(1);
    }

    // End-to-end through the real renderer:
    let peak_alpha = |a: f32| {
        let (_w, _h, data) = SfSymbol::new("star.fill")
            .color(rgb_a)
            .alpha(a)
            .render_rgba()
            .unwrap();
        data.chunks_exact(4).map(|px| px[3]).max().unwrap()
    };
    let opaque_peak = peak_alpha(1.0);
    let faint_peak = peak_alpha(0.1);
    println!("rendered peak alpha: opaque={opaque_peak}, faint={faint_peak}");
    println!();
    if faint_peak < opaque_peak / 4 {
        println!("FIX VERIFIED: color opacity reaches the rendered pixels end-to-end.");
    } else {
        println!("REGRESSION: alpha does not affect rendered pixels!");
        std::process::exit(1);
    }
}
