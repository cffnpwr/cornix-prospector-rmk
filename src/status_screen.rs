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
//! This module holds what the screen shares — the state it is given, the colors, the fonts and the
//! two upper bands — while the parts that stand on their own live beside it: the icon bitmaps in
//! [`icons`], the battery band in [`battery`] and the brightness overlay in [`brightness`]. Every
//! coordinate, color, font and threshold is a constant of whichever of the four owns it. The values
//! come from `.tmp/screen.json`, which is what the layout was settled against in a browser preview.

mod battery;
mod brightness;
mod icons;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use rmk::types::battery::BatteryStatus;
use rmk::types::connection::{ConnectionStatus, ConnectionType};
use rmk::types::modifier::ModifierCombination;
use u8g2_fonts::fonts::{u8g2_font_inb21_mr, u8g2_font_inb42_mr};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};
use u8g2_fonts::{Content, FontRenderer};

use crate::status_screen::icons::{BT_ICON, Bitmap, MODIFIER_ICONS, USB_ICON, draw_bitmap};

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

/// Label shown next to the USB icon.
const USB_LABEL: &str = "USB";

/// Label shown when neither transport carries reports.
const NO_CONNECTION_LABEL: &str = "---";

/// Background.
const BACKGROUND: Rgb565 = Rgb565::new(0, 0, 0);

/// Color of the layer, of the USB label and of a held modifier.
const FOREGROUND: Rgb565 = Rgb565::new(31, 63, 31);

/// Color of a modifier that is not held, and of the indication that nothing is connected.
const INACTIVE: Rgb565 = Rgb565::new(8, 16, 8);

/// Color of the USB icon.
///
/// Fixed, unlike the label next to it: the icon says which transport carries the reports and the
/// label says which BLE profile, so the two do not compete for the same meaning.
const USB_ICON_COLOR: Rgb565 = Rgb565::new(31, 32, 0);

/// Color of the Bluetooth icon. Fixed, see [`USB_ICON_COLOR`].
const BLUETOOTH_ICON_COLOR: Rgb565 = Rgb565::new(0, 32, 31);

/// Colors that correspond to the BLE profile number.
///
/// Same order as the indicator LEDs of the halves (`PROFILE_COLORS` in [`crate::indicator`]), so
/// that the profile has one color across the keyboard. Matches `ble_profiles_num = 5` in
/// `keyboard.toml`.
const PROFILE_COLORS: [Rgb565; 5] = [
    Rgb565::new(0, 63, 0),
    Rgb565::new(31, 0, 0),
    Rgb565::new(0, 0, 31),
    Rgb565::new(31, 63, 0),
    Rgb565::new(0, 63, 31),
];

/// Distance (px) kept from the left and right edges of the panel by the two upper bands. The
/// battery band is inset further and has its own constant.
const MARGIN: i32 = 12;

/// Vertical center (px) of the top row, which the transport label and the icons are centered on.
const HEADER_CENTER_Y: i32 = 32;

/// Shift (px) applied to the transport icon on top of centering it on [`HEADER_CENTER_Y`], so that
/// it lines up with the label beside it rather than with the row.
const CONN_ICON_OFFSET_Y: i32 = -3;

/// Gap (px) between the transport icon and its label.
const ICON_LABEL_GAP: i32 = 0;

/// Width (px) of the slot one modifier symbol is centered in.
const MODIFIER_SLOT_WIDTH: i32 = 26;

/// Gap (px) between two modifier slots.
const MODIFIER_SLOT_GAP: i32 = 4;

/// Shift (px) applied to the modifier icons on top of centering them on [`HEADER_CENTER_Y`].
const MODIFIER_ICON_OFFSET_Y: i32 = -2;

/// Vertical center (px) of the layer.
const LAYER_CENTER_Y: i32 = 118;

/// Font of the layer, the largest element of the screen. 42px tall, 35px per character, so a name
/// of more than seven characters runs past the margins.
static LAYER_FONT: FontRenderer = FontRenderer::new::<u8g2_font_inb42_mr>();

/// Font of the transport label, which is the only thing it is used for. 21px tall, 19px per
/// character.
static LABEL_FONT: FontRenderer = FontRenderer::new::<u8g2_font_inb21_mr>();

/// Number of frames the brightness overlay takes to come in from the right edge, and as many again
/// to leave.
///
/// Only the number of frames is fixed here; how long one of them lasts is set by whoever steps
/// [`BrightnessOverlay::slide`], which is [`crate::lcd`]. Measured on hardware, a frame that carries
/// the overlay takes about 63ms: roughly 29ms redrawing the whole screen and 34ms transferring it,
/// which every frame does because the panel has no partial update.
///
/// The number of frames therefore buys duration as much as smoothness.
/// That slide-in falls inside the two seconds of [`crate::lcd`] (`BRIGHTNESS_HOLD`), which are
/// counted from the last brightness change, so the overlay stands fully in view for about 1.7s
/// before it leaves.
pub const BRIGHTNESS_SLIDE_FRAMES: u8 = 4;

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

