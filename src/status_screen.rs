//! Status screen drawn on the LCD.
//!
//! The screen is laid out in three bands on the 280x240 landscape panel: the transport and the
//! held modifiers on the top row, the layer in the middle as the largest element, and the battery
//! of each half at the bottom.
//!
//! ```text
//! +----------------------------------+
//! | <conn>                 <mod key> |
//! |                                  |
//! |             <layer>              |
//! |                                  |
//! | <bat left>            <bat right>|
//! +----------------------------------+
//! ```
//!
//! Nothing here knows about the panel: [`draw`] takes any `Rgb565` draw target, reads its size
//! from the target itself and receives everything it shows as a [`Status`], so the screen can be
//! changed without touching [`crate::lcd`].
//!
//! On top of the three bands comes the brightness overlay, which is shown only while the backlight
//! is being adjusted and covers the middle of the screen while it is.
//!
//! Each of the four knows the rows it occupies as a [`Band`], and is cleared and redrawn on its
//! own: [`plan`] takes what the panel already shows and works out which bands the new [`Status`]
//! changes and which rows those come to, and [`draw`] draws that [`Redraw`]. A band spans the full
//! width of the screen because the framebuffer is row major, which makes a range of whole rows one
//! contiguous slice and so one transfer.
//!
//! The two are separate because [`crate::lcd`] holds only part of the screen at a time and so has
//! to draw the same redraw once per part, while what the redraw consists of is worked out once.
//! [`draw`] is therefore free of side effects: calling it again over a different part of the screen
//! produces the same picture.
//!
//! This module holds what the screen shares — the state it is given, the colors and fonts more than
//! one of them uses, and the layer in the middle — while the parts that stand on their own live
//! beside it: the rows a band covers in [`band`], the top row in [`header`], the icon bitmaps in
//! [`icons`], the battery band in [`battery`] and the brightness overlay in [`brightness`]. Every
//! coordinate, color, font and threshold is a constant of whichever of them owns it. The values
//! come from `.tmp/screen.json`, which is what the layout was settled against in a browser preview.

mod band;
mod battery;
mod brightness;
mod header;
mod icons;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use rmk::types::battery::BatteryStatus;
use rmk::types::connection::ConnectionStatus;
use rmk::types::modifier::ModifierCombination;
use u8g2_fonts::fonts::u8g2_font_inb42_mr;
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};
use u8g2_fonts::{Content, FontRenderer};

pub use crate::status_screen::band::Band;
use crate::status_screen::band::DirtyRows;

/// Number of halves whose battery is shown.
pub const HALF_COUNT: usize = 2;

/// Index of the left half in [`Status::batteries`]. Matches the `#[rmk_peripheral(id = 0)]` of
/// `crate::peripheral_left`, which is what `PeripheralBatteryEvent` reports as its `id`.
pub const HALF_LEFT: usize = 0;

/// Index of the right half in [`Status::batteries`]. Matches `#[rmk_peripheral(id = 1)]`.
pub const HALF_RIGHT: usize = 1;

/// Names of the layers, indexed by layer number.
///
/// RMK resolves the `[[layer]] name` entries of `keyboard.toml` at build time and keeps no name
/// table in the firmware, and Vial has no layer names either, so a name cannot be read back at
/// run time. This table is therefore a second copy of the names in `keyboard.toml` and has to be
/// kept in sync by hand; a mismatch is not detected anywhere. Layers with no entry here are shown
/// as `Layer <number>`, which is what all layers past `base` currently get.
const LAYER_NAMES: [Option<&str>; 1] = [Some("base")];

/// Background.
const BACKGROUND: Rgb565 = Rgb565::new(0, 0, 0);

/// Color of the layer, of the USB label and of a held modifier.
const FOREGROUND: Rgb565 = Rgb565::new(31, 63, 31);

/// Vertical center (px) of the layer.
const LAYER_CENTER_Y: i32 = 118;

/// Font of the layer, the largest element of the screen. 42px tall, 35px per character, so a name
/// of more than seven characters runs past the margins.
static LAYER_FONT: FontRenderer = FontRenderer::new::<u8g2_font_inb42_mr>();

/// Number of frames the brightness overlay takes to come in from the right edge, and as many again
/// to leave.
///
/// Only the number of frames is fixed here; how long one of them lasts is set by whoever steps
/// [`BrightnessOverlay::slide`], which is [`crate::lcd`]. A frame that carries the overlay is the
/// most expensive redraw the screen has: the band of the overlay is the tallest of the four and
/// covers the layer, so such a frame clears those rows, draws the layer and the overlay into them
/// and sends them. Six of them come to roughly a quarter of a second, which is what this number is
/// chosen for.
///
/// The number of frames therefore buys duration as much as smoothness.
/// That slide-in falls inside the two seconds of [`crate::lcd`] (`BRIGHTNESS_HOLD`), which are
/// counted from the last brightness change, so the overlay stands fully in view for about 1.7s
/// before it leaves.
pub const BRIGHTNESS_SLIDE_FRAMES: u8 = 6;

