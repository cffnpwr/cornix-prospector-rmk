//! Processor that re-publishes the latest [`BatteryStatusEvent`] of this half.
//!
//! Without it the central never receives a `PeripheralBatteryEvent`, because three RMK behaviors
//! combine:
//!
//! 1. The first ADC sample is taken immediately rather than after `polling_interval`
//!    (`rmk/src/input_device/adc/nrf.rs:233-236`), so it lands before the split link is up.
//! 2. A split send that fails is dropped and never retried
//!    (`rmk/src/split/peripheral.rs:239-241`), and a detected disconnect leaves `run()`
//!    (`rmk/src/split/peripheral.rs:232-237`), which drops the subscriber created at
//!    `rmk/src/split/peripheral.rs:97` along with everything published while the link was down.
//! 3. After the first sample `BatteryProcessor` publishes only when the percentage changed
//!    (`rmk/src/input_device/battery.rs:174-183`), and `get_battery_percent` saturates at 100 near
//!    a full charge (`rmk/src/input_device/battery.rs:149-151`), so the publish can happen exactly
//!    once in the lifetime of the firmware.
//!
//! The peripheral therefore sends its battery level once at boot, that send is discarded because
//! no central is connected yet, and nothing is sent again. Re-publishing the retained value gives
//! the split task something to forward once the link is up.
//!
//! The re-publish also reaches the other subscribers of `BatteryStatusEvent` on this half
//! (`BleBatteryServer`, [`IndicatorProcessor`](crate::indicator::IndicatorProcessor) and this
//! processor itself). Each of them receives the value it already holds, so nothing changes for
//! them.
//!
//! This works around RMK rather than implementing anything this keyboard needs. Delete the module
//! and its registrations once any of the three behaviors above is fixed upstream.

use rmk::event::{BatteryStatusEvent, CentralConnectedEvent, publish_event};
use rmk::macros::processor;

/// Poll interval (ms). Keep it equal to `poll_interval` of `#[processor]`.
const POLL_INTERVAL_MS: u32 = 1000;

/// Delay (ms) from observing the split link come up to the re-publish.
///
/// `CentralConnectedEvent { connected: true }` is published at
/// `rmk/src/split/ble/peripheral.rs:159`, while the `BatteryStatusEvent` subscriber that forwards
/// the value over the split link is created later, at `rmk/src/split/peripheral.rs:97`, reached
/// through `rmk/src/split/ble/peripheral.rs:174`. Publishing before that subscriber exists loses
/// the value, and the path in between contains an await (`write_peer_address` at
/// `rmk/src/split/ble/peripheral.rs:164`) at which this processor can run. Waiting is what makes
/// the re-publish land after the subscriber exists.
const LINK_UP_RESEND_DELAY_MS: u32 = 2000;

/// Interval (ms) of the unconditional re-publish.
///
/// The safety net for a value lost to the race above or to a failed send, so that the level
/// recovers no matter in which order the halves and the dongle are powered on. One split message
/// per interval is negligible.
const PERIODIC_RESEND_INTERVAL_MS: u32 = 30_000;

/// Number of polls to wait after the split link comes up.
const LINK_UP_RESEND_DELAY_TICKS: u32 = LINK_UP_RESEND_DELAY_MS / POLL_INTERVAL_MS;

/// Number of polls between two unconditional re-publishes.
const PERIODIC_RESEND_TICKS: u32 = PERIODIC_RESEND_INTERVAL_MS / POLL_INTERVAL_MS;

/// Processor that retains the battery status of this half and re-publishes it.
#[processor(subscribe = [CentralConnectedEvent, BatteryStatusEvent], poll_interval = 1000)]
pub struct BatteryResendProcessor {
    /// Latest battery status seen on this half. `None` until the first event arrives, and nothing
    /// is re-published while it is `None`.
    status: Option<BatteryStatusEvent>,
    /// Whether the split link to the central is established.
    central_connected: bool,
    /// Polls left before the re-publish scheduled by the link coming up. `0` when none is
    /// scheduled.
    link_up_delay: u32,
    /// Polls left before the next unconditional re-publish.
    periodic_delay: u32,
}

impl Default for BatteryResendProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryResendProcessor {
    /// Builds the processor with nothing retained yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status: None,
            central_connected: false,
            link_up_delay: 0,
            periodic_delay: PERIODIC_RESEND_TICKS,
        }
    }

    /// Takes in a change of the split link connection state and schedules a re-publish on the
    /// transition to connected.
    #[expect(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_central_connected_event(&mut self, event: CentralConnectedEvent) {
        if event.connected && !self.central_connected {
            self.link_up_delay = LINK_UP_RESEND_DELAY_TICKS;
        }
        self.central_connected = event.connected;
    }

    /// Retains the battery status.
    ///
    /// This also sees the events published by [`poll`](Self::poll), which carry the value already
    /// retained.
    #[expect(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        self.status = Some(event);
    }

    /// Advances both timers and re-publishes the retained status when either elapses.
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn poll(&mut self) {
        let mut resend = false;

        if self.link_up_delay > 0 {
            self.link_up_delay -= 1;
            resend |= self.link_up_delay == 0;
        }

        self.periodic_delay = self.periodic_delay.saturating_sub(1);
        if self.periodic_delay == 0 {
            self.periodic_delay = PERIODIC_RESEND_TICKS;
            resend = true;
        }

        if resend && let Some(status) = self.status {
            publish_event(status);
        }
    }
}
