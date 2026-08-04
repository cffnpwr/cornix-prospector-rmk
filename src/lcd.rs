//! ST7789 LCD of the dongle: SPI bus setup and the processor that redraws the panel.
//!
//! The panel is a 240x280 ST7789 driven through a SPIM. It is rotated by [`ROTATION`], so the
//! logical resolution is [`WIDTH`] x [`HEIGHT`] in landscape. A redraw only clears, draws and sends
//! the bands of [`crate::status_screen`] that the new state changes, each of which
//! [`framebuffer::FrameBuffer::flush`] sends as one transfer; the whole framebuffer costs about
//! 34ms to send on hardware, and a band costs that in proportion to its rows.
//!
//! This module only decides when to redraw; what is drawn lives in [`crate::status_screen`] and
//! the backlight belongs to [`crate::backlight`]. The brightness overlay is the one thing that is
//! not redrawn from an event: it is an animation, so [`LcdProcessor::poll`] steps it and watches
//! [`crate::backlight::brightness`] for the changes that start it.

mod framebuffer;
mod spim3_anomaly198;

use core::convert::Infallible;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_nrf::Peri;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pin as GpioPin};
use embassy_nrf::interrupt::typelevel::Binding;
use embassy_nrf::peripherals::SPI3;
use embassy_nrf::spim::{
    Config as SpimConfig, Frequency, Instance as SpimInstance,
    InterruptHandler as SpimInterruptHandler, Spim,
};
use embassy_time::{Delay, Duration, Instant};
use embedded_hal::digital::{ErrorType, OutputPin};
use embedded_hal_bus::spi::ExclusiveDevice;
use rmk::display::lcd_async::Builder;
use rmk::display::lcd_async::interface::SpiInterface;
use rmk::display::lcd_async::models::ST7789;
use rmk::display::lcd_async::options::{ColorInversion, ColorOrder, Orientation, Rotation};
use rmk::event::{
    ConnectionStatusChangeEvent, LayerChangeEvent, ModifierEvent, PeripheralBatteryEvent,
    PeripheralConnectedEvent,
};
use rmk::macros::processor;
use rmk::types::battery::BatteryStatus;
use static_cell::StaticCell;

use crate::backlight::{self, Brightness};
use crate::lcd::framebuffer::FrameBuffer;
use crate::lcd::spim3_anomaly198::Spim3Anomaly198Interface;
use crate::status_screen::{self, BRIGHTNESS_SLIDE_FRAMES, BrightnessOverlay, Status};

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
/// The panel is specified up to 31MHz, and embassy only offers 8 / 16 / 32MHz
/// (`embassy-nrf-0.11.0/src/spim.rs:44-51`), so this runs it about 3% over. That is a panel limit
/// rather than a silicon one, so a violation would show up as a disturbed picture; **whether this
/// panel tolerates it has to come from hardware and is not confirmed here.** Errata 189 ("SPIM: RX
/// buffer error at 32 MHz operation") does not apply: it is limited to variant `0x01` (Engineering
/// B) while this board is a Revision 3 (`nrfconnect/sdk-hal_nordic`
/// `nrfx/mdk/nrf52_erratas.h:6497-6524`).
///
/// Drop this to `M16` if the picture is disturbed. Nothing else depends on the value.
const SPI_FREQUENCY: Frequency = Frequency::M32;

/// Whether the levels of the DC pin are swapped.
///
/// `SpiInterface` drives DC Low for a command and High for data, which is the usual wiring.
/// Confirmed on hardware: the panel accepts the initialization sequence and shows what is drawn,
/// which it would not with the levels the other way round. Flipping this swaps the two levels.
const DC_INVERTED: bool = false;

/// How long the brightness overlay stays fully in view once the brightness has stopped changing.
///
/// Measured from the last change rather than from the first, so that a run of presses keeps the
/// overlay up and the count starts when the adjustment ends.
const BRIGHTNESS_HOLD: Duration = Duration::from_secs(2);

