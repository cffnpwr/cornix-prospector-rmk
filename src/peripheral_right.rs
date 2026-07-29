#![no_main]
#![no_std]

use rmk::macros::rmk_peripheral;

#[rmk_peripheral(id = 1)]
mod keyboard_peripheral {
    use cornix_prospector_rmk::indicator::{IndicatorProcessor, RIGHT_ROLES};

    /// Indicator LEDs of the right half. WS2812 data is P0_13, the LED supply enable is P0_24 and
    /// charge detection is P1_11.
    #[register_processor(poll)]
    fn indicator() -> IndicatorProcessor {
        IndicatorProcessor::new(p.PWM0, p.P0_13, p.P0_24, p.P1_11, RIGHT_ROLES)
    }
}
