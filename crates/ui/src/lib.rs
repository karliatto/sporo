//! The screens, and the look they share.
//!
//! Kept apart from the firmware for the same reason [`sporo_core`] is: a screen
//! is generic over its [`DrawTarget`](embedded_graphics::draw_target::DrawTarget)
//! and reads its geometry from `bounding_box`, so nothing here needs a chip and
//! all of it can be drawn into a buffer on the host. Whether a line fits on the
//! panel is then a test rather than something to squint at over a serial cable —
//! see `tests/layout.rs`, and [`best_fit_font`] for the failure it guards.
//!
//! Colours are fixed to [`Rgb565`], matching the panel this is built for. A
//! second panel with a different colour type would mean threading a palette
//! through every screen; there is one panel, so there is one palette.

// `std` only for the test harness; nothing outside `#[cfg(test)]` may use it.
#![cfg_attr(not(test), no_std)]

mod home;
mod word;
mod wordlist;

pub use home::show_home_screen;
pub use word::show_word_screen;
pub use wordlist::show_wordlist_screen;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use u8g2_fonts::{fonts, types::VerticalPosition, Content, FontRenderer};

/// Cleared to before anything is drawn, and what the firmware paints the panel
/// with on power-up so the ST7789's garbage never reaches the user.
pub const BACKGROUND_COLOR: Rgb565 = Rgb565::BLACK;

pub(crate) const TEXT_COLOR: Rgb565 = Rgb565::WHITE;
pub(crate) const ACCENT_COLOR: Rgb565 = Rgb565::CYAN;
pub(crate) const WARNING_COLOR: Rgb565 = Rgb565::CSS_ORANGE;

/// Letters that would spell something no word starts with. Dim rather than
/// hidden: the alphabet stays a fixed strip, so the letters that are available
/// do not shuffle sideways as the word grows.
pub(crate) const DIM_COLOR: Rgb565 = Rgb565::new(8, 16, 8);

/// Margin kept clear on each side when sizing text to the screen.
pub(crate) const HORIZONTAL_MARGIN: u32 = 8;

/// Candidate faces for the brand mark and the word being spelled, largest first;
/// [`best_fit_font`] picks the biggest that fits. LogiSoSo is a wide geometric
/// sans — deliberately contrasting with the monospace used for body text, the
/// same split the C++ screen makes between its `brandFonts` and `monospaceFonts`
/// lists.
pub(crate) static LOGO_FONTS: [FontRenderer; 4] = [
    FontRenderer::new::<fonts::u8g2_font_logisoso42_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso32_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso24_tr>(),
    FontRenderer::new::<fonts::u8g2_font_logisoso16_tr>(),
];

/// Courier, standing in for the Courier Prime Code the C++ UI uses for body
/// text. The last two are narrower fixed faces rather than Courier: a line as
/// long as the alphabet strip (26 cells) or the keypad legend does not fit on
/// 240 pixels in any size of Courier, so without them [`best_fit_font`] runs out
/// of candidates and overflows the screen.
pub(crate) static BODY_FONTS: [FontRenderer; 5] = [
    FontRenderer::new::<fonts::u8g2_font_courR14_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR12_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR10_tr>(),
    FontRenderer::new::<fonts::u8g2_font_7x13_tr>(),
    FontRenderer::new::<fonts::u8g2_font_courR08_tr>(),
];

/// The progress line, the last accepted word, and the finished phrase are all
/// short and fixed in place, so they get one small face rather than a best-fit
/// list.
pub(crate) static HEADER_FONT: FontRenderer = FontRenderer::new::<fonts::u8g2_font_courR10_tr>();

/// Width left for text once both margins are taken off `bounds`.
pub(crate) fn usable_width(bounds: &Rectangle) -> u32 {
    bounds.size.width.saturating_sub(HORIZONTAL_MARGIN * 2)
}

/// Picks the largest font whose rendering of `text` fits within `max_width`,
/// falling back to the smallest if none do. `fonts` must be ordered largest
/// first. Mirrors `getBestFitFont` in `../src/screen/tft.cpp`.
///
/// Generic over `Content` so a single `char` can be measured without first
/// having to put it in a string.
///
/// Fits against the advance rather than the bounding box: the box covers the
/// inked pixels only, so a run measured that way loses the side bearing of
/// every glyph and a long one — the alphabet strip, say — reports as fitting
/// while rendering off both edges. Advance is also what TFT_eSPI's `textWidth`
/// returns, so this stays equivalent to the C++.
pub(crate) fn best_fit_font<C>(fonts: &[FontRenderer], text: C, max_width: u32) -> &FontRenderer
where
    C: Content + Copy,
{
    fonts
        .iter()
        .find(|font| {
            font.get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
                // A missing glyph or an oversized run both mean "not this font".
                .ok()
                .is_some_and(|dimensions| dimensions.advance.x <= max_width as i32)
        })
        .unwrap_or_else(|| fonts.last().expect("font list is never empty"))
}
