//! Processor that drives the WS2812 indicator LEDs.
//!
//! What each LED shows differs between the halves, following the official firmware, and is passed
//! in as [`LEFT_ROLES`] or [`RIGHT_ROLES`]. The event handlers only update state; drawing to the
//! LEDs is concentrated in [`poll`](IndicatorProcessor::poll).

use embassy_nrf::Peri;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pin as GpioPin, Pull};
use embassy_nrf::pwm::Instance as PwmInstance;
use embassy_time::{Duration, Timer};
use rmk::event::{BatteryStatusEvent, CentralConnectedEvent, ConnectionStatusChangeEvent};
use rmk::macros::processor;
use rmk::types::battery::BatteryStatus;
use rmk::types::ble::BleState;
use rmk::types::connection::{ConnectionStatus, UsbState};

use crate::ws2812::{BITS_PER_LED, Rgb, Ws2812};

/// Number of LEDs per half.
pub const LED_COUNT: usize = 2;

/// Transfer buffer length of [`Ws2812`].
const LED_WORDS: usize = LED_COUNT * BITS_PER_LED + 1;

/// Poll interval (ms). Keep it equal to `poll_interval` of `#[processor]`.
const POLL_INTERVAL_MS: u32 = 50;

/// Period (ms) of the heartbeat. Used for the indications the official manual calls a slow blink:
/// searching for a host, charging, and a lost split link.
const HEARTBEAT_PERIOD_MS: u32 = 1000;

/// Period (ms) of the crisp blink. The official manual writes a plain "blink" for a low battery,
/// as opposed to the "slow blink" of the other indications, so it gets its own faster pattern.
const LOW_BATTERY_BLINK_PERIOD_MS: u32 = 500;

/// Lit time (ms) of "lit for one second, then off".
const HOLD_MS: u32 = 1000;

/// Lit time (ms) of the power-on self test.
///
/// In the dongle configuration nothing is displayed once everything is nominal, because the host
/// link is USB on the dongle and the split link is up. Without this the LEDs stay dark and there
/// is no way to tell a working chain from a broken one.
const SELF_TEST_MS: u32 = 2000;

/// Number of polls in one heartbeat cycle.
const HEARTBEAT_TICKS: u32 = HEARTBEAT_PERIOD_MS / POLL_INTERVAL_MS;

/// Number of polls that corresponds to half a low battery blink period.
const LOW_BATTERY_HALF_TICKS: u32 = LOW_BATTERY_BLINK_PERIOD_MS / 2 / POLL_INTERVAL_MS;

/// Number of polls that corresponds to holding the LED lit.
const HOLD_TICKS: u32 = HOLD_MS / POLL_INTERVAL_MS;

/// Number of polls that the power-on self test lasts.
const SELF_TEST_TICKS: u32 = SELF_TEST_MS / POLL_INTERVAL_MS;

/// Threshold (%) below which the battery level is considered low.
const LOW_BATTERY_PERCENT: u8 = 20;

/// Time to wait after enabling the LED supply before sending data, so that the WS2812 supply
/// voltage has settled.
const POWER_SETTLE: Duration = Duration::from_millis(1);

/// Brightness of each component. WS2812 at full brightness hurts the eyes and draws current, so
/// it is lowered to an eighth of the maximum.
const LEVEL: u8 = u8::MAX / 8;

/// Green.
const GREEN: Rgb = Rgb::new(0, LEVEL, 0);
/// Red.
const RED: Rgb = Rgb::new(LEVEL, 0, 0);
/// Blue.
const BLUE: Rgb = Rgb::new(0, 0, LEVEL);
/// Yellow.
const YELLOW: Rgb = Rgb::new(LEVEL, LEVEL, 0);
/// Cyan.
const CYAN: Rgb = Rgb::new(0, LEVEL, LEVEL);

/// Colors that correspond to the BLE profile number. Matches `ble_profiles_num = 5` in
/// `keyboard.toml`.
const PROFILE_COLORS: [Rgb; 5] = [GREEN, RED, BLUE, YELLOW, CYAN];

/// What a single LED shows.
///
/// The official firmware assigns different roles to the two halves, so each half passes its own
/// pair. Index 0 of the pair is the LED nearest the MCU, which is the inner one on both halves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LedRole {
    /// Host connection state of the central.
    HostConnection,
    /// Split link and battery on one LED, the link taking priority.
    BatteryAndLink,
    /// Battery of this half only.
    Battery,
    /// Split link to the central only.
    SplitLink,
}

/// Roles of the left half. Inner shows the battery and the link, outer shows the host connection.
pub const LEFT_ROLES: [LedRole; LED_COUNT] = [LedRole::BatteryAndLink, LedRole::HostConnection];