/// The brightness overlay, and how far in it currently is.
///
/// Kept out of the rest of [`Status`] as an `Option` rather than as a flag beside the level,
/// because the overlay is a thing the screen either draws or does not: while it is not shown, no
/// brightness at all takes part in what the panel displays.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BrightnessOverlay {
    /// How far up the backlight steps the brightness is, from 0 for off to 100 for the brightest.
    /// This is a position in the steps and not the duty cycle, which grows geometrically.
    pub percent: u8,
    /// How far the overlay has come in, from 1 for barely on screen up to
    /// [`BRIGHTNESS_SLIDE_FRAMES`] for fully in. Zero is not a state of the overlay: it is the
    /// overlay not being there, which [`Status::brightness`] spells as `None`.
    pub slide: u8,
}

/// Everything the screen shows.
///
/// This is the whole interface between the processor that tracks the state and the screen that
/// draws it, so that the screen depends on neither the panel nor the event plumbing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// Host connection of the dongle, both transports included.
    pub connection: ConnectionStatus,
    /// Active layer.
    pub layer: u8,
    /// Modifiers currently held.
    pub modifiers: ModifierCombination,
    /// Battery of each half, indexed by [`HALF_LEFT`] and [`HALF_RIGHT`].
    pub batteries: [BatteryStatus; HALF_COUNT],
    /// Brightness overlay, `None` while it is not on screen.
    pub brightness: Option<BrightnessOverlay>,
}

impl Status {
    /// The state before any event has been received: nothing connected, layer 0, no modifier held,
    /// both batteries unknown and no brightness overlay.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            connection: ConnectionStatus::new(),
            layer: 0,
            modifiers: ModifierCombination::from_bits(0),
            batteries: [BatteryStatus::Unavailable; HALF_COUNT],
            brightness: None,
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}

/// A part of the screen that is cleared and redrawn on its own.
///
/// The two orders the parts are taken in are [`TOP_DOWN`] and [`DRAW_ORDER`], and the discriminants
/// are neither of them: they are what [`Parts`] indexes a part by and nothing else.
#[derive(Clone, Copy)]
enum Part {
    /// The transport and the held modifiers along the top.
    Header,
    /// The brightness overlay, which covers the middle while the backlight is being adjusted.
    Overlay,
    /// The layer across the middle.
    Layer,
    /// The battery of both halves along the bottom.
    Battery,
}

/// Number of parts the screen is drawn in.
const PART_COUNT: usize = 4;

/// The parts, ordered from the top of the screen down.
///
/// This is the order rows are merged in, which [`DirtyRows::push`] requires, and the order the
/// bands are cleared in.
const TOP_DOWN: [Part; PART_COUNT] = [Part::Header, Part::Overlay, Part::Layer, Part::Battery];

/// The parts, in the order they are drawn.
///
/// The brightness overlay comes last on purpose: it is meant to cover the layer while the backlight
/// is being adjusted, so it has to be drawn over it rather than under it.
const DRAW_ORDER: [Part; PART_COUNT] = [Part::Header, Part::Layer, Part::Battery, Part::Overlay];

impl Part {
    /// Rows the part covers.
    fn band(self) -> Band {
        match self {
            Self::Header => header::band(),
            Self::Overlay => brightness::band(),
            Self::Layer => layer_band(),
            Self::Battery => battery::band(),
        }
    }
}

/// A set of [`Part`]s.
///
/// A set rather than one flag per part, so that a part is named wherever it is read or written and
/// never stands for a position in a list.
#[derive(Clone, Copy)]
struct Parts([bool; PART_COUNT]);

impl Parts {
    /// The empty set.
    const NONE: Self = Self([false; PART_COUNT]);

    /// Every part.
    const ALL: Self = Self([true; PART_COUNT]);

    /// Puts `part` in the set when `member`, and takes it out otherwise.
    fn set(&mut self, part: Part, member: bool) {
        self.0[part as usize] = member;
    }

    /// Whether `part` is in the set.
    fn contains(self, part: Part) -> bool {
        self.0[part as usize]
    }
}

/// What one redraw of the screen consists of.
///
/// Produced by [`plan`] and consumed by [`draw`], as many times as [`crate::lcd`] needs to cover
/// [`rows`](Self::rows) with the part of the screen it holds.
pub struct Redraw {
    /// Whether the whole screen is cleared instead of the parts that are redrawn, which is what the
    /// first frame does.
    full: bool,
    /// The parts that are redrawn.
    parts: Parts,
    /// The rows the redraw changes.
    rows: DirtyRows,
}

impl Redraw {
    /// The rows the redraw changes, as disjoint bands ordered from the top of the screen down.
    #[must_use]
    pub fn rows(&self) -> &[Band] {
        self.rows.bands()
    }
}

