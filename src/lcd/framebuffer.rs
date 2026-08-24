//! Framebuffer-backed [`DrawTarget`] for an [`lcd_async::Display`], row-filling in place of
//! `rmk::display::drivers::lcd_async::LcdAsyncDisplay`.
//!
//! # Window rather than whole screen
//!
//! The buffer holds `ROWS` rows rather than all `H` of them, because a whole `Rgb565` screen of
//! this panel is 134,400 bytes and the 255KiB of RAM has to leave room for the stack as well. The
//! rows it holds are the window, which [`FrameBuffer::set_window`] moves; the [`DrawTarget`] keeps
//! taking coordinates of the whole screen and drops whatever falls outside the window, so the
//! caller draws the same picture whichever rows are currently held and only decides how many
//! passes to make. Anything taller than the window is therefore drawn once per pass rather than
//! being clipped away, which is what makes `ROWS` a matter of RAM against redraw cost and never of
//! correctness.
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
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
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
/// - `BUF` — framebuffer storage of length `W * ROWS * 2`.
///   Typically `&'static mut [u8; W * ROWS * 2]`.
/// - `W` / `H` — display resolution in pixels.
/// - `ROWS` — rows of the display the buffer holds at once. See the module documentation.
///
/// # Lifecycle
///
/// The wrapped [`Display`] must already be initialized by [`lcd_async::Builder::init`] before
/// being passed to [`new`](Self::new).
pub(super) struct FrameBuffer<DI, MOD, RST, BUF, const W: usize, const H: usize, const ROWS: usize>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    display: Display<DI, MOD, RST>,
    buffer: BUF,
    /// Topmost row of the display the buffer currently holds. See [`set_window`](Self::set_window).
    window_top: usize,
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize, const ROWS: usize>
    FrameBuffer<DI, MOD, RST, BUF, W, H, ROWS>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    /// Wraps an already-initialized `display` together with its framebuffer storage.
    ///
    /// `buffer` must be exactly `W * ROWS * 2` bytes long (Rgb565 is 2 bytes per pixel). The window
    /// starts at the top of the display.
    pub(super) fn new(display: Display<DI, MOD, RST>, buffer: BUF) -> Self {
        debug_assert_eq!(
            buffer.as_ref().len(),
            W * ROWS * 2,
            "framebuffer length must equal W * ROWS * 2 (Rgb565)",
        );
        Self {
            display,
            buffer,
            window_top: 0,
        }
    }

    /// Moves the window so that the buffer stands for the `ROWS` rows of the display below `top`.
    ///
    /// The buffer keeps whatever it held, because every pass draws over the rows it is about to
    /// send: a window is set, drawn into and flushed before the next one is set, so carrying stale
    /// pixels across is what saves clearing rows that are about to be overwritten anyway.
    pub(super) const fn set_window(&mut self, top: usize) {
        self.window_top = top;
    }

    /// The rows of the display the buffer currently holds, as an area in display coordinates.
    ///
    /// Cut off at the bottom edge, so a window hanging past it stands only for the rows that exist.
    /// This is what every [`DrawTarget`] method clips against, and it takes the place of
    /// `bounding_box()`: it is the bounding box of the display intersected with the window.
    fn window(&self) -> Rectangle {
        let top = self.window_top.min(H);
        let height = ROWS.min(H - top);
        Rectangle::new(
            Point::new(0, i32::try_from(top).unwrap_or(i32::MAX)),
            Size::new(
                u32::try_from(W).unwrap_or(u32::MAX),
                u32::try_from(height).unwrap_or(u32::MAX),
            ),
        )
    }

    /// Sends the first `height` rows of the window to the panel.
    ///
    /// The band is always the full width of the panel, which is what makes it one transfer: the
    /// framebuffer is row major, so a range of whole rows is a contiguous slice and goes out in a
    /// single `show_raw_data` (`lcd-async-0.1.3/src/lib.rs:150-168`), while a narrower rectangle
    /// would take one transfer per row.
    ///
    /// A band that reaches past the bottom edge or past the window is cut off there and an empty
    /// band sends nothing, which also keeps `show_raw_data` away from the `y + height - 1` it
    /// computes for the address window.
    ///
    /// An inherent method rather than `rmk::display::DisplayDriver::flush` because nothing in this
    /// firmware dispatches over that trait; the only two things it would add are `init`, which has
    /// no work to do since the display is already initialized before being wrapped, and an import
    /// of the trait at every call site.
    pub(super) async fn flush(&mut self, height: usize) {
        let top = self.window_top.min(H);
        let height = height.min(ROWS).min(H - top);
        if height == 0 {
            return;
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "W and H are the panel's own pixel dimensions, always well under u16::MAX, and top and height are clamped to H"
        )]
        let (width, first_row, rows) = (W as u16, top as u16, height as u16);
        let band = &self.buffer.as_ref()[..height * W * 2];
        let _ = self
            .display
            .show_raw_data(0, first_row, width, rows, band)
            .await;
    }
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize, const ROWS: usize> OriginDimensions
    for FrameBuffer<DI, MOD, RST, BUF, W, H, ROWS>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    /// The whole display, not the window.
    ///
    /// What is drawn is laid out against the display and clipped to the window afterwards, so the
    /// size a draw target reports has to stay the same however the window moves.
    fn size(&self) -> Size {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "W and H are the panel's own pixel dimensions, always well under u32::MAX"
        )]
        Size::new(W as u32, H as u32)
    }
}

