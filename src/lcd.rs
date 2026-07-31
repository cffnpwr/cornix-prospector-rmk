//! ST7789 LCD of the dongle: SPI bus setup, backlight PWM and the processor that redraws it.
//!
//! The panel is a 240x280 ST7789 driven through a SPIM. It is rotated by [`ROTATION`], so the
//! logical resolution is [`WIDTH`] x [`HEIGHT`] in landscape. `LcdAsyncDisplay` has no partial
//! update and pushes the whole framebuffer on every flush, so redraws are rate limited by
//! [`MIN_RENDER_INTERVAL`].
//!
//! This module only decides when to redraw; what is drawn lives in [`crate::status_screen`].

use core::convert::Infallible;

use embassy_nrf::Peri;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pin as GpioPin};
use embassy_nrf::interrupt::typelevel::Binding;
use embassy_nrf::pwm::{DutyCycle, Instance as PwmInstance, Prescaler, SimpleConfig, SimplePwm};
use embassy_nrf::spim::{
    Config as SpimConfig, Frequency, Instance as SpimInstance,
    InterruptHandler as SpimInterruptHandler, Spim,
};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_hal::digital::{ErrorType, OutputPin};
use embedded_hal_bus::spi::ExclusiveDevice;
use rmk::display::DisplayDriver as _;
use rmk::display::drivers::lcd_async::LcdAsyncDisplay;
use rmk::display::lcd_async::Builder;
use rmk::display::lcd_async::interface::SpiInterface;
use rmk::display::lcd_async::models::ST7789;
use rmk::display::lcd_async::options::{ColorInversion, ColorOrder, Orientation, Rotation};
use rmk::event::LayerChangeEvent;
use rmk::macros::processor;
use static_cell::StaticCell;

use crate::status_screen;

/// Width (px) of the panel in its native orientation.
const PANEL_WIDTH: u16 = 240;

/// Height (px) of the panel in its native orientation.
const PANEL_HEIGHT: u16 = 280;

/// X offset (px) of the panel inside the 240x320 frame memory of the ST7789.
const PANEL_OFFSET_X: u16 = 0;

/// Y offset (px) of the panel inside the 240x320 frame memory of the ST7789.
///
/// The Prospector overlay declares this panel with a y offset of 20. Confirmed on hardware: the
/// drawn area reaches all four edges, with neither clipping nor a margin.
const PANEL_OFFSET_Y: u16 = 20;

/// Rotation applied to the panel.
///
/// The official Prospector firmware uses the panel in landscape. `Deg90` and `Deg270` are both
/// landscape and differ by 180 degrees. Confirmed on hardware: with `Deg90` every corner of the
/// framebuffer lands on the matching corner of the panel.
const ROTATION: Rotation = Rotation::Deg90;

/// Whether the panel needs its colors inverted.
///
/// Confirmed on hardware: with `Normal` every component came out complemented, black showing as
/// white and red as cyan, so this panel needs INVON.
const COLOR_INVERSION: ColorInversion = ColorInversion::Inverted;

/// Subpixel order of the panel.
///
/// Confirmed on hardware: red and blue are not swapped, which `Bgr` would have done.
const COLOR_ORDER: ColorOrder = ColorOrder::Rgb;

/// Logical width (px) after [`ROTATION`]. Equals [`PANEL_HEIGHT`].
pub const WIDTH: usize = 280;

/// Logical height (px) after [`ROTATION`]. Equals [`PANEL_WIDTH`].
pub const HEIGHT: usize = 240;

/// Framebuffer length (bytes). Rgb565 takes 2 bytes per pixel.
const FRAME_BUFFER_LEN: usize = WIDTH * HEIGHT * 2;

/// SPI clock.
///
/// The panel is specified up to 31MHz. This starts on the safe side; the usable maximum is
/// measured on hardware.
const SPI_FREQUENCY: Frequency = Frequency::M8;

/// Whether the levels of the DC pin are swapped.
///
/// `SpiInterface` drives DC Low for a command and High for data, which is the usual wiring.
/// Confirmed on hardware: the panel accepts the initialization sequence and shows what is drawn,
/// which it would not with the levels the other way round. Flipping this swaps the two levels.
const DC_INVERTED: bool = false;

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

/// Brightness the backlight is set to once the first frame is on the panel.
const BACKLIGHT_DEFAULT: u16 = BACKLIGHT_MAX;

