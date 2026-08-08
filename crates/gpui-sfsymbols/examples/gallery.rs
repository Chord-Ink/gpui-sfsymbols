//! SF Symbols Gallery — browse and configure symbols like Apple's SF Symbols app.
//!
//! Sidebar with categories, a symbol grid, and a live inspector for rendering
//! modes, colors, weight/scale, variable value, and `SymbolEffect` animations
//! (select `Replace` and click between two symbols to see the cross-fade).
//!
//! Run: `cargo run -p gpui-sfsymbols --example gallery`

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, MouseButton, MouseDownEvent, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, px, rgb, size,
};
use gpui_platform::application;
use gpui_sfsymbols::{
    Animate, Direction, Icon, RenderingMode, Repeat, SymbolEffect, SymbolScale, SymbolWeight,
    sfsymbols::SfSymbol,
};

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

struct Category {
    name: &'static str,
    symbols: &'static [SfSymbol],
}

/// Curated categories (every variant verified against the unified
/// `sfsymbols::SfSymbol` enum).
static CATEGORIES: &[Category] = &[
    Category {
        name: "Multicolor",
        symbols: &[
            SfSymbol::CloudSunFill,
            SfSymbol::CloudRainFill,
            SfSymbol::CloudBoltFill,
            SfSymbol::SunMaxFill,
            SfSymbol::MoonFill,
            SfSymbol::LeafFill,
            SfSymbol::FlameFill,
            SfSymbol::DropFill,
            SfSymbol::Snowflake,
            SfSymbol::Rainbow,
            SfSymbol::StarFill,
            SfSymbol::HeartFill,
        ],
    },
    Category {
        name: "Variable Color",
        symbols: &[
            SfSymbol::Wifi,
            SfSymbol::AntennaRadiowavesLeftAndRight,
            SfSymbol::Cellularbars,
            SfSymbol::Battery100,
            SfSymbol::SpeakerWave3,
            SfSymbol::Waveform,
            SfSymbol::MicFill,
        ],
    },
    Category {
        name: "Communication",
        symbols: &[
            SfSymbol::MessageFill,
            SfSymbol::PhoneFill,
            SfSymbol::EnvelopeFill,
            SfSymbol::VideoFill,
            SfSymbol::BubbleLeftFill,
            SfSymbol::BellFill,
            SfSymbol::BellBadgeFill,
            SfSymbol::MegaphoneFill,
        ],
    },
    Category {
        name: "Media",
        symbols: &[
            SfSymbol::PlayFill,
            SfSymbol::PauseFill,
            SfSymbol::StopFill,
            SfSymbol::BackwardFill,
            SfSymbol::ForwardFill,
            SfSymbol::SpeakerWave3Fill,
            SfSymbol::MusicNote,
            SfSymbol::MicFill,
            SfSymbol::PhotoFill,
            SfSymbol::CameraFill,
            SfSymbol::VideoFill,
            SfSymbol::Airplayvideo,
        ],
    },
    Category {
        name: "Arrows",
        symbols: &[
            SfSymbol::ArrowUp,
            SfSymbol::ArrowDown,
            SfSymbol::ArrowLeft,
            SfSymbol::ArrowRight,
            SfSymbol::ArrowClockwise,
            SfSymbol::ArrowCounterclockwise,
            SfSymbol::ArrowUpCircleFill,
            SfSymbol::ArrowDownCircleFill,
            SfSymbol::ArrowUturnLeft,
            SfSymbol::ArrowTriangle2Circlepath,
            SfSymbol::ArrowLeftAndRight,
        ],
    },
    Category {
        name: "Nature",
        symbols: &[
            SfSymbol::LeafFill,
            SfSymbol::DropFill,
            SfSymbol::FlameFill,
            SfSymbol::CloudFill,
            SfSymbol::SunMaxFill,
            SfSymbol::MoonFill,
            SfSymbol::Snowflake,
            SfSymbol::CloudRainFill,
            SfSymbol::Wind,
            SfSymbol::BoltFill,
        ],
    },
    Category {
        name: "People",
        symbols: &[
            SfSymbol::PersonFill,
            SfSymbol::Person2Fill,
            SfSymbol::Person3Fill,
            SfSymbol::FigureWalk,
            SfSymbol::FigureRun,
            SfSymbol::PersonCropCircleFill,
            SfSymbol::FigureWave,
            SfSymbol::FaceSmiling,
        ],
    },
    Category {
        name: "Objects",
        symbols: &[
            SfSymbol::StarFill,
            SfSymbol::HeartFill,
            SfSymbol::HouseFill,
            SfSymbol::GearshapeFill,
            SfSymbol::Magnifyingglass,
            SfSymbol::TrashFill,
            SfSymbol::FolderFill,
            SfSymbol::DocFill,
            SfSymbol::BookmarkFill,
            SfSymbol::FlagFill,
            SfSymbol::TagFill,
            SfSymbol::LockFill,
            SfSymbol::KeyFill,
        ],
    },
    Category {
        name: "Devices",
        symbols: &[
            SfSymbol::Iphone,
            SfSymbol::Ipad,
            SfSymbol::Macbook,
            SfSymbol::Desktopcomputer,
            SfSymbol::Applewatch,
            SfSymbol::Airpods,
            SfSymbol::PrinterFill,
            SfSymbol::Keyboard,
            SfSymbol::Display,
        ],
    },
    Category {
        name: "Transport",
        symbols: &[
            SfSymbol::CarFill,
            SfSymbol::Airplane,
            SfSymbol::Bicycle,
            SfSymbol::BusFill,
            SfSymbol::TramFill,
            SfSymbol::FerryFill,
            SfSymbol::FuelpumpFill,
            SfSymbol::Scooter,
        ],
    },
    Category {
        name: "Editing",
        symbols: &[
            SfSymbol::Pencil,
            SfSymbol::PaintbrushFill,
            SfSymbol::Scissors,
            SfSymbol::RulerFill,
            SfSymbol::Eyedropper,
            SfSymbol::WandAndStars,
            SfSymbol::PaintpaletteFill,
            SfSymbol::HammerFill,
            SfSymbol::WrenchFill,
        ],
    },
    Category {
        name: "Text Formatting",
        symbols: &[
            SfSymbol::Bold,
            SfSymbol::Italic,
            SfSymbol::Underline,
            SfSymbol::Strikethrough,
            SfSymbol::Textformat,
            SfSymbol::ListBullet,
            SfSymbol::ListNumber,
            SfSymbol::TextAlignleft,
            SfSymbol::DocTextFill,
        ],
    },
    Category {
        name: "Health",
        symbols: &[
            SfSymbol::HeartFill,
            SfSymbol::LungsFill,
            SfSymbol::CrossFill,
            SfSymbol::BandageFill,
            SfSymbol::PillsFill,
            SfSymbol::Stethoscope,
            SfSymbol::SyringeFill,
            SfSymbol::CrossCaseFill,
        ],
    },
];

