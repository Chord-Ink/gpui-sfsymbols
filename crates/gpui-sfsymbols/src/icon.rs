//! GPUI Icon component for SF Symbols.
//!
//! This module provides a high-level Icon API similar to GPUI Components,
//! allowing SF Symbols to be used directly as GPUI elements.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, App, ElementId, Entity, Hsla, ImageSource, IntoElement, ParentElement,
    Pixels, RenderImage, SharedString, Styled, Window, div, img, px,
};

use crate::platform::MAX_PALETTE_COLORS;
use crate::{
    Animate, Direction, RenderingMode, Repeat, SfSymbol, SymbolColor, SymbolEffect, SymbolScale,
    SymbolWeight,
};

/// Default frames-per-second for bitmap-sampled animations (Variable Color,
/// Rotate).
const DEFAULT_ANIMATION_FPS: u8 = 30;

/// Default cycle duration and repeat behavior per effect, chosen to feel
/// close to Apple's `NSSymbolEffect` defaults.
fn effect_defaults(effect: SymbolEffect) -> (Duration, Repeat) {
    match effect {
        SymbolEffect::Pulse => (Duration::from_millis(1000), Repeat::Continuously),
        SymbolEffect::Breathe => (Duration::from_millis(1400), Repeat::Continuously),
        SymbolEffect::Bounce => (Duration::from_millis(700), Repeat::Continuously),
        SymbolEffect::Wiggle => (Duration::from_millis(700), Repeat::Continuously),
        SymbolEffect::ScaleUp | SymbolEffect::ScaleDown => {
            (Duration::from_millis(800), Repeat::Continuously)
        }
        SymbolEffect::VariableColor => (Duration::from_millis(1200), Repeat::Continuously),
        SymbolEffect::Rotate => (Duration::from_millis(2000), Repeat::Continuously),
        SymbolEffect::Appear | SymbolEffect::Disappear => {
            (Duration::from_millis(300), Repeat::Once)
        }
        SymbolEffect::Replace => (Duration::from_millis(300), Repeat::Once),
    }
}

/// Ping-pong wave 0 → 1 → 0, `pulses` times per cycle (always non-negative,
/// so it is safe to feed into opacity/scale).
fn oscillate(delta: f32, pulses: u32) -> f32 {
    (delta * pulses as f32 * std::f32::consts::PI).sin().abs()
}

/// Full wave -1 → 1 → 0 over `pulses` oscillations per cycle (for Wiggle).
fn wag(delta: f32, pulses: u32) -> f32 {
    (delta * pulses as f32 * 2.0 * std::f32::consts::PI).sin()
}

/// Build the GPUI animation and progress mapping for a [`Repeat`] mode.
///
/// For [`Repeat::WithDelay`] the animation cycle is padded by the delay and
/// progress holds at the end state during the pause, so looping effects
/// restart seamlessly.
fn animation_for(duration: Duration, repeat: Repeat) -> (Animation, Box<dyn Fn(f32) -> f32>) {
    match repeat {
        Repeat::Once => (Animation::new(duration), Box::new(|delta| delta)),
        Repeat::Continuously => (Animation::new(duration).repeat(), Box::new(|delta| delta)),
        Repeat::WithDelay(delay) => {
            let total = duration + delay;
            let active = duration.as_secs_f32() / total.as_secs_f32();
            (
                Animation::new(total).repeat(),
                Box::new(move |delta| (delta / active).min(1.0)),
            )
        }
    }
}

/// Compute display dimensions preserving the symbol's aspect ratio, where
/// `target_size` is the maximum dimension.
fn display_dims(width: u32, height: u32, target_size: f32) -> (f32, f32) {
    let aspect_ratio = width as f32 / height as f32;
    if aspect_ratio >= 1.0 {
        // Wider than tall: width is the constraining dimension
        (target_size, target_size / aspect_ratio)
    } else {
        // Taller than wide: height is the constraining dimension
        (target_size * aspect_ratio, target_size)
    }
}

