//! The battery band along the bottom of the status screen.
//!
//! One bar per half, each with its level printed above it. Everything the band needs of its own —
//! its geometry, its thresholds and its colors — lives here; what it shares with the rest of the
//! screen it takes from the parent module. The rows the band occupies follow from that geometry,
//! so [`band`] derives them here rather than the parent stating them a second time.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle};
use rmk::types::battery::BatteryStatus;
use u8g2_fonts::FontRenderer;
use u8g2_fonts::fonts::u8g2_font_inr16_mr;

use super::{Band, HALF_LEFT, HALF_RIGHT, Status, centered_text, font_band, to_u32};

/// Text shown in place of the battery level while it is unknown.
const UNKNOWN_LABEL: &str = "--%";

/// Vertical center (px) of the bars.
const CENTER_Y: i32 = 214;

/// Height (px) of a bar. Also fixes the radius of its rounded ends, see [`draw_battery`].
const BAR_HEIGHT: i32 = 6;

/// Topmost row (px) of a bar, which [`CENTER_Y`] is the middle of.
const BAR_TOP: i32 = CENTER_Y - BAR_HEIGHT / 2;

/// Distance (px) the bars keep from the left and right edges of the panel.
///
/// The band is inset further than the rest of the screen, which uses `MARGIN`. The value is
/// provisional: the hardware only said "a bit more than the other rows", so it is a single
/// constant to turn once it has been looked at again.
const SIDE_MARGIN: i32 = 20;

/// Gap (px) between the two bars.
///
/// The width of a bar is derived from this rather than being a constant of its own, see [`draw`].
const CENTER_GAP: i32 = 8;

/// Gap (px) between the top of a bar and the level printed above it.
const TEXT_GAP: i32 = 8;

/// Vertical center (px) of the level printed above a bar.
const TEXT_CENTER_Y: i32 = BAR_TOP - TEXT_GAP;

/// Lowest level (%) still drawn in the high color.
///
/// The boundary between the middle and the low color is the low battery threshold the indicator
/// LEDs use (`LOW_BATTERY_PERCENT` in [`crate::indicator`]), so both start warning at once.
const HIGH_PERCENT: u8 = 35;

/// Lowest level (%) still drawn in the middle color.
const MIDDLE_PERCENT: u8 = 20;

/// Color of a bar at or above [`HIGH_PERCENT`].
const HIGH_COLOR: Rgb565 = Rgb565::new(0, 63, 0);

/// Color of a bar at or above [`MIDDLE_PERCENT`].
const MIDDLE_COLOR: Rgb565 = Rgb565::new(31, 40, 0);

/// Color of a bar below [`MIDDLE_PERCENT`].
const LOW_COLOR: Rgb565 = Rgb565::new(31, 0, 0);

/// Color of a bar whose level is unknown, which is the case right after power-on and while a half
/// is not connected.
const UNKNOWN_COLOR: Rgb565 = Rgb565::new(8, 16, 8);

/// Numerator of the fraction the bar color is dimmed by for the part of the bar that is not
/// filled, so that the full length of the bar stays visible.
const TRACK_DIM_NUM: u32 = 1;

/// Denominator of [`TRACK_DIM_NUM`].
const TRACK_DIM_DEN: u32 = 4;

/// Font of the levels. Regular rather than bold, 16px tall, 14px per character.
static LEVEL_FONT: FontRenderer = FontRenderer::new::<u8g2_font_inr16_mr>();

/// Rows the battery band covers.
///
/// The bars reach from [`BAR_TOP`] down over [`BAR_HEIGHT`], and above them stands the level, whose
/// tallest possible glyph is what fixes the top of the band.
pub(super) fn band() -> Band {
    font_band(&LEVEL_FONT, TEXT_CENTER_Y).union(Band {
        top: BAR_TOP,
        height: BAR_HEIGHT,
    })
}

/// Draws the battery of both halves along the bottom of a screen `width` px wide.
///
/// The width of one bar is derived from the panel rather than being a constant, so that changing
/// [`SIDE_MARGIN`] or [`CENTER_GAP`] cannot break the symmetry of the two halves: each bar takes
/// half of what is left once both margins and the gap between them are subtracted, the left one
/// starts at [`SIDE_MARGIN`] and the right one ends the same distance from the right edge.
pub(super) fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, width: i32, status: &Status) {
    let bar_width = (width - 2 * SIDE_MARGIN - CENTER_GAP) / 2;

    draw_battery(
        target,
        SIDE_MARGIN,
        bar_width,
        level_of(status.batteries[HALF_LEFT]),
    );
    draw_battery(
        target,
        width - SIDE_MARGIN - bar_width,
        bar_width,
        level_of(status.batteries[HALF_RIGHT]),
    );
}

