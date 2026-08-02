//! Workaround for nRF52840 Anomaly 198, which corrupts what SPIM3 transmits.
//!
//! The anomaly ("SPIM: SPIM3 transmit data might be corrupted") corrupts the bytes SPIM3 puts on
//! the wire when the CPU accesses the RAM AHB slave that holds the transmit buffer in the same
//! clock cycle as `EasyDMA` fetches from it. Nothing is reported, the data is simply wrong. It
//! applies to every IC revision from "Engineering B" up to Revision 3, the most recent one
//! (`zephyrproject-rtos/zephyr` `soc/nordic/nrf52/Kconfig:107-114`), so this board is affected. It
//! is specific to SPIM3, which is why [`crate::lcd`] hands this module the `SPI3` bus alone.
//!
//! The errata gives the workaround in two halves. This module covers the first, which is telling
//! the hardware which RAM AHB slaves the transmit buffer occupies. The second, that the CPU must
//! not write to the transmit buffer while the SPIM is transmitting, holds without anything added
//! here: the framebuffer is touched by [`crate::lcd::LcdProcessor`] alone, which finishes drawing
//! before it awaits the flush and draws again only once that flush has returned.
//!
//! Nothing in the first half can be explained. Neither the errata nor the product specification
//! documents [`ANOMALY_198_REGISTER`], so why writing it helps cannot be stated. What this module
//! does is reproduce the vendor driver: `NordicSemiconductor/nrfx` `drivers/src/nrfx_spim.c:62-97`
//! (`anomaly_198_enable` and `anomaly_198_disable`) reads the register, writes the mask of the
//! transmit buffer, and writes the value it read back once the transfer is over.
//!
//! This module exists because embassy-nrf 0.11 carries no workaround for this anomaly: the only
//! errata handling in its `spim.rs` is `_nrf52832_anomaly_109`. Once a version of embassy-nrf
//! handles it, this file and the one place [`crate::lcd`] wraps its panel interface in
//! [`Spim3Anomaly198Interface`] can both be deleted, with nothing else to unpick.

use core::ptr::{read_volatile, with_exposed_provenance_mut, write_volatile};

use rmk::display::lcd_async::interface::{Interface, InterfaceKind};

/// Address of the undocumented register the Nordic driver writes to work around Anomaly 198. The
/// value written is a mask of the RAM AHB slaves the transmit buffer occupies.
const ANOMALY_198_REGISTER: usize = 0x4000_0E00;

/// Size (bytes) of the RAM block one bit of the [`ANOMALY_198_REGISTER`] mask stands for.
const RAM_BLOCK_SIZE: usize = 0x2000;

/// Shift matching [`RAM_BLOCK_SIZE`], which turns an address into a bit index.
const RAM_BLOCK_SHIFT: usize = 13;

/// First address the vendor computation folds into a single block flag.
const RAM_TOP_BLOCK: usize = 0x2001_0000;

/// Address the vendor computation stops walking blocks at.
const RAM_BLOCK_LIMIT: usize = 0x2001_2000;

/// Lowest RAM address, as the `EasyDMA` check of embassy-nrf reads it
/// (`embassy-nrf-0.11.0/src/util.rs:3-14`).
const RAM_LOWER: usize = 0x2000_0000;

/// One past the highest RAM address, as the `EasyDMA` check of embassy-nrf reads it.
const RAM_UPPER: usize = 0x3000_0000;

/// Mask naming every RAM AHB slave, which is what the vendor computation yields for a buffer
/// reaching from [`RAM_LOWER`] past [`RAM_BLOCK_LIMIT`].
const ALL_RAM_BLOCKS: u32 = 0x1FF;

