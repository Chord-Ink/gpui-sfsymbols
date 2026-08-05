//! REGRESSION CHECK for the fixed bug: `SfSymbol::size()` / `scale()` used to
//! accept any `f32`, leading to (a) a debug-build integer-overflow panic in
//! the buffer arithmetic and (b) multi-terabyte bitmap requests that got the
//! process OOM-killed (SIGKILL, exit 137).
//!
//! Fix: `render_sf_symbol` validates that size/scale are finite and within
//! `0 < size <= 1024`, `0 < scale <= 8`, and computes buffer sizes with
//! bounds-checked math (src/platform/macos.rs). Invalid values return `None`.
//!
//! This example re-executes itself so each case runs in a child process, and
//! asserts every child now exits cleanly with `None`.
//!
//! Run: `cargo run --example bug_unvalidated_size`

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Child mode: actually perform the (previously dangerous) render.
    if let Some(mode) = args.iter().find(|a| a.starts_with("--case=")) {
        let size: f32 = mode.trim_start_matches("--case=").parse().unwrap();
        eprintln!("[child] SfSymbol::new(\"star.fill\").size({size:e}).render_rgba()");
        let r = gpui_symbols::SfSymbol::new("star.fill").size(size).render_rgba();
        eprintln!("[child] survived, result is_some={}", r.is_some());
        return;
    }

    let exe = std::env::current_exe().unwrap();
    let mut all_ok = true;

    // Before the fix: sizes 6e8 and inf panicked at macos.rs
    // ("attempt to multiply with overflow", exit 101); 1e6 was OOM-killed
    // (SIGKILL, exit 137). Now every case must exit 0 with result None.
    for size in ["600000000", "inf", "1000000", "nan", "-1", "0"] {
        let out = std::process::Command::new(&exe)
            .arg(format!("--case={size}"))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        let survived_none = stderr.contains("result is_some=false");
        let clean = out.status.success() && survived_none;
        all_ok &= clean;
        println!(
            "[case size={size:>10}] exit {} — {}",
            out.status,
            if clean { "OK (rejected, None)" } else { "FAILED" }
        );
        if !clean {
            print!("{stderr}");
        }
    }

    println!();
    if all_ok {
        println!("FIX VERIFIED: all invalid sizes are rejected with None;");
        println!("no panic, no OOM kill.");
    } else {
        println!("REGRESSION: invalid sizes crash again!");
        std::process::exit(1);
    }
}
