//! Driver that drives WS2812 LEDs with the PWM sequence feature of the nRF52840.
//!
//! One WS2812 bit period of 1.25us is mapped to one PWM period, and logical 0 / 1 are
//! expressed by the width of the High pulse at the start of the period. This module does not
//! decide what to show; it sends out the colors it is given as-is.

use embassy_nrf::Peri;
use embassy_nrf::gpio::{OutputDrive, Pin as GpioPin};
use embassy_nrf::pwm::{
    Config as PwmConfig, Instance as PwmInstance, Prescaler, SequenceConfig, SequencePwm,
    SingleSequenceMode, SingleSequencer,
};
use embassy_time::{Duration, Timer};

/// Number of bits per LED. 8 bits each for G, R and B.
pub const BITS_PER_LED: usize = 24;

/// `COUNTERTOP` of the PWM.
///
/// In up mode the nRF52840 PWM gives "period = `PWM_CLK` period x `COUNTERTOP`", so with
/// `PRESCALER` = `DIV_1` (16MHz, 62.5ns per count) 20 counts make 1.25us, which matches the
/// WS2812 bit period. The official firmware V1.12 uses the same value.
const COUNTERTOP: u16 = 20;

/// Bit 15 of a sequence word. 1 means `POLARITY` = `FallingEdge`, so the period starts High.
const FALLING_EDGE: u16 = 0x8000;

/// High width of a logical 0. T0H = 0.4us, so 0.4us / 62.5ns = 6.4 truncated to 6 counts (375ns).
const T0H: u16 = FALLING_EDGE | 0x0006;

/// High width of a logical 1. T1H = 0.8us, so 0.8us / 62.5ns = 12.8 rounded to 13 counts (812.5ns).
const T1H: u16 = FALLING_EDGE | 0x000d;

/// Word whose whole period is Low because COMPARE = 0. Placed at the end of the buffer to form
/// the reset period.
const RESET_WORD: u16 = FALLING_EDGE;

/// Number of PWM periods of the reset period.
///
/// During ENDDELAY the last value read from RAM keeps being output, so the trailing
/// [`RESET_WORD`] is extended by this many periods. One period is 1.25us, so 50 periods make
/// 62.5us, satisfying the 50us or more that WS2812 requires.
const RESET_PERIODS: u32 = 50;

/// Margin (us) added while waiting for the transfer to finish. It absorbs the resolution of
/// embassy-time (32768Hz, about 30.5us).
const SEND_MARGIN_US: u64 = 250;

/// Color of a single LED sent to WS2812.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    /// Red component.
    pub r: u8,
    /// Green component.
    pub g: u8,
    /// Blue component.
    pub b: u8,
}

impl Rgb {
    /// Off.
    pub const OFF: Self = Self::new(0, 0, 0);

    /// Builds a color from the individual components.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// WS2812 driver.
///
/// `N` is the number of LEDs and `WORDS` is the length of the transfer buffer, which must be
/// `N * BITS_PER_LED + 1`. They are split into two const parameters because stable Rust cannot
/// compute an array length from a type parameter.
pub struct Ws2812<'d, const N: usize, const WORDS: usize> {
    pwm: SequencePwm<'d>,
    words: [u16; WORDS],
}

impl<'d, const N: usize, const WORDS: usize> Ws2812<'d, N, WORDS> {
    /// Initializes the data pin as a 1ch PWM output.
    ///
    /// Returns `None` when `WORDS` is not `N * BITS_PER_LED + 1` and when the PWM fails to
    /// initialize. The caller can give up on the LED display and keep running.
    pub fn new<T: PwmInstance>(pwm: Peri<'d, T>, data_pin: Peri<'d, impl GpioPin>) -> Option<Self> {
        if WORDS != N * BITS_PER_LED + 1 {
            return None;
        }

        let mut config = PwmConfig::default();
        config.prescaler = Prescaler::Div1;
        config.max_duty = COUNTERTOP;
        // The data line needs fast edges to carry a 375ns pulse. embassy defaults to standard
        // drive, while the official firmware V1.12 writes PIN_CNF = 0x303, which is H0H1.
        config.ch0_drive = OutputDrive::HighDrive;

        Some(Self {
            pwm: SequencePwm::new_1ch(pwm, data_pin, config).ok()?,
            words: [RESET_WORD; WORDS],
        })
    }

    /// Expands `N` colors into bits in GRB order, MSB first, and sends them out as one frame.
    pub async fn write(&mut self, colors: &[Rgb; N]) {
        for (led, color) in colors.iter().enumerate() {
            for (byte_index, byte) in [color.g, color.r, color.b].into_iter().enumerate() {
                for bit in 0..8 {
                    let word = if byte & (0x80_u8 >> bit) == 0 {
                        T0H
                    } else {
                        T1H
                    };
                    if let Some(slot) = self
                        .words
                        .get_mut(led * BITS_PER_LED + byte_index * 8 + bit)
                    {
                        *slot = word;
                    }
                }
            }
        }

        let mut config = SequenceConfig::default();
        config.end_delay = RESET_PERIODS;

        let sequencer = SingleSequencer::new(&mut self.pwm, &self.words, config);
        if sequencer.start(SingleSequenceMode::Times(1)).is_ok() {
            // Dropping the SingleSequencer stops the PWM, so wait until the DMA transfer and the
            // reset period are over before leaving the scope.
            Timer::after(Self::frame_duration()).await;
        }
    }

    /// Time it takes to send one frame.
    fn frame_duration() -> Duration {
        let periods = WORDS as u64 + u64::from(RESET_PERIODS);
        // One period is 1.25us = 5/4 us
        Duration::from_micros(periods * 5 / 4 + SEND_MARGIN_US)
    }
}
