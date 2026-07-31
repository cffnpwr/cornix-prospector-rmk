//! Bring-up screen drawn on the LCD.
//!
//! This is a provisional screen whose only job is to prove that the panel lights up and shows the
//! right pixels in the right place. It is not the display the keyboard is meant to have; a later
//! task replaces the whole module with the real status screen.
//!
//! It draws two things. The four corner patches carry four different colors, so that a rotation
//! that is off by 90 or 180 degrees puts the wrong color in the wrong corner, and a swapped
//! subpixel order (RGB against BGR) turns red into blue. Either mistake is then visible at a
//! glance instead of blending into a plausible looking picture, the same reasoning as the
//! power-on self test of [`crate::indicator`].
//!
//! The layer number is drawn as a seven segment shape rather than as text. The largest built-in
//! ASCII font of `embedded-graphics` is `FONT_10X20` and the crate cannot scale glyphs up, which
//! is far too small to read across a 280x240 panel, so this is the way to get a large digit
//! without pulling in a new font dependency. It only pays off here: a seven segment shape can
//! draw digits and nothing else, so the real screen — which has to spell out layer names and
//! labels such as `USB` or `BT0` — cannot use it and replaces it together with a proper font.
//!
//! Nothing here knows about the panel: [`draw`] takes any `Rgb565` draw target and reads its size
//! from the target itself, so what is drawn can be replaced without touching [`crate::lcd`].

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

/// Background.
const BACKGROUND: Rgb565 = Rgb565::BLACK;

/// Color of the layer digit.
const DIGIT_COLOR: Rgb565 = Rgb565::WHITE;

/// Edge length (px) of the squares drawn in the corners.
const CORNER_SIZE: u32 = 40;

/// Colors of the corner squares, in the order top left, top right, bottom left, bottom right.
///
/// Four different colors, so a rotation that is off by 90 or 180 degrees and a swapped color order
/// are both visible at a glance.
const CORNER_COLORS: [Rgb565; 4] = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::YELLOW];

/// Width (px) of the layer digit.
const DIGIT_WIDTH: i32 = 80;

/// Height (px) of the layer digit.
const DIGIT_HEIGHT: i32 = 140;

/// Stroke width (px) of one segment of the layer digit.
const DIGIT_THICKNESS: i32 = 16;

/// Segments of a seven segment digit as `(x, y, width, height)` relative to the top left of the
/// digit, in the order a, b, c, d, e, f, g.
///
/// The names follow the usual seven segment labelling: `a` is the top bar, `b` and `c` the right
/// arms from top to bottom, `d` the bottom bar, `e` and `f` the left arms from bottom to top, and
/// `g` the middle bar.
const DIGIT_SEGMENTS: [(i32, i32, i32, i32); 7] = {
    let half = DIGIT_HEIGHT / 2;
    let arm = half - DIGIT_THICKNESS;
    let span = DIGIT_WIDTH - 2 * DIGIT_THICKNESS;
    [
        (DIGIT_THICKNESS, 0, span, DIGIT_THICKNESS),
        (
            DIGIT_WIDTH - DIGIT_THICKNESS,
            DIGIT_THICKNESS,
            DIGIT_THICKNESS,
            arm,
        ),
        (DIGIT_WIDTH - DIGIT_THICKNESS, half, DIGIT_THICKNESS, arm),
        (
            DIGIT_THICKNESS,
            DIGIT_HEIGHT - DIGIT_THICKNESS,
            span,
            DIGIT_THICKNESS,
        ),
        (0, half, DIGIT_THICKNESS, arm),
        (0, DIGIT_THICKNESS, DIGIT_THICKNESS, arm),
        (
            DIGIT_THICKNESS,
            half - DIGIT_THICKNESS / 2,
            span,
            DIGIT_THICKNESS,
        ),
    ]
};

/// Which segments of [`DIGIT_SEGMENTS`] are lit per decimal digit.
///
/// Each row is `[a, b, c, d, e, f, g]`.
const DIGIT_PATTERNS: [[bool; 7]; 10] = [
    // 0: a b c d e f
    [true, true, true, true, true, true, false],
    // 1: b c
    [false, true, true, false, false, false, false],
    // 2: a b d e g
    [true, true, false, true, true, false, true],
    // 3: a b c d g
    [true, true, true, true, false, false, true],
    // 4: b c f g
    [false, true, true, false, false, true, true],
    // 5: a c d f g
    [true, false, true, true, false, true, true],
    // 6: a c d e f g
    [true, false, true, true, true, true, true],
    // 7: a b c
    [true, true, true, false, false, false, false],
    // 8: a b c d e f g
    [true, true, true, true, true, true, true],
    // 9: a b c d f g
    [true, true, true, true, false, true, true],
];

/// Draws the screen for `layer` onto `target`.
///
/// Layers past 9 wrap around, because only one digit is drawn.
pub fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, layer: u8) {
    let _cleared = target.clear(BACKGROUND);

    let size = target.bounding_box().size;
    let to_i32 = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    let far_x = to_i32(size.width.saturating_sub(CORNER_SIZE));
    let far_y = to_i32(size.height.saturating_sub(CORNER_SIZE));
    let corners = [(0, 0), (far_x, 0), (0, far_y), (far_x, far_y)];
    for ((x, y), color) in corners.into_iter().zip(CORNER_COLORS) {
        fill(target, x, y, CORNER_SIZE, CORNER_SIZE, color);
    }

    let origin_x = (to_i32(size.width) - DIGIT_WIDTH) / 2;
    let origin_y = (to_i32(size.height) - DIGIT_HEIGHT) / 2;
    let Some(pattern) = DIGIT_PATTERNS.get(usize::from(layer) % DIGIT_PATTERNS.len()) else {
        return;
    };
    for (&lit, &(x, y, width, height)) in pattern.iter().zip(DIGIT_SEGMENTS.iter()) {
        if lit {
            fill(
                target,
                origin_x + x,
                origin_y + y,
                u32::try_from(width).unwrap_or(0),
                u32::try_from(height).unwrap_or(0),
                DIGIT_COLOR,
            );
        }
    }
}

/// Fills one rectangle of the target.
fn fill<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color: Rgb565,
) {
    let area = Rectangle::new(Point::new(x, y), Size::new(width, height));
    let _drawn = area
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(target);
}