/// Roles of the right half. Inner shows the battery, outer shows the split link.
pub const RIGHT_ROLES: [LedRole; LED_COUNT] = [LedRole::Battery, LedRole::SplitLink];

/// Scales every component of `color` by `num / den`.
fn scaled(color: Rgb, num: u32, den: u32) -> Rgb {
    let scale =
        |component: u8| u8::try_from(u32::from(component) * num / den.max(1)).unwrap_or(u8::MAX);
    Rgb::new(scale(color.r), scale(color.g), scale(color.b))
}

/// Processor that decides what the indicator LEDs show.
#[processor(subscribe = [ConnectionStatusChangeEvent, CentralConnectedEvent, BatteryStatusEvent], poll_interval = 50)]
pub struct IndicatorProcessor {
    /// LED driver. `None` when initialization failed, in which case nothing is displayed.
    leds: Option<Ws2812<'static, LED_COUNT, LED_WORDS>>,
    /// LED supply enable pin. High supplies the WS2812. It is only raised while something is
    /// displayed, because leaving it on costs battery life.
    power_pin: Output<'static>,
    /// Whether the LED supply is currently enabled.
    powered: bool,
    /// Charge detection pin. An input with internal pull-up where Low means charging.
    charge_pin: Input<'static>,
    /// Host connection state of the central. Synchronized over the split link.
    connection: ConnectionStatus,
    /// Whether the split link to the central is established.
    central_connected: bool,
    /// Battery level (%). `None` when it has not been obtained.
    battery_level: Option<u8>,
    /// Whether the battery is charging.
    charging: bool,
    /// Blink phase. Advances on every poll.
    phase: u32,
    /// Remaining polls to stay lit after the host connection is established.
    host_hold: u32,
    /// Remaining polls to stay lit after the split link is established.
    link_hold: u32,
    /// Remaining polls to stay lit after charging completes.
    charge_done_hold: u32,
    /// Remaining polls of the power-on self test.
    self_test: u32,
    /// What each LED shows. Index 0 is the LED nearest the MCU.
    roles: [LedRole; LED_COUNT],
    /// Colors sent last time. Kept to avoid resending the same content.
    last: Option<[Rgb; LED_COUNT]>,
}

impl IndicatorProcessor {
    /// Builds the processor from the PWM, the LED data pin, the LED supply enable pin, the charge
    /// detection pin and the roles of the two LEDs.
    pub fn new<T: PwmInstance>(
        pwm: Peri<'static, T>,
        data_pin: Peri<'static, impl GpioPin>,
        power_pin: Peri<'static, impl GpioPin>,
        charge_pin: Peri<'static, impl GpioPin>,
        roles: [LedRole; LED_COUNT],
    ) -> Self {
        Self {
            leds: Ws2812::new(pwm, data_pin),
            power_pin: Output::new(power_pin, Level::Low, OutputDrive::Standard),
            powered: false,
            charge_pin: Input::new(charge_pin, Pull::Up),
            connection: ConnectionStatus::new(),
            central_connected: false,
            battery_level: None,
            charging: false,
            phase: 0,
            host_hold: 0,
            link_hold: 0,
            charge_done_hold: 0,
            self_test: SELF_TEST_TICKS,
            roles,
            last: None,
        }
    }