/// Draws the whole screen for `status` onto `target`.
///
/// The brightness overlay comes last on purpose: it is meant to cover the layer while the backlight
/// is being adjusted, so it has to be drawn over it rather than under it.
pub fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, status: &Status) {
    let _cleared = target.clear(BACKGROUND);

    let width = to_i32(target.bounding_box().size.width);

    draw_connection(target, status.connection);
    draw_modifiers(target, width, status.modifiers);
    draw_layer(target, width, status.layer);
    battery::draw_batteries(target, width, status);
    if let Some(overlay) = status.brightness {
        brightness::draw_brightness(target, width, overlay);
    }
}

/// Draws the transport that reports are routed to, in the top left corner.
///
/// Which transport that is comes from `ConnectionStatus::decide_active`, so the screen agrees with
/// where the reports actually go. That judgement is wider than the one the indicator LEDs make,
/// which deliberately ignore a suspended USB host.
///
/// The icon keeps a fixed color per transport while the label carries the profile color, so the
/// two together say both which transport and which profile is in use.
fn draw_connection<D: DrawTarget<Color = Rgb565>>(target: &mut D, connection: ConnectionStatus) {
    let (icon, icon_color, label_color) = match connection.decide_active() {
        Some(ConnectionType::Usb) => (Some(USB_ICON), USB_ICON_COLOR, FOREGROUND),
        Some(ConnectionType::Ble) => {
            let color = PROFILE_COLORS
                .get(usize::from(connection.ble.profile))
                .copied()
                .unwrap_or(FOREGROUND);
            (Some(BT_ICON), BLUETOOTH_ICON_COLOR, color)
        }
        None => (None, INACTIVE, INACTIVE),
    };

    let mut label_x = MARGIN;
    if let Some(icon) = icon {
        draw_bitmap(
            target,
            &icon,
            icon_origin(MARGIN, &icon, CONN_ICON_OFFSET_Y),
            icon_color,
        );
        label_x = MARGIN + to_i32(icon.width) + ICON_LABEL_GAP;
    }

    let position = Point::new(label_x, HEADER_CENTER_Y);
    match connection.decide_active() {
        Some(ConnectionType::Usb) => text(&LABEL_FONT, target, USB_LABEL, position, label_color),
        Some(ConnectionType::Ble) => text(
            &LABEL_FONT,
            target,
            format_args!("BT{}", connection.ble.profile),
            position,
            label_color,
        ),
        None => text(
            &LABEL_FONT,
            target,
            NO_CONNECTION_LABEL,
            position,
            label_color,
        ),
    }
}

/// Draws the four modifier symbols in the top right corner, lit for the ones being held.
///
/// The left and the right key of a modifier are merged, because the screen shows the modifier and
/// not the key.
fn draw_modifiers<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    width: i32,
    modifiers: ModifierCombination,
) {
    let held = [
        modifiers.left_ctrl() || modifiers.right_ctrl(),
        modifiers.left_alt() || modifiers.right_alt(),
        modifiers.left_shift() || modifiers.right_shift(),
        modifiers.left_gui() || modifiers.right_gui(),
    ];

    // Placed from the right edge inwards, so the row stays anchored to the corner whatever the
    // number of modifiers and the slot width are.
    let mut right = width - MARGIN;
    for (icon, lit) in MODIFIER_ICONS.into_iter().zip(held).rev() {
        let x = right - MODIFIER_SLOT_WIDTH / 2 - to_i32(icon.width) / 2;
        let color = if lit { FOREGROUND } else { INACTIVE };
        draw_bitmap(
            target,
            &icon,
            icon_origin(x, &icon, MODIFIER_ICON_OFFSET_Y),
            color,
        );
        right -= MODIFIER_SLOT_WIDTH + MODIFIER_SLOT_GAP;
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

/// Top left corner of `bitmap` when it is centered on [`HEADER_CENTER_Y`] and then shifted by
/// `offset`, with its left edge at `x`.
fn icon_origin(x: i32, bitmap: &Bitmap, offset: i32) -> Point {
    Point::new(x, HEADER_CENTER_Y - to_i32(bitmap.height) / 2 + offset)
}

/// Draws `content` with its left edge at `position` and vertically centered on it.
fn text<D: DrawTarget<Color = Rgb565>>(
    font: &FontRenderer,
    target: &mut D,
    content: impl Content,
    position: Point,
    color: Rgb565,
) {
    let _rendered = font.render(
        content,
        position,
        VerticalPosition::Center,
        FontColor::Transparent(color),
        target,
    );
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