static MODE_OPTIONS: &[(RenderingMode, &str)] = &[
    (RenderingMode::Monochrome, "Mono"),
    (RenderingMode::Hierarchical, "Hier."),
    (RenderingMode::Palette, "Palette"),
    (RenderingMode::Multicolor, "Multi"),
];

static WEIGHT_OPTIONS: &[(SymbolWeight, &str)] = &[
    (SymbolWeight::UltraLight, "UL"),
    (SymbolWeight::Thin, "T"),
    (SymbolWeight::Light, "L"),
    (SymbolWeight::Regular, "R"),
    (SymbolWeight::Medium, "M"),
    (SymbolWeight::Semibold, "SB"),
    (SymbolWeight::Bold, "B"),
    (SymbolWeight::Heavy, "H"),
    (SymbolWeight::Black, "Bk"),
];

static SCALE_OPTIONS: &[(SymbolScale, &str)] = &[
    (SymbolScale::Small, "S"),
    (SymbolScale::Medium, "M"),
    (SymbolScale::Large, "L"),
];

static EFFECT_OPTIONS: &[(Option<SymbolEffect>, &str)] = &[
    (None, "None"),
    (Some(SymbolEffect::VariableColor), "Variable Color"),
    (Some(SymbolEffect::Rotate), "Rotate"),
    (Some(SymbolEffect::Pulse), "Pulse"),
    (Some(SymbolEffect::Breathe), "Breathe"),
    (Some(SymbolEffect::Bounce), "Bounce"),
    (Some(SymbolEffect::Wiggle), "Wiggle"),
    (Some(SymbolEffect::ScaleUp), "Scale Up"),
    (Some(SymbolEffect::ScaleDown), "Scale Down"),
    (Some(SymbolEffect::Appear), "Appear"),
    (Some(SymbolEffect::Disappear), "Disappear"),
    (Some(SymbolEffect::Replace), "Replace"),
];

