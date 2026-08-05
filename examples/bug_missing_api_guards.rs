//! REGRESSION CHECK for the fixed bug: `src/platform/macos.rs` used to send
//! AppKit selectors with no availability checking. On macOS versions lacking
//! them, `msg_send!` to an unrecognized selector raises
//! `NSInvalidArgumentException` and aborts the process:
//!
//!   imageWithSystemSymbolName:accessibilityDescription:   macOS 11.0+
//!   configurationWithPointSize:weight:scale:              macOS 11.0+
//!   configurationWithHierarchicalColor:                   macOS 12.0+
//!   configurationPreferringMulticolor                     macOS 12.0+
//!   configurationPreferringMonochrome                     macOS 13.0+
//!
//! Fix: the platform layer now probes each color-configuration selector once
//! via `respondsToSelector:` (cached in a `OnceLock`: `supports_hierarchical_color`,
//! `supports_multicolor`, `supports_monochrome` in src/platform/macos.rs) and
//! falls back to rendering the uncolored symbol instead of crashing.
//!
//! This example probes the runtime for each selector — the same check the
//! library now performs. Run: `cargo run --example bug_missing_api_guards`

use objc2::runtime::Sel;
use objc2::{ClassType, msg_send, sel};
use objc2_app_kit::{NSImage, NSImageSymbolConfiguration};

fn main() {
    // Force AppKit to load (this binary doesn't otherwise link it).
    let _ = gpui_symbols::SfSymbol::new("star.fill").render_rgba();

    let probes: &[(&str, Sel, &str)] = &[
        (
            "+[NSImage imageWithSystemSymbolName:accessibilityDescription:]",
            sel!(imageWithSystemSymbolName:accessibilityDescription:),
            "macOS 11.0+",
        ),
        (
            "+[NSImageSymbolConfiguration configurationWithPointSize:weight:scale:]",
            sel!(configurationWithPointSize:weight:scale:),
            "macOS 11.0+",
        ),
        (
            "+[NSImageSymbolConfiguration configurationWithHierarchicalColor:]",
            sel!(configurationWithHierarchicalColor:),
            "macOS 12.0+",
        ),
        (
            "+[NSImageSymbolConfiguration configurationPreferringMulticolor]",
            sel!(configurationPreferringMulticolor),
            "macOS 12.0+",
        ),
        (
            "+[NSImageSymbolConfiguration configurationPreferringMonochrome]",
            sel!(configurationPreferringMonochrome),
            "macOS 13.0+",
        ),
    ];

    println!("selector availability probe (the library now guards each of these");
    println!("with a cached respondsToSelector: check before sending it):\n");

    for (label, sel, introduced) in probes {
        let responds: bool = if label.contains("NSImage ") {
            unsafe { msg_send![NSImage::class(), respondsToSelector: *sel] }
        } else {
            unsafe { msg_send![NSImageSymbolConfiguration::class(), respondsToSelector: *sel] }
        };
        let verdict = if responds { "responds" } else { "MISSING (would fall back)" };
        println!("  {label}");
        println!("      introduced {introduced:12} this machine: {verdict}");
    }

    println!();
    println!("On macOS 11 the three color-configuration selectors are MISSING; the");
    println!("library now falls back to the uncolored symbol there instead of");
    println!("aborting on an unrecognized selector.");
}
