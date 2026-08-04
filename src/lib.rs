//! Code shared by the three firmware binaries.
//!
//! Each binary has its own crate root, so the types they use are placed in this library. Which
//! binary uses which module differs: [`battery_resend`], [`indicator`] and [`ws2812`] belong to
//! the left and right peripherals, while [`backlight`], [`lcd`] and [`status_screen`] drive the
//! LCD of the central dongle and [`held_keys`] lets go of the keys of a half that disconnects.

#![no_std]

pub mod backlight;
pub mod battery_resend;
pub mod held_keys;
pub mod indicator;
pub mod lcd;
pub mod status_screen;
pub mod ws2812;
