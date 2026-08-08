# gpui-sfsymbols

[![Crates.io](https://img.shields.io/crates/v/gpui-sfsymbols.svg)](https://crates.io/crates/gpui-sfsymbols)
[![Documentation](https://docs.rs/gpui-sfsymbols/badge.svg)](https://docs.rs/gpui-sfsymbols)
[![License](https://img.shields.io/crates/l/gpui-sfsymbols.svg)](https://github.com/Chord-Ink/gpui-sfsymbols#license)
[![Downloads](https://img.shields.io/crates/d/gpui-sfsymbols.svg)](https://crates.io/crates/gpui-sfsymbols)

macOS SF Symbols rendering for Rust / GPUI applications.

## Workspace layout

This repository is a single Cargo workspace:

| Path | Crate | Description |
|------|-------|-------------|
| [`crates/gpui-sfsymbols`](./crates/gpui-sfsymbols) | `gpui-sfsymbols` | SF Symbols rendering + GPUI `Icon` component (main crate) |
| [`crates/sfsymbols`](./crates/sfsymbols) | `sfsymbols` | Type-safe SF Symbols enums (9000+ symbols) |
| [`crates/sfsymbols-codegen`](./crates/sfsymbols-codegen) | `sfsymbols-codegen` | Code generator for `sfsymbols` (dev tool, not published) |

> **Note**: `gpui-symbols` was renamed to `gpui-sfsymbols` (the short-lived
> re-export shim previously occupying that name was removed). Depend on
> `gpui-sfsymbols` directly — its default features already include `presets`.

## Installation

```toml
[dependencies]
gpui-sfsymbols = "0.6"

# With GPUI integration
gpui-sfsymbols = { version = "0.7", features = ["gpui"] }

# With Icon component (recommended for GPUI apps)
gpui-sfsymbols = { version = "0.7", features = ["component"] }
```

## Usage

### Basic - Raw RGBA Pixels

```rust
use gpui_sfsymbols::SfSymbol;

let (width, height, rgba_data) = SfSymbol::new("star.fill")
    .size(32.0)
    .color(0x000000)
    .render_rgba()
    .unwrap();
```

### With GPUI Integration

```rust
use gpui_sfsymbols::SfSymbol;
use gpui::{img, ImageSource};

let image = SfSymbol::new("heart.fill")
    .size(48.0)
    .color(0xFF0000)
    .render()
    .unwrap();

// Use in GPUI element
img(ImageSource::Render(image)).size(px(32.))
```

### Icon Component (Recommended)

The `Icon` component provides a high-level API similar to GPUI Components:

```rust
use gpui_sfsymbols::Icon;
use gpui::px;

fn view(window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
        .child(Icon::new("star.fill"))
        .child(Icon::new("heart.fill").color(gpui::rgb(0xFF0000)).size(px(24.)))
}
```

#### Using Preset Symbols (Recommended)

With the `presets` feature, use type-safe SF Symbols enums directly:

```rust
use gpui_sfsymbols::{Icon, sfsymbols::SfSymbolV7};

// All 9000+ SF Symbols as compile-time checked enums
let icon = Icon::from_name(SfSymbolV7::StarFill);
let heart = Icon::from_name(SfSymbolV7::HeartFill).color(gpui::rgb(0xFF0000));
```

#### Using Unified Symbol Enum (New in 0.6)

Use `SfSymbol` for a single type covering all SF Symbols versions:

```rust
use gpui_sfsymbols::{Icon, sfsymbols::SfSymbol};

// Unified enum with version metadata
let icon = Icon::from_name(SfSymbol::Gearshape);

// Check minimum version required
let (major, minor) = SfSymbol::Gearshape.min_version(); // (2, 0)
```

This is useful for cross-platform abstractions:

```rust
use gpui_sfsymbols::sfsymbols::SfSymbol;

enum AppIcon { Settings, Search, Plus }

impl AppIcon {
    fn sf_symbol(&self) -> SfSymbol {
        match self {
            Self::Settings => SfSymbol::Gearshape,
            Self::Search => SfSymbol::Magnifyingglass,
            Self::Plus => SfSymbol::Plus,
        }
    }
}
```

#### Define Custom Icon Enums

Alternatively, use the `define_icons!` macro:

```rust
use gpui_sfsymbols::{Icon, define_icons};

define_icons! {
    pub enum AppIcon {
        Star => "star.fill",
        Heart => "heart.fill",
        Settings => "gearshape.fill",
    }
}

let icon = Icon::from_name(AppIcon::Star).color(gpui::rgb(0xFF0000));
```

### Aspect Ratio Preservation (New in 0.6.1)

SF Symbols are not always square - for example, `gearshape.2` is wider than tall. The `Icon` component automatically preserves the original aspect ratio:

```rust
// Non-square symbols render correctly
Icon::new("gearshape").size(px(32.))     // Square gear
Icon::new("gearshape.2").size(px(32.))   // Wide: two gears side by side
Icon::new("person.3.fill").size(px(32.)) // Wide: three people
```

The `size()` method sets the maximum dimension while maintaining the symbol's natural proportions.

### Advanced Symbol Options

Customize weight, scale, and rendering mode:

```rust
use gpui_sfsymbols::{Icon, SymbolWeight, SymbolScale, RenderingMode};

// Bold weight icon
Icon::new("star.fill")
    .weight(SymbolWeight::Bold)
    .size(px(24.));

// Large scale with multicolor rendering
Icon::new("cloud.sun.fill")
    .symbol_scale(SymbolScale::Large)
    .rendering_mode(RenderingMode::Multicolor);

// Thin weight, monochrome style
Icon::new("heart.fill")
    .weight(SymbolWeight::Thin)
    .rendering_mode(RenderingMode::Monochrome)
    .color(gpui::rgb(0xFF0000));
```

### Rendering Modes and Palette Colors

All four `NSImageSymbolConfiguration` color modes are supported, mirroring
SwiftUI's `.symbolRenderingMode(...)` + `.foregroundStyle(...)`:

```rust
use gpui::rgb;
use gpui_sfsymbols::{Icon, RenderingMode};

// Hierarchical: one color, automatic opacity layers (macOS 12+)
Icon::new("cloud.sun.fill")
    .rendering_mode(RenderingMode::Hierarchical)
    .color(gpui::rgb(0x007AFF));

// Palette: up to 3 colors — primary, secondary, tertiary (macOS 12+)
Icon::new("cloud.sun.fill")
    .rendering_mode(RenderingMode::Palette)
    .palette_colors([rgb(0x007AFF), rgb(0xFFCC00)]);

// Multicolor: the symbol's intrinsic colors (macOS 12+)
Icon::new("cloud.sun.fill")
    .rendering_mode(RenderingMode::Multicolor);
```

The low-level builder uses the crate's `SymbolColor` type (`From<u32>`, and
`From<Hsla>`/`From<Rgba>` with the `gpui` feature):

```rust
use gpui_sfsymbols::{SfSymbol, RenderingMode, SymbolColor};

SfSymbol::new("cloud.sun.fill")
    .rendering_mode(RenderingMode::Palette)
    .palette_colors([SymbolColor::rgb(0x007AFF), SymbolColor::rgb(0xFFCC00)])
    .render_rgba();

// Single-color setter keeps color and alpha together:
SfSymbol::new("star.fill")
    .color(SymbolColor::rgb(0xFF0000).with_alpha(0.5))
    .render_rgba();
```

### Variable Color (macOS 14+)

Symbols that support Variable Color take a `0.0 - 1.0` value controlling how
many layers are emphasized:

```rust
Icon::new("wifi").variable_value(0.0);  // no emphasized layers
Icon::new("wifi").variable_value(0.5);
Icon::new("wifi").variable_value(1.0);  // all layers
```

### Animations (`SymbolEffect`)

The `NSSymbolEffect` animation family is available on `Icon`, implemented
with GPUI primitives:

```rust
use gpui_sfsymbols::{Icon, SymbolEffect};

// Bitmap-sampled (played back as multi-frame images, always looping):
Icon::new("wifi").effect(SymbolEffect::VariableColor);   // exact (macOS 14+)
Icon::new("arrow.clockwise").effect(SymbolEffect::Rotate); // faithful

// GPUI element animations:
Icon::new("bell.badge.fill").effect(SymbolEffect::Pulse);
Icon::new("lungs.fill").effect(SymbolEffect::Breathe);
Icon::new("arrow.down.circle.fill").effect(SymbolEffect::Bounce);
Icon::new("bell.fill").effect(SymbolEffect::Wiggle);
Icon::new("star.fill").effect(SymbolEffect::ScaleUp);
Icon::new("heart.fill").effect(SymbolEffect::ScaleDown);
Icon::new("sparkles").effect(SymbolEffect::Appear);        // one-shot
Icon::new("xmark.circle.fill").effect(SymbolEffect::Disappear); // one-shot

// Cross-fade when the symbol name changes (needs a stable id):
Icon::new(if playing { "pause.fill" } else { "play.fill" })
    .id("transport")
    .effect(SymbolEffect::Replace);
```

Effect options, mirroring the SF Symbols inspector:

```rust
use gpui_sfsymbols::{Icon, SymbolEffect, Repeat, Animate, Direction};
use std::time::Duration;

// Repeat: Once / Continuously / Repeat with Delay
Icon::new("bell.badge.fill")
    .effect(SymbolEffect::Pulse)
    .repeat(Repeat::WithDelay(Duration::from_secs(2)));

// Pulses: oscillations per cycle
Icon::new("star.fill").effect(SymbolEffect::ScaleUp).pulses(3);

// Direction: per-effect, mirroring Apple's .up/.down/.clockwise/...
Icon::new("arrow.counterclockwise")
    .effect(SymbolEffect::Rotate)
    .direction(Direction::Counterclockwise);
Icon::new("arrow.down.circle.fill")
    .effect(SymbolEffect::Bounce)
    .direction(Direction::Down);

// Animate: whole symbol or by layer (by-layer is native to VariableColor)
Icon::new("wifi").effect(SymbolEffect::VariableColor).animate(Animate::ByLayer);

// Custom cycle duration:
Icon::new("wifi")
    .effect(SymbolEffect::VariableColor)
    .duration(Duration::from_secs(2));
```

Not supported: `Draw On` / `Draw Off` animate the symbol *drawing itself
along its paths*, which needs per-layer vector data that a rendered bitmap
does not retain.

Fidelity notes: `VariableColor` is frame-sampled from AppKit's own
`variableValue` rendering (exact); `Rotate` is frame-sampled with
`NSAffineTransform` (faithful, whole glyph); `Pulse`/`Appear`/`Disappear` are
opacity-based like Apple's, `Breathe` combines scale+opacity like Apple's
`.breathe.pulse`; `Bounce`/`Wiggle`/`Scale*`/`Replace` approximate the whole
glyph (Apple animates individual layers with springs, which is not
reproducible from bitmaps). Animation state is keyed by `ElementId` (defaults
to the symbol name) — animated icons in the same view need unique ids.

### Integration with gpui-component

Use SF Symbols with [gpui-component](https://github.com/longbridge/gpui-component) Button via `child()`:

```rust
use gpui_sfsymbols::{Icon as SfIcon, sfsymbols::SfSymbolV7};
use gpui_component::Button;

// Icon-only button
Button::new("star-btn")
    .child(SfIcon::from_name(SfSymbolV7::StarFill).size(px(16.)))

// Icon + label button
Button::new("favorite-btn")
    .child(
        div().flex().items_center().gap_2()
            .child(SfIcon::from_name(SfSymbolV7::HeartFill).size(px(16.)).color(gpui::rgb(0xFF0000)))
            .child("Favorite")
    )
```

> **Note**: gpui-component's `Button::icon()` expects SVG paths, while gpui-sfsymbols renders to pixel images. Use `Button::child()` for SF Symbols integration.

## API

### `SfSymbol`

Low-level builder for rendering SF Symbols.

| Method | Description | Default |
|--------|-------------|---------|
| `new(name)` | Create symbol with name | - |
| `size(f32)` | Point size (`0 < size <= 1024`) | 32.0 |
| `scale(f32)` | Scale factor (`0 < scale <= 8`; Retina) | 2.0 |
| `color(impl Into<SymbolColor>)` | Color with alpha (accepts `u32`, `Hsla`/`Rgba` with `gpui`) | black |
| `alpha(f32)` | Color opacity (`0.0 - 1.0`) | 1.0 |
| `palette_colors(iter)` | Palette colors, max 3 (for `Palette` mode) | empty |
| `variable_value(f64)` | Variable Color value (`0.0 - 1.0`, macOS 14+) | unset |
| `weight(SymbolWeight)` | Symbol weight | Regular |
| `symbol_scale(SymbolScale)` | Symbol scale | Medium |
| `rendering_mode(RenderingMode)` | Rendering mode | Hierarchical |
| `render_rgba()` | Render to `(width, height, Vec<u8>)` (straight alpha, sRGB) | - |
| `render_frames(effect, fps, duration)` | Sample `VariableColor` to `Vec<SymbolFrame>` | - |
| `render()` | Render to `Arc<RenderImage>` (requires `gpui` feature) | - |
| `render_animated(effect, fps, duration)` | Render to multi-frame `Arc<RenderImage>` (requires `gpui`) | - |

### `Icon`

High-level GPUI component (requires `component` feature).

| Method | Description | Default |
|--------|-------------|---------|
| `new(name)` | Create icon with SF Symbol name | - |
| `from_name(T)` | Create from `IconName` type | - |
| `size(Pixels)` | Set maximum dimension (preserves aspect ratio) | 16px |
| `color(impl Into<Hsla>)` | Set color (Hsla, Rgba, or `rgb(hex)`) | black |
| `palette_colors(iter)` | Palette colors, max 3 (for `Palette` mode) | empty |
| `variable_value(f64)` | Variable Color value (`0.0 - 1.0`, macOS 14+) | unset |
| `weight(SymbolWeight)` | Symbol weight | Regular |
| `symbol_scale(SymbolScale)` | Symbol scale | Medium |
| `rendering_mode(RenderingMode)` | Rendering mode | Hierarchical |
| `effect(SymbolEffect)` | Animate the icon | none |
| `duration(Duration)` | Effect cycle duration | per-effect |
| `repeat(Repeat)` | `Once` / `Continuously` / `WithDelay` | per-effect |
| `animate(Animate)` | `WholeSymbol` / `ByLayer` (by-layer: VariableColor only) | WholeSymbol |
| `direction(Direction)` | Effect direction (see per-effect support) | Default |
| `pulses(u32)` | Oscillations per cycle (oscillating effects) | 1 |
| `id(impl Into<ElementId>)` | Element id keying animation state | symbol name |

### Enums

**`SymbolWeight`**: `UltraLight`, `Thin`, `Light`, `Regular`, `Medium`, `Semibold`, `Bold`, `Heavy`, `Black`

**`SymbolScale`**: `Small`, `Medium`, `Large`

**`RenderingMode`**: `Monochrome`, `Hierarchical`, `Palette`, `Multicolor`

**`SymbolEffect`**: `VariableColor`, `Rotate`, `Pulse`, `Breathe`, `Bounce`, `Wiggle`, `ScaleUp`, `ScaleDown`, `Appear`, `Disappear`, `Replace` (`Draw On`/`Draw Off` not supported — see above)

**`Repeat`**: `Once`, `Continuously`, `WithDelay(Duration)`

**`Animate`**: `WholeSymbol`, `ByLayer`

**`Direction`**: `Default`, `Up`, `Down`, `Left`, `Right`, `Clockwise`, `Counterclockwise` — supported by `Bounce` (Up/Down), `Wiggle` (Left/Right/Up/Down), `Rotate` (Clockwise/Counterclockwise), `Appear`/`Disappear` (Up/Down slide), `Replace` (Up ≈ `.offUp`, Down ≈ `.downUp`)

**`SymbolColor`**: `rgb(0xRRGGBB)`, `rgba(0xRRGGBBAA)`, `with_alpha(f32)`; `From<u32>`, and `From<Hsla>`/`From<Rgba>` with `gpui`

### Cache Management

With the `cache` feature (enabled by default), rendered symbols are cached globally for performance. The cache is bounded to 256 entries (an arbitrary entry is evicted when full), so long-running apps cannot grow it without limit:

```rust
use gpui_sfsymbols::{cache_size, clear_cache};

// Get number of cached symbols
let count = cache_size();

// Clear all cached symbols (e.g., on memory pressure or appearance change)
clear_cache();
```

## Features

| Feature | Description |
|---------|-------------|
| `default` | `component` + `presets` + `cache` |
| `cache` | Global cache for rendered symbols (implies `gpui`) |
| `gpui` | GPUI integration, returns `Arc<RenderImage>` |
| `component` | High-level `Icon` component (implies `gpui`) |
| `presets` | Type-safe SF Symbols enums via `sfsymbols` crate |

## Requirements

- **macOS only** — SF Symbols are an Apple platform API rendered through
  AppKit; this crate intentionally fails to compile on other targets.
- macOS 11.0+ baseline (SF Symbols support). Colored rendering modes
  (`Hierarchical`, `Palette`) require macOS 12.0+, `Monochrome` requires
  macOS 13.0+, Variable Color (`variable_value`, `SymbolEffect::VariableColor`)
  requires macOS 14.0+ — on older systems rendering degrades gracefully to
  the uncolored/default symbol instead of crashing.
- Rust 1.85+ (edition 2024)

## Examples

```bash
cargo run -p gpui-sfsymbols --example gallery  # SF Symbols gallery: sidebar, grid, live inspector
```

## Symbol Names

See [SF Symbols](https://developer.apple.com/sf-symbols/) for available symbol names.

Common symbols:
- `star.fill`, `heart.fill`, `house.fill`
- `gearshape`, `magnifyingglass`, `trash`
- `person.fill`, `folder.fill`, `doc.fill`

## License

MIT OR Apache-2.0