/// Trait for types that can provide an SF Symbol name.
///
/// Implement this trait for your own enums to use them with [`Icon`].
///
/// # Example
///
/// ```rust,ignore
/// use gpui_sfsymbols::{IconName, Icon};
///
/// #[derive(Clone, Copy)]
/// enum MyIcon {
///     Star,
///     Heart,
///     Gear,
/// }
///
/// impl IconName for MyIcon {
///     fn name(&self) -> &'static str {
///         match self {
///             MyIcon::Star => "star.fill",
///             MyIcon::Heart => "heart.fill",
///             MyIcon::Gear => "gearshape.fill",
///         }
///     }
/// }
///
/// // Now you can use it like GPUI Components:
/// let icon = Icon::from_name(MyIcon::Star).color(gpui::rgb(0xFF0000));
/// ```
pub trait IconName: Clone + 'static {
    /// Returns the SF Symbol name for this icon.
    fn name(&self) -> &'static str;
}

/// Implement IconName for &'static str for convenience.
impl IconName for &'static str {
    fn name(&self) -> &'static str {
        self
    }
}

// Implement IconName for sfsymbols enums
#[cfg(feature = "presets")]
macro_rules! impl_icon_name_for_sfsymbols {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IconName for $ty {
                fn name(&self) -> &'static str {
                    <$ty>::name(self)
                }
            }
        )*
    };
}

#[cfg(feature = "presets")]
impl_icon_name_for_sfsymbols!(
    sfsymbols::SfSymbolV1,
    sfsymbols::SfSymbolV2,
    sfsymbols::SfSymbolV3,
    sfsymbols::SfSymbolV4,
    sfsymbols::SfSymbolV5,
    sfsymbols::SfSymbolV6,
    sfsymbols::SfSymbolV7,
    sfsymbols::SfSymbol,
);

/// A GPUI-compatible Icon component for rendering SF Symbols.
///
/// This struct implements `IntoElement` and can be used directly in GPUI views,
/// similar to how icons work in GPUI Components.
///
/// # Example
///
/// ```rust,ignore
/// use gpui_sfsymbols::Icon;
/// use gpui::px;
///
/// fn view(window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
///     div()
///         .child(Icon::new("star.fill"))
///         .child(Icon::new("heart.fill").color(gpui::rgb(0xFF0000)).size(px(24.)))
/// }
/// ```
#[derive(Clone, IntoElement)]
pub struct Icon {
    name: SharedString,
    size: Pixels,
    color: Hsla,
    palette_colors: Vec<Hsla>,
    weight: SymbolWeight,
    symbol_scale: SymbolScale,
    rendering_mode: RenderingMode,
    variable_value: Option<f64>,
    id: Option<ElementId>,
    effect: Option<SymbolEffect>,
    duration: Option<Duration>,
    repeat: Option<Repeat>,
    animate: Animate,
    direction: Direction,
    pulses: u32,
}