static REPEAT_OPTIONS: &[(Repeat, &str)] = &[
    (Repeat::Once, "Once"),
    (Repeat::Continuously, "Continuously"),
    (Repeat::WithDelay(Duration::from_secs(1)), "With Delay"),
];

static DIRECTION_OPTIONS: &[(Direction, &str)] = &[
    (Direction::Default, "Default"),
    (Direction::Up, "Up"),
    (Direction::Down, "Down"),
    (Direction::Left, "Left"),
    (Direction::Right, "Right"),
    (Direction::Clockwise, "CW"),
    (Direction::Counterclockwise, "CCW"),
];

static ANIMATE_OPTIONS: &[(Animate, &str)] = &[
    (Animate::WholeSymbol, "Whole Symbol"),
    (Animate::ByLayer, "By Layer"),
];

/// Inspector color presets (hex `0xRRGGBB`).
static COLOR_PRESETS: &[u32] = &[
    0x000000, 0x8E8E93, 0xFFFFFF, 0xFF3B30, 0xFF9500, 0xFFCC00, 0x34C759, 0x00C7BE, 0x30B0C7,
    0x007AFF, 0x5856D6, 0xAF52DE, 0xFF2D55, 0xA2845E,
];

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct Gallery {
    category: usize,
    selected: Option<SfSymbol>,
    mode: RenderingMode,
    /// Primary / secondary / tertiary colors as `0xRRGGBB` hex.
    primary: u32,
    secondary: u32,
    tertiary: u32,
    weight: SymbolWeight,
    scale: SymbolScale,
    /// Icon size in points (tiles and preview scale with it).
    icon_size: f32,
    variable_value: f64,
    effect: Option<SymbolEffect>,
    repeat: Repeat,
    direction: Direction,
    animate: Animate,
    pulses: u32,
    /// Effect cycle duration in seconds.
    duration: f32,
}

impl Gallery {
    fn new() -> Self {
        Self {
            category: 0,
            selected: Some(SfSymbol::CloudSunFill),
            mode: RenderingMode::Hierarchical,
            primary: 0x000000,
            secondary: 0x007AFF,
            tertiary: 0xFFCC00,
            weight: SymbolWeight::Regular,
            scale: SymbolScale::Medium,
            icon_size: 32.0,
            variable_value: 1.0,
            effect: None,
            repeat: Repeat::Continuously,
            direction: Direction::Default,
            animate: Animate::WholeSymbol,
            pulses: 1,
            duration: 1.0,
        }
    }