/// Works out what `status` changes against `shown` on a screen of `size`.
///
/// `shown` is what the panel already displays, and `None` when it displays nothing yet. That first
/// frame covers the whole screen rather than the union of the bands: the rows between and around
/// the bands belong to none of them, and the panel holds noise after reset until something is
/// written over them. Clearing and sending the whole screen settles those rows for good, and every
/// frame after it can stay inside the bands it changed.
#[must_use]
pub fn plan(size: Size, status: &Status, shown: Option<Status>) -> Redraw {
    let mut rows = DirtyRows::new();

    let Some(shown) = shown else {
        rows.push(Band {
            top: 0,
            height: to_i32(size.height),
        });
        return Redraw {
            full: true,
            parts: Parts::ALL,
            rows,
        };
    };

    let header = status.connection != shown.connection || status.modifiers != shown.modifiers;
    let mut layer = status.layer != shown.layer;
    let battery = status.batteries != shown.batteries;
    let mut overlay = status.brightness != shown.brightness;

    // The overlay covers the layer, and a band is cleared across the full width, so clearing
    // either of the two wipes whatever the other drew: while the overlay is on screen, a change to
    // one of them redraws both. Only while it is on screen, so that a layer change on its own does
    // not pay for the rows the overlay would occupy.
    if (status.brightness.is_some() || shown.brightness.is_some())
        && brightness::band().intersects(layer_band())
    {
        let both = layer || overlay;
        layer = both;
        overlay = both;
    }

    let mut parts = Parts::NONE;
    parts.set(Part::Header, header);
    parts.set(Part::Overlay, overlay);
    parts.set(Part::Layer, layer);
    parts.set(Part::Battery, battery);

    for part in TOP_DOWN {
        if parts.contains(part) {
            rows.push(part.band());
        }
    }

    Redraw {
        full: false,
        parts,
        rows,
    }
}

/// Draws `redraw` onto `target`, which shows `status`.
///
/// Whatever falls outside the part of the screen `target` holds is dropped by the target itself, so
/// this draws the same thing whichever part that is and may be called once per part.
pub fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, status: &Status, redraw: &Redraw) {
    let width = to_i32(target.bounding_box().size.width);

    // Every band is cleared before anything is drawn: a clear reaches across the full width, so
    // clearing one band after having drawn another would erase it.
    if redraw.full {
        let _cleared = target.clear(BACKGROUND);
    } else {
        for part in TOP_DOWN {
            if redraw.parts.contains(part) {
                clear_band(target, width, part.band());
            }
        }
    }

    for part in DRAW_ORDER {
        if !redraw.parts.contains(part) {
            continue;
        }
        match part {
            Part::Header => header::draw(target, width, status),
            Part::Layer => draw_layer(target, width, status.layer),
            Part::Battery => battery::draw(target, width, status),
            Part::Overlay => draw_overlay(target, width, status),
        }
    }
}

/// Clears one band across the full width of a screen `width` px wide.
fn clear_band<D: DrawTarget<Color = Rgb565>>(target: &mut D, width: i32, band: Band) {
    let area = Rectangle::new(
        Point::new(0, band.top),
        Size::new(to_u32(width), to_u32(band.height)),
    );
    let _filled = target.fill_solid(&area, BACKGROUND);
}

/// Draws the brightness overlay when there is one to draw.
fn draw_overlay<D: DrawTarget<Color = Rgb565>>(target: &mut D, width: i32, status: &Status) {
    if let Some(overlay) = status.brightness {
        brightness::draw(target, width, overlay);
    }
}

/// Rows the layer covers. See [`draw_layer`].
fn layer_band() -> Band {
    font_band(&LAYER_FONT, LAYER_CENTER_Y)
}

/// Rows any glyph of `font` can reach when it is drawn centered on `center_y`.
///
/// `get_font_bounding_box` is the widest box any glyph of the font can occupy when it is rendered
/// at the origin with the given vertical position (`u8g2-fonts-0.8.0/src/renderer.rs:365-381`), so
/// the band does not depend on which text is currently shown.
fn font_band(font: &FontRenderer, center_y: i32) -> Band {
    let bounds = font.get_font_bounding_box(VerticalPosition::Center);
    Band {
        top: center_y + bounds.top_left.y,
        height: to_i32(bounds.size.height),
    }
}

/// Draws the layer across the middle of the screen.
fn draw_layer<D: DrawTarget<Color = Rgb565>>(target: &mut D, width: i32, layer: u8) {
    let center = Point::new(width / 2, LAYER_CENTER_Y);
    match LAYER_NAMES.get(usize::from(layer)).copied().flatten() {
        Some(name) => centered_text(&LAYER_FONT, target, name, center, FOREGROUND),
        None => centered_text(
            &LAYER_FONT,
            target,
            format_args!("Layer {layer}"),
            center,
            FOREGROUND,
        ),
    }
}

/// Draws `content` centered on `position` in both directions.
fn centered_text<D: DrawTarget<Color = Rgb565>>(
    font: &FontRenderer,
    target: &mut D,
    content: impl Content,
    position: Point,
    color: Rgb565,
) {
    let _rendered = font.render_aligned(
        content,
        position,
        VerticalPosition::Center,
        HorizontalAlignment::Center,
        FontColor::Transparent(color),
        target,
    );
}

/// `value` as an `i32`, saturating instead of wrapping.
fn to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// `value` as a `u32`, with negative values becoming zero.
fn to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}