/// Framebuffer of the panel. Too large to build on the stack, so it lives in a static.
static FRAME_BUFFER: StaticCell<[u8; FRAME_BUFFER_LEN]> = StaticCell::new();

/// Whether the panel has shown its first frame.
///
/// Read through [`panel_ready`] by [`crate::backlight`], which owns the backlight and must not
/// light it before there is something to see, because the panel holds noise after reset. It stays
/// `false` forever when the panel fails to come up, which is what keeps a dead panel dark.
///
/// `Relaxed` is enough: the flag publishes no other data, and both sides do nothing but read and
/// write this one bit.
static PANEL_READY: AtomicBool = AtomicBool::new(false);

/// Whether the panel has shown its first frame. See [`PANEL_READY`].
pub fn panel_ready() -> bool {
    PANEL_READY.load(Ordering::Relaxed)
}

/// SPI device of the panel: the SPIM plus its chip select.
type Bus = ExclusiveDevice<Spim<'static>, Output<'static>, Delay>;

/// 4-line serial interface of the panel, wrapped in the workaround of [`spim3_anomaly198`].
type PanelInterface = Spim3Anomaly198Interface<SpiInterface<Bus, PolarityPin>>;

/// Initialized panel together with its framebuffer.
type Panel = FrameBuffer<
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

/// Processor that keeps the panel showing the state of the keyboard.
///
/// RMK's own `DisplayProcessor` is not used: its `RenderContext` carries only the BLE status and
/// cannot tell whether reports are going out over USB or over BLE, which this display has to show.
/// The five events subscribed to here are what [`Status`] is assembled from, and are a subset of
/// the eleven `DisplayProcessor` takes. **`ActionEvent` must not be added to them**, see the
/// documentation of [`crate::backlight`].
///
/// The processor is polled on top of that, because the brightness overlay is an animation and
/// nothing publishes an event per frame. A tick that finds nothing to do costs one comparison,
/// because [`render`](Self::render) returns at once while the screen would come out the way it
/// already is.
///
/// The tick is shorter than a frame that carries the overlay takes to reach the panel, so it never
/// paces the animation: the frames come as fast as the panel takes them. While the overlay
/// moves, the ticker of `polling_loop` is therefore always
/// overdue and `select` takes it ahead of the subscription every time
/// (`embassy-futures-0.1.2/src/select.rs:60-71`), so the events go unread for the length of the
/// slide. That costs freshness and nothing else: all five are published with `publish_event`
/// (`rmk/src/event/mod.rs:200-204`), which never waits and drops the oldest state instead
/// (`embassy-sync-0.7.2/src/pubsub/mod.rs:352-360`), so no publisher is ever held up and the
/// newest state of each is still queued once the slide is over. The ticker catches up in the polls
/// that follow, which draw nothing.
///
/// Polling rather than a `DeadlineProcessor` (`rmk/src/processor/mod.rs:113-150`), which would idle
/// on the subscription and only arm a timer while the overlay moves: `#[register_processor]`
/// expands to `process_loop()` or to `polling_loop()` and never to `run()`
/// (`rmk-macro/src/codegen/registered_processor.rs:60-71`), so `deadline_loop` could only be
/// reached by implementing `PollingProcessor` by hand and overriding `polling_loop`, leaving
/// `interval` and `update` behind as members that are never called. A 30Hz tick on a dongle that
/// runs off USB power is not worth that.
#[processor(subscribe = [ConnectionStatusChangeEvent, LayerChangeEvent, ModifierEvent, PeripheralBatteryEvent, PeripheralConnectedEvent], poll_interval = 33)]
pub struct LcdProcessor {
    /// Panel. `None` when initialization failed, in which case nothing is displayed and the rest
    /// of the firmware keeps running.
    panel: Option<Panel>,
    /// State reported last.
    status: Status,
    /// State the panel currently shows.
    drawn: Option<Status>,
    /// Brightness read from [`crate::backlight`] last, which a poll compares against to notice
    /// that the backlight has been stepped.
    brightness: Brightness,
    /// How far the brightness overlay has come in, from 0 for off screen up to
    /// [`BRIGHTNESS_SLIDE_FRAMES`] for fully in.
    slide: u8,
    /// When the overlay starts sliding back out.
    hold_until: Instant,
}

