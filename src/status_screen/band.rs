//! The rows of the status screen a redraw touches.
//!
//! A [`Band`] is the range of rows one part of the layout occupies, and [`DirtyRows`] is what a
//! redraw comes to: the bands it changed, merged into as few of them as they allow. Neither knows
//! what is drawn in a band, only where it starts and how tall it is, so this is what changes when
//! the way a redraw reaches the panel changes rather than when the screen changes what it shows. It
//! is also the shape [`crate::lcd`] reads a redraw through.

use super::PART_COUNT;

/// A full-width range of rows of the screen.
///
/// Bands are full width because the framebuffer is row major: a range of whole rows is one
/// contiguous slice and so one transfer, while a narrower rectangle is one transfer per row.
#[derive(Clone, Copy)]
pub struct Band {
    /// Topmost row (px) of the band.
    pub(super) top: i32,
    /// Number of rows (px) the band covers.
    pub(super) height: i32,
}

impl Band {
    /// A band of no rows at all.
    const EMPTY: Self = Self { top: 0, height: 0 };

    /// The row (px) just past the bottom of the band.
    const fn bottom(self) -> i32 {
        self.top + self.height
    }

    /// Whether the two bands share a row.
    pub(super) const fn intersects(self, other: Self) -> bool {
        self.top < other.bottom() && other.top < self.bottom()
    }

    /// The smallest band that covers both.
    pub(super) fn union(self, other: Self) -> Self {
        let top = self.top.min(other.top);
        Self {
            top,
            height: self.bottom().max(other.bottom()) - top,
        }
    }

    /// The band as a row range of the framebuffer: the first row and the number of rows.
    ///
    /// A negative value comes out as zero, which the layout of the screen never produces.
    #[must_use]
    pub fn row_range(self) -> (usize, usize) {
        let top = usize::try_from(self.top).unwrap_or(0);
        let height = usize::try_from(self.height).unwrap_or(0);
        (top, height)
    }
}

/// The rows a redraw changed, as disjoint bands ordered from the top of the screen down.
///
/// Bands that overlap or that merely touch are merged, because sending the two together costs less
/// than sending them apart: a transfer carries a row in about 0.14ms (560 bytes at the 32Mbit/s of
/// `crate::lcd`) against a fixed cost of a few hundred microseconds per transfer.
pub struct DirtyRows {
    /// The bands, of which the first [`Self::len`] are in use.
    bands: [Band; PART_COUNT],
    /// How many of [`Self::bands`] are in use.
    len: usize,
}

impl DirtyRows {
    /// No rows at all.
    pub(super) const fn new() -> Self {
        Self {
            bands: [Band::EMPTY; PART_COUNT],
            len: 0,
        }
    }

    /// Adds `band`, merging it into the band before it when the two overlap or touch.
    ///
    /// Bands are added from the top of the screen down, so only the one added last can reach
    /// `band`.
    pub(super) fn push(&mut self, band: Band) {
        if let Some(last) = self.bands[..self.len].last_mut()
            && band.top <= last.bottom()
        {
            *last = last.union(band);
            return;
        }
        if let Some(slot) = self.bands.get_mut(self.len) {
            *slot = band;
            self.len += 1;
        }
    }

    /// The bands, ordered from the top of the screen down.
    #[must_use]
    pub fn bands(&self) -> &[Band] {
        &self.bands[..self.len]
    }
}