    /// Takes in a change of the host connection state.
    #[expect(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_connection_status_change_event(&mut self, event: ConnectionStatusChangeEvent) {
        let was_connected = self.connection.ble.state == BleState::Connected;
        self.connection = event.0;
        if !was_connected && self.connection.ble.state == BleState::Connected {
            self.host_hold = HOLD_TICKS;
        }
    }

    /// Takes in a change of the split link connection state.
    #[expect(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_central_connected_event(&mut self, event: CentralConnectedEvent) {
        if event.connected && !self.central_connected {
            self.link_hold = HOLD_TICKS;
        }
        self.central_connected = event.connected;
    }

    /// Takes in a change of the battery level.
    #[expect(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        self.battery_level = match event.0 {
            BatteryStatus::Available { level, .. } => level,
            BatteryStatus::Unavailable => None,
        };
    }

    /// Advances the phase and each timer, and sends to the LEDs when the display changed.
    async fn poll(&mut self) {
        self.phase = self.phase.wrapping_add(1);
        self.update_charging();
        self.host_hold = self.host_hold.saturating_sub(1);
        self.link_hold = self.link_hold.saturating_sub(1);
        self.charge_done_hold = self.charge_done_hold.saturating_sub(1);
        self.self_test = self.self_test.saturating_sub(1);

        let colors = if self.self_test > 0 {
            // Distinct colors per LED so that a wrong GRB byte order or a reversed chain order
            // shows up as swapped colors rather than as a plausible-looking result.
            [RED, GREEN]
        } else {
            self.roles.map(|role| self.role_color(role))
        };
        let lit = colors.iter().any(|color| *color != Rgb::OFF);

        if lit && !self.powered {
            self.power_pin.set_high();
            self.powered = true;
            Timer::after(POWER_SETTLE).await;
            // The WS2812 lost its state while unpowered, so the frame has to be resent even when
            // the colors did not change.
            self.last = None;
        }

        if self.last != Some(colors) {
            self.last = Some(colors);
            if let Some(leds) = self.leds.as_mut() {
                leds.write(&colors).await;
            }
        }

        if !lit && self.powered {
            self.powered = false;
            self.power_pin.set_low();
        }
    }

    /// Reads the charge detection pin and detects charge completion (the transition from
    /// charging to not charging).
    fn update_charging(&mut self) {
        let charging = self.charge_pin.is_low();
        if self.charging
            && !charging
            && self
                .battery_level
                .is_some_and(|level| level >= LOW_BATTERY_PERCENT)
        {
            self.charge_done_hold = HOLD_TICKS;
        }
        self.charging = charging;
    }

    /// `color` dimmed along a triangular heartbeat.
    ///
    /// Brightness ramps up and back down over one cycle, so the LED is fully dark only for the
    /// single tick at the trough. This matches the observed behavior of the official firmware,
    /// where the dark part is momentary rather than half of the cycle.
    fn heartbeat(&self, color: Rgb) -> Rgb {
        let half = HEARTBEAT_TICKS / 2;
        let position = self.phase % HEARTBEAT_TICKS;
        let level = if position < half {
            position
        } else {
            HEARTBEAT_TICKS - position
        };
        scaled(color, level, half)
    }

    /// `true` when the low battery blink is in its lit phase.
    fn low_battery_blink_on(&self) -> bool {
        (self.phase / LOW_BATTERY_HALF_TICKS).is_multiple_of(2)
    }

    /// Color for one LED according to the role assigned to it.
    fn role_color(&self, role: LedRole) -> Rgb {
        match role {
            LedRole::HostConnection => self.host_color(),
            LedRole::BatteryAndLink => self.battery_and_link_color(),
            LedRole::Battery => self.battery_color(),
            LedRole::SplitLink => self.split_link_color(),
        }
    }

    /// Shows the BLE host connection state of the central.
    ///
    /// Stays off while BLE is `Inactive`.
    fn host_color(&self) -> Rgb {
        let Some(&color) = PROFILE_COLORS.get(usize::from(self.connection.ble.profile)) else {
            return Rgb::OFF;
        };
        match self.connection.ble.state {
            // Flash regardless of which transport routes reports, so that pairing stays visible
            // even while the dongle is also plugged into a host over USB.
            BleState::Connected if self.host_hold > 0 => color,
            // Blink only when no host is actively using the dongle over USB. The central
            // advertises even while USB is in use, so blinking unconditionally would blink
            // through normal USB operation.
            BleState::Advertising if !self.usb_in_use() => self.heartbeat(color),
            _ => Rgb::OFF,
        }
    }

    /// Whether a host is actively using the dongle over USB.
    ///
    /// Deliberately narrower than `ConnectionStatus::usb_ready` (which is private to RMK and also
    /// counts `Suspended`): a charger that enumerates and then suspends, or a sleeping host, is
    /// not active use, and the search indication is wanted in those cases.
    fn usb_in_use(&self) -> bool {
        self.connection.usb == UsbState::Configured
    }

    /// Shows the split link and the battery on one LED, the link taking priority.
    fn battery_and_link_color(&self) -> Rgb {
        let link = self.split_link_color();
        if link == Rgb::OFF {
            self.battery_color()
        } else {
            link
        }
    }

    /// Shows the battery of this half: charging, charge completion, then a low level.
    ///
    /// The battery is not judged to be low while its level has not been obtained.
    fn battery_color(&self) -> Rgb {
        if self.charging {
            return self.heartbeat(GREEN);
        }
        if self.charge_done_hold > 0 {
            return GREEN;
        }
        if self
            .battery_level
            .is_some_and(|level| level < LOW_BATTERY_PERCENT)
        {
            return if self.low_battery_blink_on() {
                RED
            } else {
                Rgb::OFF
            };
        }
        Rgb::OFF
    }

    /// Shows the split link to the central.
    fn split_link_color(&self) -> Rgb {
        if !self.central_connected {
            return self.heartbeat(BLUE);
        }
        if self.link_hold > 0 {
            return BLUE;
        }
        Rgb::OFF
    }
}