impl LcdProcessor {
    /// Brings the SPI bus and the panel up, then shows the first frame.
    ///
    /// [`PANEL_READY`] is raised afterwards, and only when the panel came up, which is what lets
    /// [`crate::backlight`] light the backlight without ever showing the noise the panel holds
    /// after reset. When the panel fails to come up it is dropped, the flag stays down and the
    /// rest of the firmware is unaffected.
    pub async fn new(
        spim: Peri<'static, SPI3>,
        irq: impl Binding<<SPI3 as SpimInstance>::Interrupt, SpimInterruptHandler<SPI3>> + 'static,
        sck: Peri<'static, impl GpioPin>,
        mosi: Peri<'static, impl GpioPin>,
        cs: Peri<'static, impl GpioPin>,
        dc: Peri<'static, impl GpioPin>,
        reset: Peri<'static, impl GpioPin>,
    ) -> Self {
        let panel = Self::init_panel(spim, irq, sck, mosi, cs, dc, reset).await;

        let mut processor = Self {
            panel,
            status: Status::new(),
            drawn: None,
            // Taken as the starting point rather than as a change, so that the step the backlight
            // comes up at does not slide the overlay in at boot.
            brightness: backlight::brightness(),
            slide: 0,
            // In the past, so the overlay is not held anywhere.
            hold_until: Instant::from_ticks(0),
        };

        processor.render().await;
        if processor.panel.is_some() {
            PANEL_READY.store(true, Ordering::Relaxed);
        }
        processor
    }

    /// Builds the SPI bus and runs the initialization sequence of the panel.
    ///
    /// The SPIM is [`SPI3`] rather than any instance, because the workaround the bus is wrapped in
    /// applies to SPIM3 alone. See [`spim3_anomaly198`].
    ///
    /// Returns `None` when the bus, the panel or the framebuffer fails to come up, so that the
    /// caller can give up on the display and keep running.
    async fn init_panel(
        spim: Peri<'static, SPI3>,
        irq: impl Binding<<SPI3 as SpimInstance>::Interrupt, SpimInterruptHandler<SPI3>> + 'static,
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

        let interface = Spim3Anomaly198Interface::new(SpiInterface::new(device, dc));

        // The size and the offset are given in the native orientation of the panel, because
        // lcd-async maps them onto the frame memory itself once it knows the rotation.
        let panel = Builder::new(ST7789, interface)
            .reset_pin(reset)
            .display_size(PANEL_WIDTH, PANEL_HEIGHT)
            .display_offset(PANEL_OFFSET_X, PANEL_OFFSET_Y)
            .orientation(Orientation::new().rotate(ROTATION))
            .invert_colors(COLOR_INVERSION)
            .color_order(COLOR_ORDER)
            .init(&mut Delay)
            .await
            .ok()?;

        Some(FrameBuffer::new(panel, Self::frame_buffer()?))
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

    /// Takes in a change of the host connection state.
    async fn on_connection_status_change_event(&mut self, event: ConnectionStatusChangeEvent) {
        self.status.connection = event.0;
        self.render().await;
    }

    /// Takes in a layer change.
    async fn on_layer_change_event(&mut self, event: LayerChangeEvent) {
        self.status.layer = event.0;
        self.render().await;
    }

    /// Takes in a change of the modifiers being held.
    async fn on_modifier_event(&mut self, event: ModifierEvent) {
        self.status.modifiers = event.modifier;
        self.render().await;
    }

    /// Takes in the battery of one half.
    ///
    /// The `id` is the one given to `#[rmk_peripheral]`, so it selects the half. Anything outside
    /// the two halves is dropped rather than shown in the wrong place.
    async fn on_peripheral_battery_event(&mut self, event: PeripheralBatteryEvent) {
        let Some(battery) = self.status.batteries.get_mut(event.id) else {
            return;
        };
        *battery = event.state.0;
        self.render().await;
    }

    /// Takes in a change of the link to one half.
    ///
    /// A half that is not connected has its battery dropped rather than left at the level it last
    /// reported: `PeripheralBatteryEvent` only arrives over that link, so nothing else would ever
    /// clear it and the screen would keep showing a level that is no longer being measured. The
    /// level comes back once the half reconnects and reports again, which
    /// [`crate::battery_resend`] makes it do straight away.
    ///
    /// A drop is noticed no sooner than the link layer gives up on the half, which takes the ten
    /// seconds of the supervision timeout (`rmk/src/split/ble/central.rs:269`) plus the 500ms the
    /// reconnect loop waits before it publishes (same file, `:260`). A half that is switched off
    /// cleanly disconnects and is noticed at once.
    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if event.connected {
            return;
        }
        let Some(battery) = self.status.batteries.get_mut(event.id) else {
            return;
        };
        *battery = BatteryStatus::Unavailable;
        self.render().await;
    }

