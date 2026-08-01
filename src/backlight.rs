//! Backlight of the dongle LCD: the PWM output and the processor that steps it from the keyboard.
//!
//! This is a module of its own rather than part of [`crate::lcd`] so that stepping the brightness
//! never shares a task with drawing. `ActionEvent` is published for every key action that resolves
//! (`rmk/src/keyboard.rs:1217-1222`) and it is published with back pressure: `publish_event_async`
//! reaches `embassy_sync::pubsub::Publisher::publish`, which waits while the channel is full
//! (`embassy-sync-0.7.2/src/pubsub/publisher.rs:35-41`), and the publish stops being a no-op as
//! soon as the event has a subscriber (`PUBLISH_IS_NOOP` is `subs == 0`,
//! `rmk-macro/src/event.rs:162`). A processor handles its events one at a time, so subscribing to
//! `ActionEvent` from [`LcdProcessor`](crate::lcd::LcdProcessor) would let a full screen flush hold
//! up the keyboard task once the 16 slot channel filled. Nothing here draws, so the channel is
//! always drained promptly and the display can never delay a keystroke.
//!
//! The display still has to show the step being set, which it takes from [`BRIGHTNESS`]: the step
//! goes to the panel through a static that [`crate::lcd`] polls, so that the subscription to
//! `ActionEvent` stays here and stays the only one.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_nrf::Peri;
use embassy_nrf::gpio::{Level, Pin as GpioPin};
use embassy_nrf::pwm::{DutyCycle, Instance as PwmInstance, Prescaler, SimpleConfig, SimplePwm};
use rmk::event::ActionEvent;
use rmk::macros::processor;
use rmk::types::action::Action;

use crate::lcd;

/// Whether a Low level lights the backlight.
///
/// Confirmed on hardware: driving the pin permanently Low leaves the backlight dark while a
/// permanent High lights it, so the backlight is active high. Flipping this reverses both the duty
/// polarity and the level the pin idles at.
const BACKLIGHT_ACTIVE_LOW: bool = false;

/// `COUNTERTOP` of the backlight PWM, which is also the brightest value accepted by
/// [`Backlight::set_brightness`].
const BACKLIGHT_MAX: u16 = 1000;

/// Prescaler of the backlight PWM. `Div16` gives 1MHz, so [`BACKLIGHT_MAX`] makes a 1kHz period.
const BACKLIGHT_PRESCALER: Prescaler = Prescaler::Div16;

/// Brightness of every backlight step, from off to the brightest.
///
/// Step 0 is off. Steps 1 to 15 are a geometric progression from 10 to [`BACKLIGHT_MAX`], laid out
/// as exactly one decade of duty every seven steps: step `i` is `10.0f64.powf((i + 6) / 7.0)`
/// rounded, which makes each step about 1.39x the one below and every seventh step exactly ten
/// times the one before it. An even split of [`BACKLIGHT_MAX`] is not used because perceived
/// brightness grows far more slowly than the duty cycle, which would put most of the perceived
/// range into the lowest step and leave the upper steps hard to tell apart. The curve is a
/// starting point, not a measurement: no photometry was done on this panel, and whether step 1 (1%
/// duty) is visible at all is unconfirmed.
///
/// Step 0 turns the backlight fully off, which costs the only feedback the dongle has. The panel
/// is the only place it reports its state, so while the backlight is off a working dongle and a
/// failed display look the same. Being able to darken the dongle completely is worth that.
///
/// Sixteen steps means fifteen presses to cross the whole range.
const BACKLIGHT_LEVELS: [u16; 16] = [
    0,
    10,
    14,
    19,
    27,
    37,
    52,
    72,
    100,
    139,
    193,
    268,
    373,
    518,
    720,
    BACKLIGHT_MAX,
];

/// Index into [`BACKLIGHT_LEVELS`] the backlight starts at, and returns to on every power cycle:
/// the brightness is deliberately not persisted.
const BACKLIGHT_DEFAULT_LEVEL: usize = BACKLIGHT_LEVELS.len() - 1;

/// `Action::User` id that steps the backlight up.
///
/// The id is the position of the keycode in `customKeycodes` of `vial.json`, so the two constants
/// here have to stay in step with that list. Ids `0..=9` are taken by the BLE profile keys RMK
/// handles itself (`rmk/src/keyboard.rs:1699-1721`, with `ble_profiles_num = 5`).
const BRIGHTNESS_UP_ID: u8 = 10;

/// `Action::User` id that steps the backlight down. See [`BRIGHTNESS_UP_ID`].
const BRIGHTNESS_DOWN_ID: u8 = 11;

