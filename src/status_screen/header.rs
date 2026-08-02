//! The top row of the status screen.
//!
//! The transport that carries the reports on the left and the modifiers being held on the right,
//! both centered on one line. Everything the row needs of its own — its geometry, its font and its
//! colors — lives here; what it shares with the rest of the screen it takes from the parent module.
//! The rows the band occupies follow from that geometry, so [`band`] derives them here rather than
//! the parent stating them a second time.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use rmk::types::connection::{ConnectionStatus, ConnectionType};
use rmk::types::modifier::ModifierCombination;
use u8g2_fonts::fonts::u8g2_font_inb21_mr;
use u8g2_fonts::types::{FontColor, VerticalPosition};
use u8g2_fonts::{Content, FontRenderer};

use super::icons::{BT_ICON, Bitmap, MODIFIER_ICONS, USB_ICON, draw_bitmap};
use super::{Band, FOREGROUND, Status, font_band, to_i32};

/// Label shown next to the USB icon.
const USB_LABEL: &str = "USB";

/// Label shown when neither transport carries reports.
const NO_CONNECTION_LABEL: &str = "---";

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

/// Distance (px) the transport and the modifiers keep from the left and right edges of the panel.
/// The battery band is inset further and has its own constant.
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

/// Font of the transport label, which is the only thing it is used for. 21px tall, 19px per
/// character.
static LABEL_FONT: FontRenderer = FontRenderer::new::<u8g2_font_inb21_mr>();

/// Rows the top band covers.
///
/// The icons are as tall as their bitmaps and sit where [`icon_origin`] puts them; the label
/// reaches as far as the tallest glyph of [`LABEL_FONT`] can. Taking the whole font rather than the
/// glyphs of one label keeps the band the same whichever transport is shown, so that switching
/// transports cannot leave part of the label behind.
pub(super) fn band() -> Band {
    let conn_icons = [
        icon_band(&USB_ICON, CONN_ICON_OFFSET_Y),
        icon_band(&BT_ICON, CONN_ICON_OFFSET_Y),
    ];
    let modifier_icons = MODIFIER_ICONS.map(|icon| icon_band(&icon, MODIFIER_ICON_OFFSET_Y));

    let mut band = font_band(&LABEL_FONT, HEADER_CENTER_Y);
    for icon in conn_icons.into_iter().chain(modifier_icons) {
        band = band.union(icon);
    }
    band
}

/// Rows a header icon covers when it is drawn with `offset`. See [`icon_origin`].
fn icon_band(bitmap: &Bitmap, offset: i32) -> Band {
    Band {
        top: icon_origin(0, bitmap, offset).y,
        height: to_i32(bitmap.height),
    }
}

/// Draws the top band: the transport on the left and the held modifiers on the right.
pub(super) fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, width: i32, status: &Status) {
    draw_connection(target, status.connection);
    draw_modifiers(target, width, status.modifiers);
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
