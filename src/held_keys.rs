//! Processor that releases the keys a half was still holding when its link drops.
//!
//! A press and the release that follows it reach the dongle as two separate split messages, which
//! `PeripheralManager::process_peripheral_message` republishes as they arrive
//! (`rmk/src/split/driver.rs:180-198`). Nothing ties the two together: when the link drops in
//! between, the manager task is torn down with the connection
//! (`rmk/src/split/ble/central.rs:238-244, 296-306`) and the press stays in the HID report as if
//! the key were still down, until the half comes back and the key is pressed and released again.
//!
//! The half cannot make up for it from its side. Its split task drops the [`KeyboardEvent`]
//! subscriber it reads from when it notices the disconnect
//! (`rmk/src/split/peripheral.rs:92, 234-236`), so a release pressed out while the link is down is
//! discarded there and never sent. The dongle is therefore where the keys have to be let go of.
//!
//! [`KeyboardEvent`] is what the keyboard itself works from
//! (`rmk/src/keyboard.rs:158-168, 264`), so a release published here is indistinguishable from one
//! that came off a matrix. This processor follows which positions are down and publishes a release
//! for those of the half that just went away.
//!
//! Only the keys seen pressed are released, rather than every key of the half: a release carries
//! meaning of its own, and `Action::LayerToggle` for one toggles its layer on the release rather
//! than on the press (`rmk/src/keyboard.rs:1249-1254`). The current keymap has no such action, but
//! one added to `keyboard.toml` would be triggered by a blanket release.
//!
//! What this cannot shorten is how long the key stays down. The drop is noticed no earlier than
//! the link supervision timeout of 10s (`rmk/src/split/ble/central.rs:269`), and
//! `PeripheralConnectedEvent { connected: false }` follows another 500ms later
//! (`rmk/src/split/ble/central.rs:209-212, 260`). Neither value can be set from `keyboard.toml`.
//! A key therefore stays down for upwards of ten seconds before it is released here.

use rmk::event::{KeyboardEvent, KeyboardEventPos, PeripheralConnectedEvent, publish_event};
use rmk::macros::processor;

/// Rows of the keymap. Hand copy of `rows` under `[layout]` in `keyboard.toml`.
const ROWS: usize = 8;

/// Columns of the keymap. Hand copy of `cols` under `[layout]` in `keyboard.toml`. A row of
/// [`HeldKeysProcessor::held`] is one `u8`, so this cannot exceed 8.
const COLS: u8 = 7;

/// First row of each half and the row one past its last, indexed by the id given to
/// `#[rmk_peripheral]`.
///
/// Hand copy of `row_offset` and `rows` of the `[[split.peripheral]]` entries in `keyboard.toml`,
/// which is what the dongle adds to the row of every key it receives from that half
/// (`rmk/src/split/driver.rs:190-195`). Like the layer names of [`crate::status_screen`] this is a
/// second copy of what `keyboard.toml` holds and has to be kept in sync by hand; a mismatch is
/// detected nowhere and would leave the keys of one half held while releasing keys of the other.
const HALF_ROWS: [(u8, u8); 2] = [(0, 4), (4, 8)];

/// Processor that holds which keys are down and releases those of a half that disconnects.
///
/// [`PeripheralConnectedEvent`] is subscribed to ahead of [`KeyboardEvent`] because the generated
/// subscriber takes the events in the order they are listed here, so a disconnect is never left
/// waiting behind a key that is being typed on the other half.
#[processor(subscribe = [PeripheralConnectedEvent, KeyboardEvent])]
pub struct HeldKeysProcessor {
    /// One row of the matrix per entry, one column per bit, set while that key is down.
    held: [u8; ROWS],
}

impl Default for HeldKeysProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl HeldKeysProcessor {
    /// Builds the processor with no key held.
    #[must_use]
    pub const fn new() -> Self {
        Self { held: [0; ROWS] }
    }

    /// Follows a key going down or coming up.
    ///
    /// This also sees the releases published by
    /// [`on_peripheral_connected_event`](Self::on_peripheral_connected_event), which clear bits
    /// that are already clear. Positions that are not a key of the matrix, which is what a rotary
    /// encoder produces, are left alone: they carry an encoder id rather than a row and cannot be
    /// assigned to a half here.
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_keyboard_event(&mut self, event: KeyboardEvent) {
        let KeyboardEventPos::Key(pos) = event.pos else {
            return;
        };
        let Some(held) = self.held.get_mut(usize::from(pos.row)) else {
            return;
        };
        if pos.col >= COLS {
            return;
        }

        let mask = 1 << pos.col;
        if event.pressed {
            *held |= mask;
        } else {
            *held &= !mask;
        }
    }

    /// Releases the keys of a half that has disconnected.
    ///
    /// `connected: false` is published once per attempt to reach the half
    /// (`rmk/src/split/ble/central.rs:209-212`), so this runs again every 15s or so while a half
    /// stays away and at boot before either half has been reached. Every run past the first finds
    /// nothing held and publishes nothing.
    ///
    /// The releases go out through `publish_event`, which drops the oldest event of the channel
    /// once it is full (`embassy-sync-0.7.2/src/pubsub/mod.rs:352-360`) rather than waiting for
    /// the keyboard to catch up. The channel holds 16 events
    /// (`rmk-config/src/default_config/event_default.toml:16-19`) and nothing here yields in
    /// between, so a half that had more keys than that down at once keeps some of them held.
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if event.connected {
            return;
        }
        let Some(&(first_row, end_row)) = HALF_ROWS.get(event.id) else {
            return;
        };

        for row in first_row..end_row {
            let Some(held) = self.held.get_mut(usize::from(row)) else {
                continue;
            };
            let down = *held;
            *held = 0;

            for col in 0..COLS {
                if down & (1 << col) != 0 {
                    publish_event(KeyboardEvent::key(row, col, false));
                }
            }
        }
    }
}