    /// Build an `Icon` with the current inspector configuration.
    ///
    /// When `animated` is set and an effect is selected, the effect options
    /// are applied and the icon gets the stable `gallery-preview` id — that is
    /// what makes `Replace` cross-fade when the selection changes.
    fn configured_icon(&self, name: &'static str, size: f32, animated: bool) -> Icon {
        let mut icon = Icon::new(name)
            .size(px(size))
            .color(rgb(self.primary))
            .palette_colors([rgb(self.primary), rgb(self.secondary), rgb(self.tertiary)])
            .weight(self.weight)
            .symbol_scale(self.scale)
            .rendering_mode(self.mode)
            .variable_value(self.variable_value);

        if let (Some(effect), true) = (self.effect, animated) {
            icon = icon
                .id("gallery-preview")
                .effect(effect)
                .repeat(self.repeat)
                .direction(self.direction)
                .animate(self.animate)
                .pulses(self.pulses)
                .duration(Duration::from_secs_f32(self.duration));
        }
        icon
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1024.), px(640.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("SF Symbols Gallery")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Gallery::new()),
        )
        .unwrap();
    });
}

// ---------------------------------------------------------------------------
// UI helpers
// ---------------------------------------------------------------------------

const ACCENT: u32 = 0x007AFF;
const CHIP_BG: u32 = 0xE5E5EA;
const CHIP_FG: u32 = 0x333333;
const SIDEBAR_BG: u32 = 0xF5F5F7;
const SEPARATOR: u32 = 0xD1D1D6;

/// A selectable chip button.
fn chip(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &mut Context<Gallery>,
    on_select: impl Fn(&mut Gallery, &mut Context<Gallery>) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_2()
        .h_6()
        .rounded_md()
        .flex()
        .items_center()
        .text_xs()
        .bg(if selected { rgb(ACCENT) } else { rgb(CHIP_BG) })
        .text_color(if selected {
            rgb(0xFFFFFF)
        } else {
            rgb(CHIP_FG)
        })
        .cursor(gpui::CursorStyle::PointingHand)
        .hover(move |style| {
            if selected {
                style.bg(rgb(ACCENT))
            } else {
                style.bg(rgb(SEPARATOR))
            }
        })
        .child(label.into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                on_select(this, cx);
                cx.notify();
            }),
        )
}

fn section_label(text: &'static str) -> impl IntoElement {
    div().text_xs().text_color(rgb(0x8E8E93)).child(text)
}

/// A group of option chips (one selected), as elements.
///
/// Uses a plain loop rather than iterator closures so `&mut Context`
/// reborrows cleanly.
fn chips<T: Copy + PartialEq + 'static>(
    options: &[(T, &'static str)],
    id_prefix: &str,
    current: T,
    cx: &mut Context<Gallery>,
    set: fn(&mut Gallery, T),
) -> Vec<gpui::AnyElement> {
    let mut out = Vec::with_capacity(options.len());
    for &(value, label) in options {
        out.push(
            chip(
                SharedString::from(format!("{id_prefix}-{label}")),
                label,
                value == current,
                cx,
                move |this, _| set(this, value),
            )
            .into_any_element(),
        );
    }
    out
}

/// A row of preset color swatches; disabled rows render dimmed and inert.
fn swatch_row(
    id_prefix: &'static str,
    current: u32,
    enabled: bool,
    cx: &mut Context<Gallery>,
    on_select: fn(&mut Gallery, u32),
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_1()
        .opacity(if enabled { 1.0 } else { 0.35 })
        .children(COLOR_PRESETS.iter().enumerate().map(|(i, &hex)| {
            let selected = enabled && hex == current;
            div()
                .id(SharedString::from(format!("{id_prefix}-swatch-{i}")))
                .size(px(18.))
                .rounded_sm()
                .bg(rgb(hex))
                .border_1()
                .border_color(if selected { rgb(ACCENT) } else { rgb(0xC7C7CC) })
                .cursor(gpui::CursorStyle::PointingHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        if enabled {
                            on_select(this, hex);
                            cx.notify();
                        }
                    }),
                )
        }))
}

