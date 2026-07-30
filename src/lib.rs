//! Code shared by the left and right peripheral firmware.
//!
//! Each binary has its own crate root, so types used from both are placed in this library.

#![no_std]

pub mod indicator;
pub mod ws2812;