impl<DI, MOD, RST, BUF, const W: usize, const H: usize, const ROWS: usize> DrawTarget
    for FrameBuffer<DI, MOD, RST, BUF, W, H, ROWS>
where
    DI: Interface<Word = u8>,
    MOD: Model<ColorFormat = Rgb565>,
    RST: OutputPin,
    BUF: AsMut<[u8]> + AsRef<[u8]>,
{
    type Color = Rgb565;
    type Error = Infallible;

    /// Draws pixels one at a time, discarding whichever fall outside the window.
    ///
    /// Glyphs and bitmaps go through here (`crate::status_screen::icons::draw_bitmap`), which is
    /// the pixel-by-pixel path `RawFrameBuf::draw_iter` also uses
    /// (`lcd-async-0.1.3/src/raw_framebuf.rs:221-245`); this mirrors it, including silently
    /// dropping out-of-bounds pixels rather than erroring or panicking. Pixels above or below the
    /// window are dropped the same way, which is what lets the caller draw the whole screen into
    /// one window at a time.
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let window = self.window();
        let window_top = self.window_top;
        let buffer = self.buffer.as_mut();

        for Pixel(coord, color) in pixels {
            if window.contains(coord) {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "window() starts at x 0 and at a non-negative row, so contains(coord) being true means coord.x and coord.y are >= 0"
                )]
                let (x, y) = (coord.x as usize, coord.y as usize);
                let index = ((y - window_top) * W + x) * 2;
                buffer[index..index + 2].copy_from_slice(&color.to_be_bytes());
            }
        }
        Ok(())
    }

    /// Fills `area`, clipped to the window, one row at a time.
    ///
    /// This is the method the whole module exists for: unlike `RawFrameBuf::fill_solid`, every
    /// row is written with `fill` (when the two color bytes match) or `chunks_exact_mut` plus
    /// `copy_from_slice` (when they don't), rather than a multiply-and-bounds-check per pixel.
    ///
    /// Clipping is what `RawFrameBuf::fill_solid` does against the whole screen
    /// (`lcd-async-0.1.3/src/raw_framebuf.rs:272`), narrowed to the window: a rectangle that
    /// reaches past an edge, or past the rows currently held, is cut off there rather than
    /// wrapping or erroring.
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let drawable_area = area.intersection(&self.window());
        if drawable_area.is_zero_sized() {
            return Ok(());
        }

        #[expect(
            clippy::cast_sign_loss,
            reason = "drawable_area is clipped to window(), which starts at x 0 and at a non-negative row, so its top_left.x and top_left.y are >= 0"
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
        let window_top = self.window_top;
        let buffer = self.buffer.as_mut();

        for row in 0..height {
            let row_start = ((top - window_top + row) * W + left) * 2;
            let row_slice = &mut buffer[row_start..row_start + width * 2];
            if same_bytes {
                row_slice.fill(color_bytes[0]);
            } else {
                for pixel in row_slice.as_chunks_mut::<2>().0 {
                    *pixel = color_bytes;
                }
            }
        }
        Ok(())
    }

    /// Fills the window in one pass.
    ///
    /// Clearing the screen and clearing the window are the same call here, because the buffer
    /// stands for whichever rows the window holds and no others: a caller that clears the screen
    /// once per window ends up having cleared all of it.
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
            for pixel in buffer.as_chunks_mut::<2>().0 {
                *pixel = color_bytes;
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
