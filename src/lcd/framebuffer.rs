//! Framebuffer-backed [`DrawTarget`] for an [`lcd_async::Display`], row-filling in place of
//! `rmk::display::drivers::lcd_async::LcdAsyncDisplay`.
//!
//! # Why not `LcdAsyncDisplay`
//!
//! `LcdAsyncDisplay` wraps `lcd_async::raw_framebuf::RawFrameBuf` and forwards every `DrawTarget`
//! method to it (`rmk/src/display/drivers/lcd_async.rs:125-147`). `RawFrameBuf::fill_solid`
//! (`lcd-async-0.1.3/src/raw_framebuf.rs:271-292`) walks `drawable_area.points()` and writes one
//! pixel at a time, each write paying a multiply for the byte offset and a bounds check, with no
//! per-row `memcpy`. Measured on this hardware, that path costs 4.3µs per pixel versus 0.033µs per
//! pixel for `RawFrameBuf::clear`'s `memset` (`:247-269`) — about 130 times slower. Every
//! `RoundedRectangle` fill goes through `fill_solid` once per scanline
//! (`embedded-graphics-0.8.1/src/primitives/rounded_rectangle/styled.rs:133-136` and
//! `src/primitives/common/scanline.rs:140-154`), so this is not a rare path.
//!
//! `LcdAsyncDisplay` keeps its framebuffer in a private field (`rmk/src/display/drivers/lcd_async.rs:57-58`)
//! and only exposes the `DrawTarget` methods `RawFrameBuf` already implements, so there is no way
//! to keep the type and swap in a faster `fill_solid`. [`FrameBuffer`] holds the same two pieces —
//! an initialized [`Display`] and the framebuffer bytes — and implements `DrawTarget` itself so
//! that `fill_solid` can write whole rows.

use core::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Dimensions as _, OriginDimensions, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::ToBytes as _;
use embedded_graphics::primitives::Rectangle;
use embedded_hal::digital::OutputPin;
use rmk::display::lcd_async::Display;
use rmk::display::lcd_async::interface::Interface;
use rmk::display::lcd_async::models::Model;

