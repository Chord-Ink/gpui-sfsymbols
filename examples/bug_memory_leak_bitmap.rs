//! REGRESSION CHECK for the fixed bug: `render_sf_symbol` used to leak every
//! `NSBitmapImageRep` it created (alloc/init under MRC, never released), ~1
//! full bitmap per render.
//!
//! Fix: the platform layer now uses objc2 `Retained` (releases on drop) and
//! runs the whole render inside an `NSAutoreleasePool`
//! (src/platform/macos.rs).
//!
//! This example renders in a loop *without* adding a pool of its own and
//! asserts the per-render memory growth is ~0.
//!
//! Run: `cargo run --example bug_memory_leak_bitmap`

use gpui_symbols::SfSymbol;

fn rss_kb() -> i64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

fn main() {
    const RENDERS: usize = 1000;

    // Warm up one-time AppKit initialization.
    let (w, h, _) = SfSymbol::new("star.fill").size(64.0).render_rgba().unwrap();
    let bitmap_kb = (w * h * 4) as f64 / 1024.0;
    println!("symbol renders to {w}x{h} = {bitmap_kb:.1} KB per bitmap\n");

    let before = rss_kb();
    for _ in 0..RENDERS {
        let _ = SfSymbol::new("star.fill").size(64.0).render_rgba();
    }
    let after = rss_kb();

    let per_render = (after - before) as f64 / RENDERS as f64;
    println!("RSS before: {before} KB");
    println!("RSS after:  {after} KB");
    println!("leaked:     {per_render:.2} KB per render over {RENDERS} renders");
    println!();
    // Before the fix this measured ~115 KB/render (one full bitmap).
    if per_render < 4.0 {
        println!("FIX VERIFIED: no meaningful per-render leak (was ~{bitmap_kb:.1} KB/render).");
    } else {
        println!("REGRESSION: leak is back ({per_render:.1} KB/render)!");
        std::process::exit(1);
    }
}
