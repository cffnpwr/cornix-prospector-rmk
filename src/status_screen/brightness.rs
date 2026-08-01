//! The brightness overlay of the status screen.
//!
//! A vertical bar with a sun above it on a rounded backdrop, shown beside the layer while the
//! backlight is being adjusted and off screen the rest of the time. The backdrop is what makes it
//! readable over the layer it covers.
//!
//! Everything the overlay needs of its own — its geometry, its colors and the distance it travels —
//! lives here; the level it shows and how far in it is come from the parent module as a
//! [`BrightnessOverlay`], which is what [`crate::lcd`] steps.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle};

use super::icons::{SUN_ICON, draw_bitmap};
use super::{BRIGHTNESS_SLIDE_FRAMES, BrightnessOverlay, to_i32};

/// Horizontal center (px) of the overlay once it is fully in. To the right of the layer, which it
/// is allowed to cover.
const CENTER_X: i32 = 242;

/// Vertical center (px) of the overlay. The icon, the gap and the bar are centered on it as one
/// block, so the backdrop around them is centered on it too. Same line as the layer.
const CENTER_Y: i32 = 118;

/// Width (px) of the bar. Also fixes the radius of its rounded ends, see [`fill_bar`].
const BAR_WIDTH: i32 = 6;

/// Height (px) of the bar, which is the whole range of the backlight.
const BAR_HEIGHT: i32 = 96;

/// Gap (px) between the sun and the top of the bar.
const ICON_GAP: i32 = 6;

/// Corner radius (px) of the backdrop.
const RADIUS: u32 = 16;

/// Distance (px) the backdrop keeps around the sun and the bar.
const PADDING: i32 = 8;

/// Color of the filled part of the bar, and of the sun above it. The sun takes the same color
/// because the overlay says one thing and the layout it was settled against gives it no color of
/// its own.
const BAR_COLOR: Rgb565 = Rgb565::new(31, 63, 31);

/// Color of the whole length of the bar, so that the part above the level stays visible.
const TRACK_COLOR: Rgb565 = Rgb565::new(8, 16, 8);

/// Color of the backdrop. Bright enough to separate the overlay from the layer underneath and dark
/// enough not to compete with the bar on it.
const BACKDROP_COLOR: Rgb565 = Rgb565::new(3, 6, 3);

/// Draws the overlay onto a screen `width` px wide.
///
/// The bar is a stadium filled from the bottom up, and it is drawn the way the battery band draws
/// its own: the filled part shares the bottom cap of the track and is drawn at its full cap length
/// but clipped to the length the level asks for, because `RoundedRectangle` scales radii down to
/// fit the box it is given
/// (`embedded-graphics-0.8.1/src/primitives/rounded_rectangle/corner_radii.rs:60-100`) and a fill
/// shorter than the bar is wide would otherwise come out square cornered and poke past the rounded
/// cap of the track.
pub(super) fn draw_brightness<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    width: i32,
    overlay: BrightnessOverlay,
) {
    let icon_width = to_i32(SUN_ICON.width);
    let icon_height = to_i32(SUN_ICON.height);
    let block_width = icon_width.max(BAR_WIDTH);
    let block_height = icon_height + ICON_GAP + BAR_HEIGHT;
    let backdrop = Rectangle::new(
        Point::new(
            CENTER_X - block_width / 2 - PADDING,
            CENTER_Y - block_height / 2 - PADDING,
        ),
        Size::new(
            to_u32(block_width + 2 * PADDING),
            to_u32(block_height + 2 * PADDING),
        ),
    );
    // Everything is laid out where the overlay comes to rest and then moved as one, so that the
    // slide cannot pull the parts apart.
    let offset = Point::new(slide_offset(width, backdrop.top_left.x, overlay.slide), 0);

    let _drawn =
        RoundedRectangle::with_equal_corners(backdrop.translate(offset), Size::new(RADIUS, RADIUS))
            .into_styled(PrimitiveStyle::with_fill(BACKDROP_COLOR))
            .draw(target);

    let icon = Point::new(CENTER_X - icon_width / 2, backdrop.top_left.y + PADDING);
    draw_bitmap(target, &SUN_ICON, icon + offset, BAR_COLOR);

    let bar_x = CENTER_X - BAR_WIDTH / 2 + offset.x;
    let bar_bottom = icon.y + icon_height + ICON_GAP + BAR_HEIGHT;
    fill_bar(target, bar(bar_x, bar_bottom, BAR_HEIGHT), TRACK_COLOR);

    // Rounded rather than truncated, so that the two ends of the range are the only ones drawn as
    // an empty and as a full bar.
    let filled = (BAR_HEIGHT * i32::from(overlay.percent.min(100)) + 50) / 100;
    let visible = bar(bar_x, bar_bottom, filled);
    let drawn = bar(bar_x, bar_bottom, filled.max(BAR_WIDTH));
    fill_bar(&mut target.clipped(&visible), drawn, BAR_COLOR);
}

/// Distance (px) the overlay is pushed to the right at `slide`, given a screen `width` px wide and
/// a backdrop whose left edge is at `backdrop_x` once the overlay is fully in.
///
/// At slide 0 the backdrop starts where the panel ends, which is what puts the whole overlay off
/// the right edge, and the distance shrinks to nothing over [`BRIGHTNESS_SLIDE_FRAMES`] frames.
fn slide_offset(width: i32, backdrop_x: i32, slide: u8) -> i32 {
    let frames = i32::from(BRIGHTNESS_SLIDE_FRAMES).max(1);
    let left = (frames - i32::from(slide)).clamp(0, frames);
    let travel = (width - backdrop_x).max(0);
    travel * left / frames
}

/// A bar [`BAR_WIDTH`] px wide and `length` px tall whose bottom edge is at `bottom`, with its left
/// edge at `x`.
///
/// The bar grows from the bottom because that is the direction the level it shows grows in.
fn bar(x: i32, bottom: i32, length: i32) -> Rectangle {
    Rectangle::new(
        Point::new(x, bottom - length),
        Size::new(to_u32(BAR_WIDTH), to_u32(length)),
    )
}

/// Fills `area` as a stadium: a rectangle whose corner radius is half its width, which turns both
/// ends of an upright bar into semicircles.
///
/// `RoundedRectangle` confines radii that do not fit the box it is given, so an area shorter than
/// its own width comes out as a smaller stadium and finally as a circle instead of overflowing.
fn fill_bar<D: DrawTarget<Color = Rgb565>>(target: &mut D, area: Rectangle, color: Rgb565) {
    let radius = area.size.width / 2;
    let _drawn = RoundedRectangle::with_equal_corners(area, Size::new(radius, radius))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target);
}

/// `value` as a `u32`, with negative values becoming zero.
fn to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}
