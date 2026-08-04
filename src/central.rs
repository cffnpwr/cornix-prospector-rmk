#![no_main]
#![no_std]

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    use cornix_prospector_rmk::backlight::BacklightProcessor;
    use cornix_prospector_rmk::held_keys::HeldKeysProcessor;
    use cornix_prospector_rmk::lcd::LcdProcessor;
    use embassy_nrf::peripherals::SPI3;
    use embassy_nrf::spim::InterruptHandler as SpimInterruptHandler;

    // SPIM3 is not part of the interrupts RMK binds, so it is added to the same `Irqs` struct
    // instead of a second `bind_interrupts!`, which keeps a double binding impossible.
    add_interrupt!(SPIM3 => SpimInterruptHandler<SPI3>;);

    /// Backlight of the LCD on P1_11, driven by PWM0.
    ///
    /// Kept ahead of `lcd` on purpose: the initializers run in the order the functions appear
    /// here, and this one drives the backlight pin dark, which has to happen before the panel
    /// initialization starts.
    #[register_processor(poll)]
    fn backlight() -> BacklightProcessor {
        BacklightProcessor::new(p.PWM0, p.P1_11)
    }

    /// ST7789 LCD of the dongle. SPI is SCK P1_13 and MOSI P1_15 with CS P1_14, DC is P1_12 and
    /// reset is P0_29.
    ///
    /// Polled as well as event driven: the brightness overlay is an animation and nothing
    /// publishes an event per frame.
    #[register_processor(poll)]
    fn lcd() -> LcdProcessor {
        LcdProcessor::new(p.SPI3, Irqs, p.P1_13, p.P1_15, p.P1_14, p.P1_12, p.P0_29).await
    }

    /// Releases the keys a half was still holding when its link drops.
    #[register_processor(event)]
    fn held_keys() -> HeldKeysProcessor {
        HeldKeysProcessor::new()
    }
}