/// Draws one half as a bar of `bar_width` starting at `x`, with its level printed above it.
///
/// The bar is a stadium: the corner radius is half its height, so both ends are semicircles. The
/// filled part is drawn with the same radius rather than as a plain rectangle, which is what keeps
/// it inside the track: both start at the same left edge and so share the same left cap, and the
/// filled part is never longer, so its right cap always falls on ground the track already covers.
/// A plain rectangle would instead stick out of the rounded left end by the radius.
///
/// A filled part shorter than the bar is tall would still not work on its own. `RoundedRectangle`
/// scales radii down to fit the box it is given
/// (`embedded-graphics-0.8.1/src/primitives/rounded_rectangle/corner_radii.rs:60-100`), so a one
/// or two pixel wide fill comes out square cornered and pokes one pixel past the rounded cap of
/// the track. It is therefore drawn at its full cap length and clipped to the length the level
/// asks for: the visible shape is then a slice of the same cap the track has, and the level is
/// still shown to the pixel rather than rounded up to a whole circle.
fn draw_battery<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    x: i32,
    bar_width: i32,
    level: Option<u8>,
) {
    let color = level_color(level);
    let bar = Point::new(x, BAR_TOP);
    let height = to_u32(BAR_HEIGHT);

    // The whole bar is drawn first, so that its full length stays readable and the filled part is
    // a fraction of something visible rather than a floating rectangle. It is dimmed only when
    // there is a level to fill it with; an unknown level leaves one plain gray bar.
    let track = match level {
        Some(_) => dimmed(color, TRACK_DIM_NUM, TRACK_DIM_DEN),
        None => color,
    };
    fill_bar(
        target,
        Rectangle::new(bar, Size::new(to_u32(bar_width), height)),
        track,
    );
    if let Some(level) = level {
        // Rounded rather than truncated, so a bar is never one pixel shorter than the number above
        // it says.
        let filled = (bar_width * i32::from(level.min(100)) + 50) / 100;
        let visible = Rectangle::new(bar, Size::new(to_u32(filled), height));
        let drawn = Rectangle::new(bar, Size::new(to_u32(filled.max(BAR_HEIGHT)), height));
        fill_bar(&mut target.clipped(&visible), drawn, color);
    }

    let center = Point::new(x + bar_width / 2, TEXT_CENTER_Y);
    match level {
        Some(level) => centered_text(&LEVEL_FONT, target, format_args!("{level}%"), center, color),
        None => centered_text(&LEVEL_FONT, target, UNKNOWN_LABEL, center, color),
    }
}

/// Fills `area` as a stadium: a rectangle whose corner radius is half its height, which turns both
/// ends into semicircles.
///
/// `RoundedRectangle` confines radii that do not fit the box it is given, so an area shorter than
/// its own height comes out as a smaller stadium and finally as a circle instead of overflowing.
fn fill_bar<D: DrawTarget<Color = Rgb565>>(target: &mut D, area: Rectangle, color: Rgb565) {
    let radius = area.size.height / 2;
    let _drawn = RoundedRectangle::with_equal_corners(area, Size::new(radius, radius))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target);
}

/// The level a `BatteryStatus` carries, if it carries one.
fn level_of(battery: BatteryStatus) -> Option<u8> {
    match battery {
        BatteryStatus::Available { level, .. } => level,
        BatteryStatus::Unavailable => None,
    }
}

/// Color a bar is drawn in for `level`.
fn level_color(level: Option<u8>) -> Rgb565 {
    match level {
        Some(level) if level >= HIGH_PERCENT => HIGH_COLOR,
        Some(level) if level >= MIDDLE_PERCENT => MIDDLE_COLOR,
        Some(_) => LOW_COLOR,
        None => UNKNOWN_COLOR,
    }
}

/// `color` with every component scaled by `num / den`.
fn dimmed(color: Rgb565, num: u32, den: u32) -> Rgb565 {
    let scale = |component: u8| u8::try_from(u32::from(component) * num / den.max(1)).unwrap_or(0);
    Rgb565::new(scale(color.r()), scale(color.g()), scale(color.b()))
}
