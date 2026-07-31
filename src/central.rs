#![no_main]
#![no_std]

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    use cornix_prospector_rmk::lcd::LcdProcessor;
    use embassy_nrf::peripherals::SPI3;
    use embassy_nrf::spim::InterruptHandler as SpimInterruptHandler;

    // SPIM3 is not part of the interrupts RMK binds, so it is added to the same `Irqs` struct
    // instead of a second `bind_interrupts!`, which keeps a double binding impossible.
    add_interrupt!(SPIM3 => SpimInterruptHandler<SPI3>;);

    /// ST7789 LCD of the dongle. SPI is SCK P1_13 and MOSI P1_15 with CS P1_14, DC is P1_12,
    /// reset is P0_29 and the backlight is P1_11.
    #[register_processor(event)]
    fn lcd() -> LcdProcessor {
        LcdProcessor::new(
            p.SPI3, Irqs, p.P1_13, p.P1_15, p.P1_14, p.P1_12, p.P0_29, p.PWM0, p.P1_11,
        )
        .await
    }
}