/// Shortest time between two flushes.
///
/// One flush transfers the whole framebuffer, so without a limit a burst of events would queue up
/// transfers faster than the bus can drain them. RMK's own `DisplayProcessor` starts from the same
/// 33ms.
const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(33);

/// Framebuffer of the panel. Too large to build on the stack, so it lives in a static.
static FRAME_BUFFER: StaticCell<[u8; FRAME_BUFFER_LEN]> = StaticCell::new();

/// SPI device of the panel: the SPIM plus its chip select.
type Bus = ExclusiveDevice<Spim<'static>, Output<'static>, Delay>;

/// 4-line serial interface of the panel.
type PanelInterface = SpiInterface<Bus, PolarityPin>;

/// Initialized panel together with its framebuffer.
type Panel = LcdAsyncDisplay<
    PanelInterface,
    ST7789,
    Output<'static>,
    &'static mut [u8; FRAME_BUFFER_LEN],
    WIDTH,
    HEIGHT,
>;

/// Output pin whose logical level can be swapped at build time.
///
/// Used for DC, so that [`DC_INVERTED`] alone decides the polarity of the pin.
struct PolarityPin {
    pin: Output<'static>,
    inverted: bool,
}

impl PolarityPin {
    /// Wraps `pin`, swapping every level when `inverted`.
    const fn new(pin: Output<'static>, inverted: bool) -> Self {
        Self { pin, inverted }
    }
}

impl ErrorType for PolarityPin {
    type Error = Infallible;
}

impl OutputPin for PolarityPin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        if self.inverted {
            self.pin.set_high();
        } else {
            self.pin.set_low();
        }
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        if self.inverted {
            self.pin.set_low();
        } else {
            self.pin.set_high();
        }
        Ok(())
    }
}

/// Backlight of the panel, driven by one PWM channel.
pub struct Backlight {
    pwm: SimplePwm<'static>,
}

