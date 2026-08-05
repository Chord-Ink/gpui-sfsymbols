//! REGRESSION CHECK for the fixed bug: `render_sf_symbol` used to run without
//! an `NSAutoreleasePool`, so on any thread without one (every `std::thread`,
//! CLI processes, test binaries) all autoreleased NSString/NSImage/NSColor/
//! configuration temporaries leaked — measured at ~213 KB/render on top of
//! the separate bitmap leak.
//!
//! Fix: the whole render now runs inside `objc2::rc::autoreleasepool`
//! (src/platform/macos.rs), so any thread is safe.
//!
//! This example renders on a bare `std::thread` (no pool of its own) and
//! asserts the per-render growth is ~0.
//!
//! Run: `cargo run --example bug_autorelease_leak`

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

    let leaked = std::thread::spawn(|| {
        let _ = SfSymbol::new("star.fill").size(64.0).render_rgba(); // warm-up
        let before = rss_kb();
        for _ in 0..RENDERS {
            let _ = SfSymbol::new("star.fill").size(64.0).render_rgba();
        }
        rss_kb() - before
    })
    .join()
    .unwrap();

    let per_render = leaked as f64 / RENDERS as f64;
    println!("{RENDERS} renders on a bare std::thread (no user pool):");
    println!("leaked: {per_render:.2} KB per render");
    println!();
    // Before the fix this measured ~330 KB/render without a pool.
    if per_render < 4.0 {
        println!("FIX VERIFIED: internal autorelease pool drains all temporaries.");
    } else {
        println!("REGRESSION: autorelease leak is back ({per_render:.1} KB/render)!");
        std::process::exit(1);
    }
}