/// Mask of the RAM AHB slaves `buffer` occupies, computed the way `anomaly_198_enable` computes it
/// (`NordicSemiconductor/nrfx` `drivers/src/nrfx_spim.c:62-97`).
///
/// The addresses are not assumed: the framebuffer and the small buffers holding commands and their
/// arguments end up wherever the linker and the executor put them, and this firmware has them on
/// both sides of [`RAM_TOP_BLOCK`].
///
/// A buffer outside RAM is not what `EasyDMA` reads: embassy-nrf copies it into a buffer of its own
/// first (`embassy-nrf-0.11.0/src/spim.rs:470-480`), and that copy cannot be reached from here, so
/// every RAM AHB slave is named instead.
fn occupied_ram_blocks(buffer: &[u8]) -> u32 {
    if buffer.is_empty() {
        return 0;
    }

    let start = buffer.as_ptr().addr();
    let Some(end) = start.checked_add(buffer.len()) else {
        return ALL_RAM_BLOCKS;
    };
    if start < RAM_LOWER || end >= RAM_UPPER {
        return ALL_RAM_BLOCKS;
    }

    let mut block = start & !(RAM_BLOCK_SIZE - 1);
    if block >= RAM_TOP_BLOCK {
        return 1 << ((RAM_TOP_BLOCK >> RAM_BLOCK_SHIFT) & 0xFFFF);
    }

    let mut blocks: u32 = 0;
    let mut flag: u32 = 1 << ((block >> RAM_BLOCK_SHIFT) & 0xFFFF);
    loop {
        blocks |= flag;
        flag <<= 1;
        block += RAM_BLOCK_SIZE;
        if block >= end || block >= RAM_BLOCK_LIMIT {
            return blocks;
        }
    }
}

/// Keeps a set of RAM AHB slaves named in [`ANOMALY_198_REGISTER`] for as long as it is alive.
///
/// Restoring on drop is what makes the register survive a transfer whose future is dropped part of
/// the way through, which is where an early return would leave it changed.
struct Anomaly198Guard {
    /// Value the register held before this guard changed it.
    preserved: u32,
}

impl Anomaly198Guard {
    /// Names `blocks` in the register, keeping the value it held. An empty mask leaves the
    /// register alone, matching what the vendor driver does for an empty buffer.
    fn enable(blocks: u32) -> Self {
        let register = with_exposed_provenance_mut::<u32>(ANOMALY_198_REGISTER);
        // SAFETY: `ANOMALY_198_REGISTER` is a word aligned address inside the peripheral region
        // of the address map, which is mapped for the whole life of the firmware and never
        // borrowed by Rust code, so no reference can alias it. Reads and writes are volatile
        // because the value is hardware state. `LcdProcessor` is the only owner of the SPIM3 bus
        // and holds `&mut self` across the transfer, so no second guard can be alive at once.
        let preserved = unsafe { read_volatile(register) };
        if blocks != 0 {
            // SAFETY: as above.
            unsafe { write_volatile(register, blocks) };
        }
        Self { preserved }
    }
}

impl Drop for Anomaly198Guard {
    fn drop(&mut self) {
        let register = with_exposed_provenance_mut::<u32>(ANOMALY_198_REGISTER);
        // SAFETY: as in `Anomaly198Guard::enable`.
        unsafe { write_volatile(register, self.preserved) };
    }
}

/// [`Interface`] that keeps the Anomaly 198 workaround applied around every transfer of the one it
/// wraps. See the module documentation for the anomaly and the sources.
pub(super) struct Spim3Anomaly198Interface<I> {
    /// Interface every transfer is handed to.
    inner: I,
}

impl<I> Spim3Anomaly198Interface<I> {
    /// Wraps `inner`. Only meaningful for an interface driving SPIM3, since the anomaly is
    /// specific to that instance.
    pub(super) const fn new(inner: I) -> Self {
        Self { inner }
    }
}

impl<I: Interface<Word = u8>> Interface for Spim3Anomaly198Interface<I> {
    type Word = u8;
    type Error = I::Error;

    const KIND: InterfaceKind = I::KIND;

    /// Sends a command and its arguments with every RAM AHB slave named.
    ///
    /// The wrapped interface sends the command byte out of a buffer it builds itself
    /// (`lcd-async-0.1.3/src/interface/spi.rs`, `SpiInterface::send_command`), whose address
    /// cannot be reached from here, so the mask cannot be narrowed to the slaves of `args`.
    async fn send_command(&mut self, command: u8, args: &[u8]) -> Result<(), Self::Error> {
        let _guard = Anomaly198Guard::enable(ALL_RAM_BLOCKS);
        self.inner.send_command(command, args).await
    }

    /// Sends pixel data with the RAM AHB slaves of `data` named. This is the transfer that carries
    /// the framebuffer, and the one that holds the bus for most of the time it is busy.
    async fn send_data_slice(&mut self, data: &[Self::Word]) -> Result<(), Self::Error> {
        let _guard = Anomaly198Guard::enable(occupied_ram_blocks(data));
        self.inner.send_data_slice(data).await
    }
}