/// Number of bits [`BRIGHTNESS`] keeps the percentage in, which is also what the revision above it
/// is shifted by.
const BRIGHTNESS_PERCENT_BITS: u32 = 8;

/// Mask of the percentage in [`BRIGHTNESS`].
const BRIGHTNESS_PERCENT_MASK: u32 = 0xff;

/// The brightness the screen shows, as `revision << BRIGHTNESS_PERCENT_BITS | percent`.
///
/// This is the whole path from the keycodes to [`crate::lcd`], which watches it and slides the
/// brightness overlay in when it changes. It is a static rather than an event because the display
/// must not subscribe to `ActionEvent`, see the module documentation, and it is one word rather
/// than two so that a reader gets a percentage and the revision it belongs to from a single load,
/// with no ordering to reason about between two locations.
///
/// `Relaxed` is enough for both sides: the word publishes no other data, so there is nothing for an
/// acquire or a release to order it against, and a reader only compares the revision against the
/// one it saw last.
///
/// The value here is only what stands until [`BacklightProcessor::new`] publishes the step the
/// backlight starts at, which happens before anything reads it: the initializers of
/// `#[register_processor]` run in the order the functions appear in `crate::central`, and the
/// backlight is registered ahead of the display.
static BRIGHTNESS: AtomicU32 = AtomicU32::new(0);

/// The brightness [`crate::lcd`] shows, as it stands now.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Brightness {
    /// How far up [`BACKLIGHT_LEVELS`] the backlight is, from 0 for off to 100 for the brightest.
    /// This is a position in the steps and not the duty cycle, which grows geometrically: the
    /// overlay shows which step of the sixteen is in use, not how much light comes out.
    pub percent: u8,
    /// Counts how often a brightness key has set the backlight, including the presses at either
    /// end that leave the step where it is. A reader that only compared the percentage would miss
    /// those, and miss a step down and back up between two of its own reads as well. It wraps
    /// around, which is harmless because it is only ever compared for equality.
    pub revision: u8,
}

/// The brightness as it stands now. See [`BRIGHTNESS`].
#[must_use]
pub fn brightness() -> Brightness {
    let packed = BRIGHTNESS.load(Ordering::Relaxed);
    Brightness {
        percent: u8::try_from(packed & BRIGHTNESS_PERCENT_MASK).unwrap_or(0),
        revision: u8::try_from((packed >> BRIGHTNESS_PERCENT_BITS) & BRIGHTNESS_PERCENT_MASK)
            .unwrap_or(0),
    }
}

/// Backlight of the panel, driven by one PWM channel.
struct Backlight {
    pwm: SimplePwm<'static>,
}

impl Backlight {
    /// Sets the backlight pin up as a PWM output, initially dark.
    fn new<T: PwmInstance>(pwm: Peri<'static, T>, pin: Peri<'static, impl GpioPin>) -> Self {
        let mut config = SimpleConfig::default();
        config.prescaler = BACKLIGHT_PRESCALER;
        config.max_duty = BACKLIGHT_MAX;
        // Level the pin holds while the PWM generator is off, which has to be the dark one.
        config.ch0_idle_level = if BACKLIGHT_ACTIVE_LOW {
            Level::High
        } else {
            Level::Low
        };

        let mut backlight = Self {
            pwm: SimplePwm::new_1ch(pwm, pin, &config),
        };
        backlight.set_brightness(0);
        backlight
    }

    /// Sets the brightness, from 0 for dark up to [`BACKLIGHT_MAX`] for the brightest. Larger
    /// values are clamped.
    fn set_brightness(&mut self, brightness: u16) {
        let duty = brightness.min(BACKLIGHT_MAX);
        // With normal polarity the pin is High while the counter is at or above the duty value,
        // so the duty value is the Low fraction of the period. That is what an active low
        // backlight wants; an active high one needs the inverted polarity.
        self.pwm.set_duty(
            0,
            DutyCycle::normal(duty).with_inverted(!BACKLIGHT_ACTIVE_LOW),
        );
    }
}

