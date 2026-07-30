#![no_main]
#![no_std]

use rmk::macros::rmk_peripheral;

#[rmk_peripheral(id = 0)]
mod keyboard_peripheral {
    use cornix_prospector_rmk::indicator::{IndicatorProcessor, LEFT_ROLES};

    /// Indicator LEDs of the left half. WS2812 data is P0_24, the LED supply enable is P0_13 and
    /// charge detection is P1_09.
    #[register_processor(poll)]
    fn indicator() -> IndicatorProcessor {
        IndicatorProcessor::new(p.PWM0, p.P0_24, p.P0_13, p.P1_09, LEFT_ROLES)
    }
}
