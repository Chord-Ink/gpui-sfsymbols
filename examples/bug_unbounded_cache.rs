//! REGRESSION CHECK for the fixed bug: the global render cache used to be
//! unbounded — caller-controlled keys (name/size/scale), no cap, no eviction —
//! so long-running apps (or icon-size animations) grew memory forever, with
//! each entry pinning a full RGBA frame.
//!
//! Fix: the cache is capped at `MAX_CACHE_ENTRIES` (256); inserting past the
//! cap evicts an arbitrary entry (src/symbol.rs). `clear_cache()` remains for
//! explicit control.
//!
//! Run: `cargo run --example bug_unbounded_cache`

use gpui_symbols::{SfSymbol, cache_size, clear_cache};

fn main() {
    const ATTEMPTS: usize = 1000;
    const CAP: usize = 256; // MAX_CACHE_ENTRIES in src/symbol.rs

    clear_cache();

    // Distinct keys: same symbol, slightly different size each time.
    for i in 0..ATTEMPTS {
        let _ = SfSymbol::new("star.fill")
            .size(16.0 + i as f32 * 0.25)
            .render_with_size();
    }

    let len = cache_size();
    println!("inserted {ATTEMPTS} distinct keys, cache holds {len} entries (cap {CAP})");
    println!();
    if len <= CAP {
        println!("FIX VERIFIED: cache is bounded; eviction keeps it at the cap.");
    } else {
        println!("REGRESSION: cache is unbounded again ({len} entries)!");
        std::process::exit(1);
    }
    clear_cache();
}