impl Icon {
    /// Create a new Icon with the given SF Symbol name.
    ///
    /// Default size is 16px, default color is black.
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            size: px(16.),
            color: gpui::black(),
            palette_colors: Vec::new(),
            weight: SymbolWeight::default(),
            symbol_scale: SymbolScale::default(),
            rendering_mode: RenderingMode::default(),
            variable_value: None,
            id: None,
            effect: None,
            duration: None,
            repeat: None,
            animate: Animate::default(),
            direction: Direction::default(),
            pulses: 1,
        }
    }

    /// Create a new Icon from a type implementing [`IconName`].
    pub fn from_name<T: IconName>(icon: T) -> Self {
        Self::new(icon.name())
    }

    /// Set the size of the icon in pixels.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Set the color of the icon using any type that converts to [`Hsla`],
    /// mirroring SwiftUI's `.foregroundStyle(...)` for single-color modes.
    ///
    /// Accepts GPUI color types: `Hsla`, `Rgba`, or use helper functions like
    /// `gpui::rgb(0xFF0000)`, `gpui::hsla(h, s, l, a)`. The color's alpha
    /// (opacity) is honored.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui::{rgb, hsla};
    ///
    /// Icon::new("star.fill").color(rgb(0xFF0000));
    /// Icon::new("heart.fill").color(hsla(0.0, 1.0, 0.5, 0.5)); // 50% opacity
    /// ```
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = color.into();
        self
    }

    /// Set the symbol weight (default: Regular).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, SymbolWeight};
    ///
    /// Icon::new("star.fill").weight(SymbolWeight::Bold);
    /// ```
    pub fn weight(mut self, weight: SymbolWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set the symbol scale (default: Medium).
    ///
    /// Controls the overall visual weight of the symbol independent of point size.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, SymbolScale};
    ///
    /// Icon::new("star.fill").symbol_scale(SymbolScale::Large);
    /// ```
    pub fn symbol_scale(mut self, scale: SymbolScale) -> Self {
        self.symbol_scale = scale;
        self
    }

    /// Set the rendering mode (default: Hierarchical).
    ///
    /// - `Monochrome`: Single color for entire symbol
    /// - `Hierarchical`: Primary color with automatic opacity layers
    /// - `Palette`: Multiple distinct colors
    /// - `Multicolor`: Original multicolor design
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, RenderingMode};
    ///
    /// Icon::new("cloud.sun.fill").rendering_mode(RenderingMode::Multicolor);
    /// ```
    pub fn rendering_mode(mut self, mode: RenderingMode) -> Self {
        self.rendering_mode = mode;
        self
    }

    /// Set the palette colors used with [`RenderingMode::Palette`].
    ///
    /// Up to 3 colors are used — primary, secondary, tertiary — mirroring
    /// SwiftUI's `.foregroundStyle(c1, c2, c3)`; extra colors are ignored.
    /// Accepts any GPUI color (`Hsla`, `Rgba`, `gpui::rgb(hex)`, ...).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui::rgb;
    /// use gpui_sfsymbols::{Icon, RenderingMode};
    ///
    /// Icon::new("cloud.sun.fill")
    ///     .rendering_mode(RenderingMode::Palette)
    ///     .palette_colors([rgb(0x007AFF), rgb(0xFFCC00)]);
    /// ```
    pub fn palette_colors<I, C>(mut self, colors: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Hsla>,
    {
        self.palette_colors = colors
            .into_iter()
            .take(MAX_PALETTE_COLORS)
            .map(Into::into)
            .collect();
        self
    }

    /// Set the Variable Color value for the symbol (macOS 14+, `0.0 - 1.0`).
    ///
    /// `0.0` shows no emphasized layers, `1.0` shows all of them (for symbols
    /// that support Variable Color). Non-finite values unset the value; on
    /// macOS < 14 the symbol renders in its default appearance.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Icon::new("wifi").variable_value(0.5);
    /// ```
    pub fn variable_value(mut self, value: impl Into<Option<f64>>) -> Self {
        self.variable_value = value
            .into()
            .and_then(|v| v.is_finite().then(|| v.clamp(0.0, 1.0)));
        self
    }

    /// Animate the icon with the given [`SymbolEffect`].
    ///
    /// Animation state is keyed by the icon's [`ElementId`] (see [`Icon::id`]);
    /// animated icons in the same view must have unique ids.
    ///
    /// **Note for [`SymbolEffect::Replace`]:** set a stable [`Icon::id`] —
    /// the default id derives from the symbol name, which is exactly what
    /// changes during a replace transition.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, SymbolEffect};
    ///
    /// Icon::new("wifi").effect(SymbolEffect::VariableColor);
    /// Icon::new("bell.badge").effect(SymbolEffect::Pulse);
    /// // Play/pause toggle with cross-fade:
    /// Icon::new("play.fill").id("transport").effect(SymbolEffect::Replace);
    /// ```
    pub fn effect(mut self, effect: SymbolEffect) -> Self {
        self.effect = Some(effect);
        self
    }

    /// Override the effect cycle duration (see [`SymbolEffect`] for the
    /// per-effect defaults).
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set how the animation repeats, mirroring the SF Symbols inspector's
    /// `Once` / `Repeat with Delay` / `Continuously`.
    ///
    /// Default: ongoing effects (`Pulse`, `Breathe`, `Bounce`, `Wiggle`,
    /// `ScaleUp`, `ScaleDown`) repeat continuously; `Appear`, `Disappear` and
    /// `Replace` play once. Bitmap-sampled effects (`VariableColor`,
    /// `Rotate`) always loop — this option has no effect on them.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, SymbolEffect, Repeat};
    /// use std::time::Duration;
    ///
    /// Icon::new("bell.badge.fill")
    ///     .effect(SymbolEffect::Pulse)
    ///     .repeat(Repeat::WithDelay(Duration::from_secs(2)));
    /// ```
    pub fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = Some(repeat);
        self
    }

    /// Set whether the effect animates the whole symbol or layer by layer,
    /// mirroring the SF Symbols inspector's `Animate` option.
    ///
    /// Only [`SymbolEffect::VariableColor`] is natively per-layer (that is
    /// what `variableValue` rendering does); all other effects animate the
    /// whole symbol regardless of this setting, because a rendered bitmap no
    /// longer carries layer information.
    pub fn animate(mut self, animate: Animate) -> Self {
        self.animate = animate;
        self
    }

    /// Set the number of pulses (oscillations) per animation cycle
    /// (default: 1), for the oscillating effects `Pulse`, `Breathe`,
    /// `Bounce`, `Wiggle`, `ScaleUp` and `ScaleDown`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// Icon::new("star.fill").effect(SymbolEffect::ScaleUp).pulses(3);
    /// ```
    pub fn pulses(mut self, pulses: u32) -> Self {
        self.pulses = pulses.max(1);
        self
    }

    /// Set the effect direction, mirroring Apple's `.up` / `.down` / `.left`
    /// / `.right` / `.clockwise` / `.counterClockwise` options.
    ///
    /// Supported by `Bounce`, `Wiggle`, `Rotate`, `Appear`, `Disappear` and
    /// `Replace` — see [`Direction`] for the per-effect table; ignored by
    /// other effects.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use gpui_sfsymbols::{Icon, SymbolEffect, Direction};
    ///
    /// Icon::new("arrow.clockwise")
    ///     .effect(SymbolEffect::Rotate)
    ///     .direction(Direction::Counterclockwise);
    /// ```
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Set the [`ElementId`] used to key animation state (defaults to the
    /// symbol name). Animated icons in the same view must have unique ids.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The element id keying this icon's animation state.
    fn element_id(&self) -> ElementId {
        self.id
            .clone()
            .unwrap_or_else(|| ElementId::from(self.name.clone()))
    }

    /// The effect cycle duration and repeat behavior, honoring overrides.
    fn effect_timing(&self, effect: SymbolEffect) -> (Duration, Repeat) {
        let (default_duration, default_repeat) = effect_defaults(effect);
        (
            self.duration.unwrap_or(default_duration),
            self.repeat.unwrap_or(default_repeat),
        )
    }

    /// Build the low-level symbol renderer with this icon's configuration.
    fn base_symbol(&self, scale_factor: f32) -> SfSymbol {
        let size: f32 = self.size.into();
        SfSymbol::new(self.name.as_ref())
            .size(size)
            .scale(scale_factor)
            .color(SymbolColor::from(self.color))
            .palette_colors(self.palette_colors.iter().map(|&c| SymbolColor::from(c)))
            .weight(self.weight)
            .symbol_scale(self.symbol_scale)
            .rendering_mode(self.rendering_mode)
            .variable_value(self.variable_value)
    }

    /// Render the icon to an image with its actual dimensions.
    ///
    /// `scale_factor` is the display scale of the target window (e.g. 2.0 for
    /// Retina) so the raster matches the screen the icon is shown on.
    ///
    /// Returns `(image, width, height)` where width and height are the actual
    /// rendered pixel dimensions (may not be square).
    fn render_image(&self, scale_factor: f32) -> Option<(Arc<RenderImage>, u32, u32)> {
        self.base_symbol(scale_factor).render_with_size()
    }

    /// Warn about a failed render and produce an empty element.
    fn render_failed(&self) -> gpui::AnyElement {
        log::warn!(
            "gpui-sfsymbols: failed to render SF Symbol {:?} \
             (unknown name or invalid parameters)",
            self.name
        );
        gpui::Empty.into_any_element()
    }

    /// Static render path (no effect).
    fn render_static(self, window: &mut Window) -> gpui::AnyElement {
        let target_size: f32 = self.size.into();
        let scale_factor = window.scale_factor();

        match self.render_image(scale_factor) {
            Some((image, width, height)) => {
                sized_img(image, width, height, target_size).into_any_element()
            }
            None => self.render_failed(),
        }
    }

    /// Bitmap-sampled path (Variable Color, Rotate): a pre-rendered
    /// multi-frame image that GPUI loops.
    fn render_bitmap_effect(self, effect: SymbolEffect, window: &mut Window) -> gpui::AnyElement {
        let target_size: f32 = self.size.into();
        let scale_factor = window.scale_factor();
        let (duration, _) = self.effect_timing(effect);

        let rendered = self.base_symbol(scale_factor).render_animated(
            effect,
            self.direction,
            DEFAULT_ANIMATION_FPS,
            duration,
        );
        match rendered {
            Some((image, width, height)) => {
                sized_img(image, width, height, target_size).into_any_element()
            }
            None => self.render_failed(),
        }
    }

    /// Element-animated path (Pulse, Breathe, Bounce, Wiggle, Scale, Appear,
    /// Disappear).
    fn render_with_effect(self, effect: SymbolEffect, window: &mut Window) -> gpui::AnyElement {
        let target_size: f32 = self.size.into();
        let scale_factor = window.scale_factor();
        let id = self.element_id();
        let pulses = self.pulses;

        let Some((image, width, height)) = self.render_image(scale_factor) else {
            return self.render_failed();
        };

        let (dw, dh) = display_dims(width, height, target_size);
        let (duration, repeat) = self.effect_timing(effect);
        let (animation, progress) = animation_for(duration, repeat);
        let base = img(ImageSource::Render(image)).w(px(dw)).h(px(dh));

        match effect {
            SymbolEffect::Pulse => base
                .with_animation(id, animation, move |el, delta| {
                    el.opacity(0.2 + 0.8 * oscillate(progress(delta), pulses))
                })
                .into_any_element(),
            SymbolEffect::Breathe => base
                .with_animation(id, animation, move |el, delta| {
                    let wave = oscillate(progress(delta), pulses);
                    let s = 1.0 + 0.05 * wave;
                    el.opacity(0.65 + 0.35 * wave).w(px(dw * s)).h(px(dh * s))
                })
                .into_any_element(),
            SymbolEffect::Appear => {
                // Slide in from the direction while fading in (Up = enters
                // from below, moving up).
                let start_offset = match self.direction {
                    Direction::Up => target_size * 0.25,
                    Direction::Down => -target_size * 0.25,
                    _ => 0.0,
                };
                div()
                    .child(base)
                    .with_animation(id, animation, move |el, delta| {
                        let d = progress(delta);
                        el.opacity(d).mt(px(start_offset * (1.0 - d)))
                    })
                    .into_any_element()
            }
            SymbolEffect::Disappear => {
                // Slide out toward the direction while fading out.
                let end_offset = match self.direction {
                    Direction::Up => -target_size * 0.25,
                    Direction::Down => target_size * 0.25,
                    _ => 0.0,
                };
                div()
                    .child(base)
                    .with_animation(id, animation, move |el, delta| {
                        let d = progress(delta);
                        el.opacity(1.0 - d).mt(px(end_offset * d))
                    })
                    .into_any_element()
            }
            SymbolEffect::ScaleUp => base
                .with_animation(id, animation, move |el, delta| {
                    let s = 1.0 + 0.2 * oscillate(progress(delta), pulses);
                    el.w(px(dw * s)).h(px(dh * s))
                })
                .into_any_element(),
            SymbolEffect::ScaleDown => base
                .with_animation(id, animation, move |el, delta| {
                    let s = 1.0 - 0.2 * oscillate(progress(delta), pulses);
                    el.w(px(dw * s)).h(px(dh * s))
                })
                .into_any_element(),
            SymbolEffect::Bounce => {
                // Up bobs upward (negative offset), Down bobs downward.
                let sign = if self.direction == Direction::Down {
                    1.0
                } else {
                    -1.0
                };
                let amplitude = target_size * 0.25;
                div()
                    .child(base)
                    .with_animation(id, animation, move |el, delta| {
                        el.mt(px(sign * amplitude * oscillate(progress(delta), pulses)))
                    })
                    .into_any_element()
            }
            SymbolEffect::Wiggle => {
                // Translation axis and sign from the direction (default Right).
                let (vertical, sign) = match self.direction {
                    Direction::Left => (false, -1.0),
                    Direction::Up => (true, -1.0),
                    Direction::Down => (true, 1.0),
                    _ => (false, 1.0),
                };
                let amplitude = target_size * 0.1;
                div()
                    .child(base)
                    .with_animation(id, animation, move |el, delta| {
                        let offset = sign * amplitude * wag(progress(delta), pulses);
                        if vertical {
                            el.mt(px(offset))
                        } else {
                            el.ml(px(offset))
                        }
                    })
                    .into_any_element()
            }
            SymbolEffect::VariableColor | SymbolEffect::Rotate | SymbolEffect::Replace => {
                unreachable!("bitmap-sampled effects and Replace have dedicated render paths")
            }
        }
    }

    /// Replace path: cross-fade to the new symbol when the name changes.
    fn render_replace(self, window: &mut Window, cx: &mut App) -> gpui::AnyElement {
        /// Per-id state tracking the previously displayed symbol.
        struct ReplaceState {
            name: SharedString,
            image: Option<(Arc<RenderImage>, u32, u32)>,
            /// The symbol being faded out, and when the fade started.
            previous: Option<(Arc<RenderImage>, u32, u32, Instant)>,
        }

        let target_size: f32 = self.size.into();
        let scale_factor = window.scale_factor();
        let id = self.element_id();
        let (duration, _) = self.effect_timing(SymbolEffect::Replace);

        let state: Entity<ReplaceState> =
            window.use_keyed_state(id.clone(), cx, |_, _| ReplaceState {
                name: SharedString::default(),
                image: None,
                previous: None,
            });

        let Some((cur_image, cur_w, cur_h)) = self.render_image(scale_factor) else {
            return self.render_failed();
        };

        // Transition bookkeeping. Updates notify the view, so they only run
        // when something actually changed (no render loop).
        let name_changed = state.read(cx).name != self.name;
        if name_changed {
            state.update(cx, |s, _| {
                // Fade out the symbol we were showing (if any).
                s.previous = s
                    .image
                    .take()
                    .filter(|_| !s.name.is_empty())
                    .map(|(img, w, h)| (img, w, h, Instant::now()));
                s.name = self.name.clone();
                s.image = Some((cur_image.clone(), cur_w, cur_h));
            });
        } else {
            let fade_done = state
                .read(cx)
                .previous
                .as_ref()
                .is_some_and(|(_, _, _, started)| started.elapsed() >= duration);
            if fade_done {
                state.update(cx, |s, _| s.previous = None);
            }
        }

        let previous = state.read(cx).previous.clone();
        match previous {
            // Fade in progress: cross-fade old -> new. Direction adds a
            // slide: Up moves the incoming symbol up from below (≈ Apple's
            // `.offUp`), Down slides the outgoing symbol downward (≈
            // `.downUp`).
            Some((prev_image, prev_w, prev_h, _started)) => {
                let (dw, dh) = display_dims(cur_w, cur_h, target_size);
                let slide = target_size * 0.25;
                let incoming_mt = if self.direction == Direction::Up {
                    slide
                } else {
                    0.0
                };
                let outgoing_mt = if self.direction == Direction::Down {
                    slide
                } else {
                    0.0
                };
                div()
                    .relative()
                    .w(px(dw))
                    .h(px(dh))
                    .with_animation(id, Animation::new(duration), move |el, delta| {
                        el.child(
                            sized_img(cur_image.clone(), cur_w, cur_h, target_size)
                                .opacity(delta)
                                .mt(px(incoming_mt * (1.0 - delta))),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .opacity(1.0 - delta)
                                .mt(px(outgoing_mt * delta))
                                .child(sized_img(prev_image.clone(), prev_w, prev_h, target_size)),
                        )
                    })
                    .into_any_element()
            }
            // No transition: static render.
            None => sized_img(cur_image, cur_w, cur_h, target_size).into_any_element(),
        }
    }
}

