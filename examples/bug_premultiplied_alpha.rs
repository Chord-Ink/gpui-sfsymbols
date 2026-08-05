//! REGRESSION CHECK for the fixed bug: the crate used to hand *premultiplied*
//! alpha pixels to GPUI, whose Metal renderer expects *straight* alpha —
//! translucent pixels (the whole Hierarchical default mode!) composited too
//! dark. `render_rgba`'s "raw RGBA" documentation was also wrong.
//!
//! AppKit cannot draw into non-premultiplied bitmaps (graphics context
//! creation fails), so the fix converts premultiplied -> straight alpha in
//! `premultiplied_to_straight` after copying the pixels out
//! (src/platform/macos.rs), mirroring GPUI's own `swap_rgba_pa_to_bgra`.
//!
//! Evidence: for straight alpha, a fully-saturated red symbol's translucent
//! pixels keep r=255 regardless of alpha; premultiplied pixels have r <= a.
//!
//! Run: `cargo run --example bug_premultiplied_alpha`

use gpui_symbols::SfSymbol;

fn main() {
    let (w, h, data) = SfSymbol::new("star.fill")
        .size(64.0)
        .color(0xFF0000) // pure red
        .render_rgba()
        .expect("render failed");

    let mut premultiplied = 0u32; // r <= a among translucent pixels
    let mut straight = 0u32; //      r >  a among translucent pixels
    let mut samples = Vec::new();

    for px in data.chunks_exact(4) {
        let (r, a) = (px[0], px[3]);
        if a > 0 && a < 255 {
            if r > a {
                straight += 1;
            } else {
                premultiplied += 1;
            }
            if samples.len() < 5 {
                samples.push((px[0], px[1], px[2], px[3]));
            }
        }
    }

    println!("rendered {w}x{h}, translucent pixels: {}", premultiplied + straight);
    for (r, g, b, a) in &samples {
        println!("  sample: r={r:3} g={g:3} b={b:3} a={a:3}");
    }
    println!();
    println!("pixels consistent with straight alpha      (r >  a): {straight}");
    println!("pixels consistent with premultiplied alpha (r <= a): {premultiplied}");
    println!();
    if straight > 0 && premultiplied == 0 {
        println!("FIX VERIFIED: translucent pixels keep full color regardless of alpha —");
        println!("the buffer is straight alpha, matching GPUI's renderer and the");
        println!("documented contract of render_rgba.");
    } else {
        println!("REGRESSION: pixel data is premultiplied again!");
        std::process::exit(1);
    }
}