/// A `-`/`+` stepper with a centered value label.
fn stepper(
    id_prefix: &'static str,
    display: impl Into<SharedString>,
    cx: &mut Context<Gallery>,
    on_dec: fn(&mut Gallery),
    on_inc: fn(&mut Gallery),
) -> impl IntoElement {
    fn button(
        id: SharedString,
        label: &'static str,
        on_click: fn(&mut Gallery),
        cx: &mut Context<Gallery>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .size(px(20.))
            .rounded_sm()
            .bg(rgb(CHIP_BG))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .cursor(gpui::CursorStyle::PointingHand)
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                    on_click(this);
                    cx.notify();
                }),
            )
    }

    div()
        .flex()
        .items_center()
        .gap_2()
        .child(button(
            SharedString::from(format!("{id_prefix}-dec")),
            "-",
            on_dec,
            cx,
        ))
        .child(
            div()
                .w(px(56.))
                .flex()
                .justify_center()
                .text_sm()
                .child(display.into()),
        )
        .child(button(
            SharedString::from(format!("{id_prefix}-inc")),
            "+",
            on_inc,
            cx,
        ))
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

impl Render for Gallery {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .bg(rgb(0xFFFFFF))
            .text_color(rgb(0x1D1D1F))
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .child(self.grid(cx))
                    .child(self.inspector(cx)),
            )
    }
}