/// Build a sized `img` element preserving the symbol's aspect ratio.
fn sized_img(image: Arc<RenderImage>, width: u32, height: u32, target_size: f32) -> gpui::Img {
    let (w, h) = display_dims(width, height, target_size);
    img(ImageSource::Render(image)).w(px(w)).h(px(h))
}

impl gpui::RenderOnce for Icon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        match self.effect {
            Some(effect) if effect.is_bitmap_sampled() => self.render_bitmap_effect(effect, window),
            Some(SymbolEffect::Replace) => self.render_replace(window, cx),
            Some(effect) => self.render_with_effect(effect, window),
            None => self.render_static(window),
        }
    }
}

/// Convenience macro for creating Icon enums with SF Symbol mappings.
///
/// # Example
///
/// ```rust,ignore
/// use gpui_sfsymbols::define_icons;
///
/// define_icons! {
///     pub enum AppIcon {
///         Star => "star.fill",
///         Heart => "heart.fill",
///         Settings => "gearshape.fill",
///     }
/// }
///
/// // Use it:
/// let icon = Icon::from_name(AppIcon::Star);
/// ```
#[macro_export]
macro_rules! define_icons {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($variant:ident => $symbol:literal),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($variant),*
        }

        impl $crate::IconName for $name {
            fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant => $symbol),*
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pulse = one 0 -> 1 -> 0 wave per cycle; `pulses` multiplies the
    /// number of oscillations inside the same cycle.
    #[test]
    fn oscillate_with_pulses() {
        assert!(oscillate(0.0, 1).abs() < 1e-6);
        assert!((oscillate(0.5, 1) - 1.0).abs() < 1e-6);
        assert!(oscillate(1.0, 1).abs() < 1e-6);
        // Two pulses: wave peaks at 1/4 and 3/4, back to base at half-cycle.
        assert!((oscillate(0.25, 2) - 1.0).abs() < 1e-6);
        assert!(oscillate(0.5, 2).abs() < 1e-6);
        assert!((oscillate(0.75, 2) - 1.0).abs() < 1e-6);
    }

    /// Wiggle uses a full sine wave (left-right-left), returning to zero.
    #[test]
    fn wag_full_wave() {
        assert!(wag(0.0, 1).abs() < 1e-6);
        assert!(wag(0.25, 1) > 0.99);
        assert!(wag(0.75, 1) < -0.99);
        assert!(wag(1.0, 1).abs() < 1e-5);
    }

    /// Repeat::WithDelay pads the cycle; progress holds at the end state
    /// during the pause so looping effects restart seamlessly.
    #[test]
    fn with_delay_holds_end_state() {
        let (_animation, progress) = animation_for(
            Duration::from_millis(500),
            Repeat::WithDelay(Duration::from_millis(500)),
        );
        // Active half of the padded cycle maps 0..0.5 -> 0..1.
        assert!(progress(0.0).abs() < 1e-6);
        assert!((progress(0.25) - 0.5).abs() < 1e-6);
        // Delay half holds the end state.
        assert_eq!(progress(0.5), 1.0);
        assert_eq!(progress(0.75), 1.0);
        assert_eq!(progress(1.0), 1.0);
    }

    /// Once and Continuously pass progress through unchanged.
    #[test]
    fn once_and_continuous_are_identity() {
        let (_a, once) = animation_for(Duration::from_millis(300), Repeat::Once);
        let (_b, cont) = animation_for(Duration::from_millis(300), Repeat::Continuously);
        for delta in [0.0, 0.3, 0.7, 1.0] {
            assert_eq!(once(delta), delta);
            assert_eq!(cont(delta), delta);
        }
    }
}