/// Bridges an [`lcd_async::Display`] and a software framebuffer into a fast [`DrawTarget`].
///
/// See the module documentation for why this exists instead of
/// `rmk::display::drivers::lcd_async::LcdAsyncDisplay`.
///
/// # Generics
///
/// - `DI` — bus interface (`Word = u8`).
/// - `MOD` — chip model from [`lcd_async::models`] using the `Rgb565` color format.
/// - `RST` — reset pin.
/// - `BUF` — framebuffer storage of length `W * H * 2`. Typically `&'static mut [u8; W * H * 2]`.
/// - `W` / `H` — display resolution in pixels.
///
/// # Lifecycle
///
/// The wrapped [`Display`] must already be initialized by [`lcd_async::Builder::init`] before
/// being passed to [`new`](Self::new).
pub(super) struct FrameBuffer<DI, MOD, RST, BUF, const W: usize, const H: usize>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    display: Display<DI, MOD, RST>,
    buffer: BUF,
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize> FrameBuffer<DI, MOD, RST, BUF, W, H>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    /// Wraps an already-initialized `display` together with its framebuffer storage.
    ///
    /// `buffer` must be exactly `W * H * 2` bytes long (Rgb565 is 2 bytes per pixel).
    pub(super) fn new(display: Display<DI, MOD, RST>, buffer: BUF) -> Self {
        debug_assert_eq!(
            buffer.as_ref().len(),
            W * H * 2,
            "framebuffer length must equal W * H * 2 (Rgb565)",
        );
        Self { display, buffer }
    }

    /// Sends `height` rows of the framebuffer, starting at row `top`, to the panel.
    ///
    /// The band is always the full width of the panel, which is what makes it one transfer: the
    /// framebuffer is row major, so a range of whole rows is a contiguous slice and goes out in a
    /// single `show_raw_data` (`lcd-async-0.1.3/src/lib.rs:150-168`), while a narrower rectangle
    /// would take one transfer per row. The whole frame is the band that covers every row, so
    /// there is no separate full-frame path.
    ///
    /// A band that reaches past the bottom edge is cut off there and an empty band sends nothing,
    /// which also keeps `show_raw_data` away from the `y + height - 1` it computes for the address
    /// window.
    ///
    /// An inherent method rather than `rmk::display::DisplayDriver::flush` because nothing in this
    /// firmware dispatches over that trait; the only two things it would add are `init`, which has
    /// no work to do since the display is already initialized before being wrapped, and an import
    /// of the trait at every call site.
    pub(super) async fn flush(&mut self, top: usize, height: usize) {
        let top = top.min(H);
        let height = height.min(H - top);
        if height == 0 {
            return;
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "W and H are the panel's own pixel dimensions, always well under u16::MAX, and top and height are clamped to H"
        )]
        let (width, first_row, rows) = (W as u16, top as u16, height as u16);
        let start = top * W * 2;
        let band = &self.buffer.as_ref()[start..start + height * W * 2];
        let _ = self
            .display
            .show_raw_data(0, first_row, width, rows, band)
            .await;
    }
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize> OriginDimensions
    for FrameBuffer<DI, MOD, RST, BUF, W, H>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    fn size(&self) -> Size {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "W and H are the panel's own pixel dimensions, always well under u32::MAX"
        )]
        Size::new(W as u32, H as u32)
    }
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize> DrawTarget
    for FrameBuffer<DI, MOD, RST, BUF, W, H>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    type Color = Rgb565;
    type Error = Infallible;

    /// Draws pixels one at a time, discarding whichever fall outside the framebuffer.
    ///
    /// Glyphs and bitmaps go through here (`crate::status_screen::icons::draw_bitmap`), which is
    /// the pixel-by-pixel path `RawFrameBuf::draw_iter` also uses
    /// (`lcd-async-0.1.3/src/raw_framebuf.rs:221-245`); this mirrors it, including silently
    /// dropping out-of-bounds pixels rather than erroring or panicking.
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bounding_box = self.bounding_box();
        let buffer = self.buffer.as_mut();

        for Pixel(coord, color) in pixels {
            if bounding_box.contains(coord) {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "bounding_box() starts at the origin, so contains(coord) being true means coord.x and coord.y are >= 0"
                )]
                let (x, y) = (coord.x as usize, coord.y as usize);
                let index = (y * W + x) * 2;
                buffer[index..index + 2].copy_from_slice(&color.to_be_bytes());
            }
        }
        Ok(())
    }

    /// Fills `area`, clipped to the framebuffer, one row at a time.
    ///
    /// This is the method the whole module exists for: unlike `RawFrameBuf::fill_solid`, every
    /// row is written with `fill` (when the two color bytes match) or `chunks_exact_mut` plus
    /// `copy_from_slice` (when they don't), rather than a multiply-and-bounds-check per pixel.
    ///
    /// Clipping matches `RawFrameBuf::fill_solid`: `area.intersection(&self.bounding_box())` is
    /// the same call it makes (`lcd-async-0.1.3/src/raw_framebuf.rs:272`), so a rectangle that
    /// reaches past an edge is cut off at that edge rather than wrapping or erroring, identically
    /// to before.
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let drawable_area = area.intersection(&self.bounding_box());
        if drawable_area.is_zero_sized() {
            return Ok(());
        }

        #[expect(
            clippy::cast_sign_loss,
            reason = "drawable_area is clipped to bounding_box(), whose top_left is the origin, so its top_left.x and top_left.y are >= 0"
        )]
        let (left, top) = (
            drawable_area.top_left.x as usize,
            drawable_area.top_left.y as usize,
        );
        let (width, height) = (
            drawable_area.size.width as usize,
            drawable_area.size.height as usize,
        );

        let color_bytes = color.to_be_bytes();
        let same_bytes = color_bytes[0] == color_bytes[1];
        let buffer = self.buffer.as_mut();

        for row in 0..height {
            let row_start = ((top + row) * W + left) * 2;
            let row_slice = &mut buffer[row_start..row_start + width * 2];
            if same_bytes {
                row_slice.fill(color_bytes[0]);
            } else {
                for pixel in row_slice.chunks_exact_mut(2) {
                    pixel.copy_from_slice(&color_bytes);
                }
            }
        }
        Ok(())
    }

    /// Fills the whole framebuffer in one pass.
    ///
    /// The default would route this to [`fill_solid`](Self::fill_solid) and pay for one slice per
    /// row; the whole buffer is contiguous, so it is one `fill` instead. Measured on hardware:
    /// 2,200µs this way against 3,200µs a row at a time.
    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let color_bytes = color.to_be_bytes();
        let buffer = self.buffer.as_mut();

        if color_bytes[0] == color_bytes[1] {
            buffer.fill(color_bytes[0]);
        } else {
            for pixel in buffer.chunks_exact_mut(2) {
                pixel.copy_from_slice(&color_bytes);
            }
        }
        Ok(())
    }

    // `fill_contiguous` is not overridden: `RawFrameBuf` never implemented it either
    // (`lcd-async-0.1.3/src/raw_framebuf.rs` has no `fn fill_contiguous`), so the trait's default,
    // which delegates to `draw_iter`, is what every draw already went through before this change.
    // Nothing measured here showed it on the slow path the module doc describes, and adding it
    // would be an improvement past what this task asks for.
}