impl Backlight {
    /// Sets the backlight pin up as a PWM output, initially dark.
    pub fn new<T: PwmInstance>(pwm: Peri<'static, T>, pin: Peri<'static, impl GpioPin>) -> Self {
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
    pub fn set_brightness(&mut self, brightness: u16) {
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

/// Processor that keeps the panel showing the current layer.
///
/// RMK's own `DisplayProcessor` is not used: its `RenderContext` carries only the BLE status and
/// cannot tell whether reports are going out over USB or over BLE, which this display has to show.
#[processor(subscribe = [LayerChangeEvent])]
pub struct LcdProcessor {
    /// Panel. `None` when initialization failed, in which case nothing is displayed and the rest
    /// of the firmware keeps running.
    panel: Option<Panel>,
    /// Backlight of the panel.
    backlight: Backlight,
    /// Layer reported last.
    layer: u8,
    /// Layer the panel currently shows.
    drawn_layer: Option<u8>,
    /// When the last flush finished, used by the redraw rate limit.
    last_render: Instant,
}

impl LcdProcessor {
    /// Brings the backlight, the SPI bus and the panel up, then shows the first frame.
    ///
    /// The order matters twice. The backlight PWM is set up before the panel is touched, because
    /// otherwise its pin would be left floating for the roughly 300ms the initialization sequence
    /// takes. It is set up dark and only lit once the first frame has reached the panel, so the
    /// noise the panel holds after reset is never shown. When the panel fails to come up it is
    /// dropped and the backlight stays dark, leaving the rest of the firmware unaffected.
    #[allow(
        clippy::too_many_arguments,
        reason = "the panel needs six pins next to its two peripherals, and grouping them would only move the list elsewhere"
    )]
    pub async fn new<S: SpimInstance, P: PwmInstance>(
        spim: Peri<'static, S>,
        irq: impl Binding<S::Interrupt, SpimInterruptHandler<S>> + 'static,
        sck: Peri<'static, impl GpioPin>,
        mosi: Peri<'static, impl GpioPin>,
        cs: Peri<'static, impl GpioPin>,
        dc: Peri<'static, impl GpioPin>,
        reset: Peri<'static, impl GpioPin>,
        pwm: Peri<'static, P>,
        backlight_pin: Peri<'static, impl GpioPin>,
    ) -> Self {
        // Built before the panel is touched, so the backlight pin is driven dark rather than left
        // floating while the initialization sequence runs.
        let backlight = Backlight::new(pwm, backlight_pin);
        let panel = Self::init_panel(spim, irq, sck, mosi, cs, dc, reset).await;

        let mut processor = Self {
            panel,
            backlight,
            layer: 0,
            drawn_layer: None,
            // Far enough in the past that the first frame is not held back by the rate limit.
            last_render: Instant::from_ticks(0),
        };

        processor.render().await;
        if processor.panel.is_some() {
            processor.backlight.set_brightness(BACKLIGHT_DEFAULT);
        }
        processor
    }

    /// Sets the backlight brightness. See [`Backlight::set_brightness`] for the range.
    pub fn set_brightness(&mut self, brightness: u16) {
        self.backlight.set_brightness(brightness);
    }

    /// Builds the SPI bus and runs the initialization sequence of the panel.
    ///
    /// Returns `None` when the bus, the panel or the framebuffer fails to come up, so that the
    /// caller can give up on the display and keep running.
    #[allow(
        clippy::too_many_arguments,
        reason = "the panel needs six pins next to its SPIM, and grouping them would only move the list elsewhere"
    )]
    async fn init_panel<S: SpimInstance>(
        spim: Peri<'static, S>,
        irq: impl Binding<S::Interrupt, SpimInterruptHandler<S>> + 'static,
        sck: Peri<'static, impl GpioPin>,
        mosi: Peri<'static, impl GpioPin>,
        cs: Peri<'static, impl GpioPin>,
        dc: Peri<'static, impl GpioPin>,
        reset: Peri<'static, impl GpioPin>,
    ) -> Option<Panel> {
        let mut config = SpimConfig::default();
        config.frequency = SPI_FREQUENCY;

        let bus = Spim::new_txonly(spim, irq, sck, mosi, config);
        // Chip select idles High; ExclusiveDevice raises it again itself.
        let cs = Output::new(cs, Level::High, OutputDrive::Standard);
        let device = ExclusiveDevice::new(bus, cs, Delay).ok()?;

        let dc = PolarityPin::new(
            Output::new(dc, Level::Low, OutputDrive::Standard),
            DC_INVERTED,
        );
        // Reset idles High; the builder pulses it Low itself.
        let reset = Output::new(reset, Level::High, OutputDrive::Standard);

        // The size and the offset are given in the native orientation of the panel, because
        // lcd-async maps them onto the frame memory itself once it knows the rotation.
        let panel = Builder::new(ST7789, SpiInterface::new(device, dc))
            .reset_pin(reset)
            .display_size(PANEL_WIDTH, PANEL_HEIGHT)
            .display_offset(PANEL_OFFSET_X, PANEL_OFFSET_Y)
            .orientation(Orientation::new().rotate(ROTATION))
            .invert_colors(COLOR_INVERSION)
            .color_order(COLOR_ORDER)
            .init(&mut Delay)
            .await
            .ok()?;

        Some(LcdAsyncDisplay::new(panel, Self::frame_buffer()?))
    }

    /// Hands out the framebuffer, once.
    fn frame_buffer() -> Option<&'static mut [u8; FRAME_BUFFER_LEN]> {
        let buffer = FRAME_BUFFER.try_uninit()?;
        // SAFETY: `u8` has no invalid bit patterns and `write_bytes` fills every byte of the
        // array. It is filled in place because passing the array by value would put a
        // FRAME_BUFFER_LEN byte temporary on the stack.
        Some(unsafe {
            buffer.as_mut_ptr().write_bytes(0, 1);
            buffer.assume_init_mut()
        })
    }

    /// Takes in a layer change.
    async fn on_layer_change_event(&mut self, event: LayerChangeEvent) {
        self.layer = event.0;
        self.render().await;
    }

    /// Redraws and flushes when the content changed, waiting out [`MIN_RENDER_INTERVAL`] first.
    ///
    /// Waiting inside the handler leaves further events queued in the subscriber, so a burst
    /// collapses into a single flush of the newest state.
    async fn render(&mut self) {
        let layer = self.layer;
        if self.drawn_layer == Some(layer) || self.panel.is_none() {
            return;
        }

        let elapsed = self.last_render.elapsed();
        if elapsed < MIN_RENDER_INTERVAL {
            Timer::after(MIN_RENDER_INTERVAL - elapsed).await;
        }

        if let Some(panel) = self.panel.as_mut() {
            status_screen::draw(panel, layer);
            panel.flush().await;
        }

        self.drawn_layer = Some(layer);
        self.last_render = Instant::now();
    }
}