/// Processor that owns the backlight and steps it from the two brightness keycodes.
///
/// **Register this before [`LcdProcessor`](crate::lcd::LcdProcessor).** The initializers of
/// `#[register_processor]` are emitted in the order the functions appear in the module
/// (`rmk-macro/src/codegen/registered_processor.rs:30-71`), and the backlight pin has to be driven
/// dark before the panel is touched, or it would be left floating for the roughly 300ms the
/// initialization sequence of the panel takes.
///
/// Lighting the backlight waits for the first frame, so that the noise the panel holds after reset
/// is never shown. The poll interval of the `#[processor]` attribute (100ms) is what checks
/// [`lcd::panel_ready`] for that. A flag polled from here is used rather than a shared handle to
/// the [`Backlight`] because it leaves this processor the sole owner of the PWM; the cost is a
/// timer that keeps ticking after the backlight is lit and up to one interval of extra darkness at
/// boot, neither of which matters on a dongle running off USB power. `panel_ready` stays false
/// forever when the panel fails to come up, which is what keeps a dead panel dark.
#[processor(subscribe = [ActionEvent], poll_interval = 100)]
pub struct BacklightProcessor {
    /// Backlight, dark until the panel has shown its first frame.
    backlight: Backlight,
    /// Index into [`BACKLIGHT_LEVELS`] the backlight is at, or will be set to once it is lit.
    level: usize,
    /// How often a brightness key has set the level, see [`Brightness::revision`].
    revision: u8,
    /// Whether the backlight has been lit, that is, whether [`level`](Self::level) is what the PWM
    /// currently outputs.
    lit: bool,
}

impl BacklightProcessor {
    /// Sets the backlight pin up as a PWM output, leaves it dark and publishes the step it will
    /// come up at.
    ///
    /// The step is published here rather than when the backlight is lit so that [`crate::lcd`],
    /// which is built afterwards, starts out agreeing with it and shows no overlay at boot.
    pub fn new<T: PwmInstance>(pwm: Peri<'static, T>, pin: Peri<'static, impl GpioPin>) -> Self {
        let processor = Self {
            backlight: Backlight::new(pwm, pin),
            level: BACKLIGHT_DEFAULT_LEVEL,
            revision: 0,
            lit: false,
        };
        processor.publish();
        processor
    }

    /// Drives the PWM to the current level and records that the backlight is lit.
    fn apply(&mut self) {
        self.backlight.set_brightness(BACKLIGHT_LEVELS[self.level]);
        self.lit = true;
    }

    /// Publishes the current level and revision to [`BRIGHTNESS`].
    ///
    /// The percentage is rounded, so that the two ends of the range are the only steps that come
    /// out as 0 and as 100.
    fn publish(&self) {
        let top = BACKLIGHT_LEVELS.len() - 1;
        let percent = u8::try_from((self.level * 100 + top / 2) / top.max(1)).unwrap_or(100);
        BRIGHTNESS.store(
            (u32::from(self.revision) << BRIGHTNESS_PERCENT_BITS) | u32::from(percent),
            Ordering::Relaxed,
        );
    }

    /// Steps the backlight when a brightness key is pressed, and ignores every other action.
    ///
    /// Handled on press. `ActionEvent` is published for both edges of a key
    /// (`rmk/src/keyboard.rs:1217-1222`, the head of `process_key_action_normal`), so one edge has
    /// to be dropped or a single keystroke would move two steps. Press is taken because the panel
    /// then changes while the key is still down, which is the feedback a repeated adjustment key
    /// wants. This differs from the user keys RMK handles itself, which act on release
    /// (`rmk/src/keyboard.rs:1699-1721`); the reason for that choice is not documented there.
    ///
    /// Both ends saturate instead of wrapping around, so that holding down one of the two keys
    /// settles at an end rather than jumping back to the other one.
    ///
    /// Nothing happens before the backlight is lit, so that a key pressed during startup cannot
    /// light a panel that is still showing what it held after reset, nor one that failed to come
    /// up at all.
    #[allow(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn on_action_event(&mut self, event: ActionEvent) {
        if !event.keyboard_event.pressed || !self.lit {
            return;
        }

        self.level = match event.action {
            Action::User(BRIGHTNESS_UP_ID) => (self.level + 1).min(BACKLIGHT_LEVELS.len() - 1),
            Action::User(BRIGHTNESS_DOWN_ID) => self.level.saturating_sub(1),
            _ => return,
        };
        self.apply();
        // Counted even when the level did not move, so that a press against either end keeps the
        // overlay on screen for as long as the key is being pressed.
        self.revision = self.revision.wrapping_add(1);
        self.publish();
    }

    /// Lights the backlight once the panel has shown its first frame, then has nothing left to do.
    #[allow(
        clippy::unused_async,
        reason = "must stay async because #[processor] calls it with await"
    )]
    async fn poll(&mut self) {
        if self.lit || !lcd::panel_ready() {
            return;
        }
        self.apply();
    }
}