    /// Moves the brightness overlay one frame.
    ///
    /// The overlay is driven from here rather than from an event because it has to move on its own
    /// once the brightness stops changing. Every poll asks [`crate::backlight`] where the
    /// brightness stands: a value that differs from the one seen last restarts the hold, so a run
    /// of presses keeps the overlay up and [`BRIGHTNESS_HOLD`] is counted from the last of them.
    ///
    /// The slide is stepped one frame per poll towards whichever end the hold asks for, so a
    /// brightness change caught mid-departure brings the overlay back from where it had got to
    /// rather than from off screen. What one frame lasts is not set here: a poll that moves the
    /// overlay redraws and flushes the band of the overlay together with the layer it covers, so
    /// the number of frames is what the length of the slide is chosen with. See
    /// [`BRIGHTNESS_SLIDE_FRAMES`].
    async fn poll(&mut self) {
        let brightness = backlight::brightness();
        let now = Instant::now();
        if brightness != self.brightness {
            self.brightness = brightness;
            self.hold_until = now + BRIGHTNESS_HOLD;
        }

        let target = if now < self.hold_until {
            BRIGHTNESS_SLIDE_FRAMES
        } else {
            0
        };
        if self.slide < target {
            self.slide += 1;
        } else if self.slide > target {
            self.slide -= 1;
        }

        self.status.brightness = (self.slide > 0).then_some(BrightnessOverlay {
            percent: self.brightness.percent,
            slide: self.slide,
        });
        self.render().await;
    }

    /// Redraws and flushes when the screen would come out different.
    ///
    /// Comparing the whole state against what is on the panel is what suppresses the redundant
    /// flushes: modifier and battery events in particular arrive far more often than the picture
    /// changes. That comparison, together with the event channels dropping the states a lagging
    /// subscriber did not keep up with, is what bounds a burst; what a redraw then costs is set by
    /// the bands that changed rather than by the size of the panel.
    ///
    /// [`status_screen::draw`] is given what the panel already shows, so that it can tell which
    /// bands those are, and answers with the rows it changed. Those rows go out as they come, one
    /// transfer each: they are disjoint and ordered from the top down, and there are at most three
    /// of them because the four bands of the screen only ever fall into that many separate runs.
    async fn render(&mut self) {
        let status = self.status;
        if self.drawn == Some(status) || self.panel.is_none() {
            return;
        }
        let drawn = self.drawn;

        if let Some(panel) = self.panel.as_mut() {
            let rows = status_screen::draw(panel, &status, drawn);
            for band in rows.bands() {
                let (top, height) = band.row_range();
                panel.flush(top, height).await;
            }
        }

        self.drawn = Some(status);
    }
}