impl Gallery {
    /// Left sidebar: category list.
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(180.))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SIDEBAR_BG))
            .border_r_1()
            .border_color(rgb(SEPARATOR))
            .child(
                div()
                    .p_3()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Categories"),
            )
            .child(
                div()
                    .id("category-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .pb_2()
                    .children(CATEGORIES.iter().enumerate().map(|(i, category)| {
                        let selected = i == self.category;
                        div()
                            .id(SharedString::from(format!("category-{i}")))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .cursor(gpui::CursorStyle::PointingHand)
                            .when(!selected, |el| el.hover(|s| s.bg(rgb(0xE8E8EC))))
                            .when(selected, |el| el.bg(rgb(0xDCDCE0)))
                            .child(category.name)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                    this.category = i;
                                    cx.notify();
                                }),
                            )
                    })),
            )
    }

    /// Center: symbol grid for the selected category.
    fn grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let category = &CATEGORIES[self.category];
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(category.name),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8E8E93))
                            .child(format!("{} symbols", category.symbols.len())),
                    ),
            )
            .child(
                div()
                    .id("symbol-grid")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .pb_4()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(category.symbols.iter().map(|symbol| {
                                let selected = self.selected == Some(*symbol);
                                div()
                                    .id(SharedString::from(format!("symbol-{}", symbol.name())))
                                    .size(px(self.icon_size + 24.))
                                    .rounded_lg()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(if selected {
                                        rgb(0xE3EFFF)
                                    } else {
                                        rgb(SIDEBAR_BG)
                                    })
                                    .border_1()
                                    .border_color(if selected {
                                        rgb(ACCENT)
                                    } else {
                                        rgb(0xE5E5EA)
                                    })
                                    .cursor(gpui::CursorStyle::PointingHand)
                                    .child(self.configured_icon(
                                        symbol.name(),
                                        self.icon_size,
                                        false,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, _event: &MouseDownEvent, _window, cx| {
                                                this.selected = Some(*symbol);
                                                cx.notify();
                                            },
                                        ),
                                    )
                            })),
                    ),
            )
    }

    /// Right: inspector with a live preview and all configuration controls.
    fn inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_palette = self.mode == RenderingMode::Palette;
        div()
            .w(px(300.))
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(SEPARATOR))
            .child(
                div()
                    .id("inspector-panel")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    // Live preview (animated when an effect is selected)
                    .child(
                        div()
                            .h(px(140.))
                            .rounded_lg()
                            .bg(rgb(SIDEBAR_BG))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when_some(self.selected, |el, symbol| {
                                el.child(self.configured_icon(
                                    symbol.name(),
                                    self.icon_size * 2.0,
                                    true,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_xs()
                            .text_color(rgb(0x8E8E93))
                            .child(self.selected.map(|s| s.name()).unwrap_or("no selection")),
                    )
                    // Rendering mode
                    .child(section_label("Rendering Mode"))
                    .child(div().flex().flex_wrap().gap_1().children(chips(
                        MODE_OPTIONS,
                        "mode",
                        self.mode,
                        cx,
                        |this, mode| this.mode = mode,
                    )))
                    // Colors
                    .child(section_label("Primary"))
                    .child(swatch_row(
                        "primary",
                        self.primary,
                        true,
                        cx,
                        |this, hex| this.primary = hex,
                    ))
                    .child(section_label("Secondary (Palette)"))
                    .child(swatch_row(
                        "secondary",
                        self.secondary,
                        is_palette,
                        cx,
                        |this, hex| this.secondary = hex,
                    ))
                    .child(section_label("Tertiary (Palette)"))
                    .child(swatch_row(
                        "tertiary",
                        self.tertiary,
                        is_palette,
                        cx,
                        |this, hex| this.tertiary = hex,
                    ))
                    // Weight
                    .child(section_label("Weight"))
                    .child(div().flex().flex_wrap().gap_1().children(chips(
                        WEIGHT_OPTIONS,
                        "weight",
                        self.weight,
                        cx,
                        |this, weight| this.weight = weight,
                    )))
                    // Scale
                    .child(section_label("Scale"))
                    .child(div().flex().flex_wrap().gap_1().children(chips(
                        SCALE_OPTIONS,
                        "scale",
                        self.scale,
                        cx,
                        |this, scale| this.scale = scale,
                    )))
                    // Size
                    .child(section_label("Size"))
                    .child(stepper(
                        "size",
                        format!("{:.0} pt", self.icon_size),
                        cx,
                        |this| this.icon_size = (this.icon_size - 4.0).max(16.0),
                        |this| this.icon_size = (this.icon_size + 4.0).min(64.0),
                    ))
                    // Variable Color value
                    .child(section_label("Variable Value"))
                    .child(stepper(
                        "variable",
                        format!("{:.1}", self.variable_value),
                        cx,
                        |this| this.variable_value = (this.variable_value - 0.1).max(0.0),
                        |this| this.variable_value = (this.variable_value + 0.1).min(1.0),
                    ))
                    // Animation
                    .child(section_label("Animation"))
                    .child(div().flex().flex_wrap().gap_1().children(chips(
                        EFFECT_OPTIONS,
                        "effect",
                        self.effect,
                        cx,
                        |this, effect| this.effect = effect,
                    )))
                    // Effect options (only when an effect is selected)
                    .when_some(self.effect, |el, _| {
                        el.child(section_label("Repeat"))
                            .child(div().flex().flex_wrap().gap_1().children(chips(
                                REPEAT_OPTIONS,
                                "repeat",
                                self.repeat,
                                cx,
                                |this, repeat| this.repeat = repeat,
                            )))
                            .child(section_label("Direction"))
                            .child(div().flex().flex_wrap().gap_1().children(chips(
                                DIRECTION_OPTIONS,
                                "direction",
                                self.direction,
                                cx,
                                |this, direction| this.direction = direction,
                            )))
                            .child(section_label("Pulses"))
                            .child(stepper(
                                "pulses",
                                self.pulses.to_string(),
                                cx,
                                |this| this.pulses = this.pulses.saturating_sub(1).max(1),
                                |this| this.pulses = (this.pulses + 1).min(5),
                            ))
                            .child(section_label("Duration"))
                            .child(stepper(
                                "duration",
                                format!("{:.1} s", self.duration),
                                cx,
                                |this| this.duration = (this.duration - 0.2).max(0.2),
                                |this| this.duration = (this.duration + 0.2).min(3.0),
                            ))
                            .child(section_label("Animate"))
                            .child(div().flex().flex_wrap().gap_1().children(chips(
                                ANIMATE_OPTIONS,
                                "animate",
                                self.animate,
                                cx,
                                |this, animate| this.animate = animate,
                            )))
                    }),
            )
    }
}
