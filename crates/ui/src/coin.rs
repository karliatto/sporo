//! The coin flips that finish a phrase.

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use u8g2_fonts::{
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer,
};

use sporo_app::action::Action;
use sporo_core::flips::{Flip, Flips, FLIP_COUNT};

use crate::{
    best_fit_font,
    legend::{self, Hint},
    usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, DIM_COLOR, HEADER_FONT,
    HORIZONTAL_MARGIN, LOGO_FONTS, TEXT_COLOR,
};

/// Names the screen. It opens on its own the moment the last word is accepted,
/// rather than being picked from the menu, so it has to say what it is.
const TITLE_TEXT: &str = "coin flips";

/// The keypad legend while flips are still coming in. `Confirm` is deliberately
/// absent: it does nothing until the last flip lands, and offering a key that is
/// inert is worse than not offering it at all.
pub(crate) const FLIPPING: [Hint; 3] = [
    Hint::new(&[Action::Heads], "heads"),
    Hint::new(&[Action::Tails], "tails"),
    Hint::new(&[Action::Back], "undo"),
];

/// Replaces it once every flip is in — the cue, along with the row turning
/// [`ACCENT_COLOR`], that `Confirm` will now do something.
pub(crate) const READY: [Hint; 2] = [
    Hint::new(&[Action::Back], "undo a flip"),
    Hint::new(&[Action::Confirm], "accept"),
];

/// Gap between the top edge and the progress line. The word screen's value, so
/// the counter does not jump when that screen hands over to this one.
const HEADER_MARGIN: i32 = 4;

/// Distance from the bottom edge up to the middle of the flip row, which leaves
/// the space below it for the underlines and the hint line.
const ROW_FROM_BOTTOM: i32 = 71;

/// Width of one flip's cell. Fixed rather than measured per glyph: `H` and `T`
/// are not the same width in every face, and a row that shuffled sideways as
/// flips landed would be hard to check against the coins on the table.
const SLOT_PITCH: i32 = 28;

/// Space kept clear between one cell's glyph and the next, and so the budget the
/// face is chosen against. Sizing the face to the whole row instead would take
/// the largest that fits end to end — glyphs a pixel apart, reading as one word
/// rather than as seven separate flips.
const SLOT_GAP: i32 = 6;

/// The underline under every cell, measured down from the middle of the row. It
/// is what makes a slot that has not been flipped yet read as a slot rather than
/// as blank panel, so it is drawn for all of them, not only the filled ones.
const SLOT_BAR_OFFSET: i32 = 20;
const SLOT_BAR_HEIGHT: u32 = 2;
const SLOT_BAR_WIDTH: u32 = 20;

/// Progress along the top, the row of flips across the middle, keypad legend
/// along the bottom.
///
/// The row is drawn left to right in the order the flips were entered, which is
/// also most-significant bit first — so what is on the panel is the binary
/// number the user could write down and check later.
pub(crate) fn show_coin_screen<D>(display: &mut D, flips: &Flips)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let right = bounds.size.width as i32 - HORIZONTAL_MARGIN as i32;
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    HEADER_FONT
        .render_aligned(
            format_args!("{}/{}", flips.count(), FLIP_COUNT),
            Point::new(HORIZONTAL_MARGIN as i32, HEADER_MARGIN),
            VerticalPosition::Top,
            HorizontalAlignment::Left,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("progress render failed");

    HEADER_FONT
        .render_aligned(
            TITLE_TEXT,
            Point::new(right, HEADER_MARGIN),
            VerticalPosition::Top,
            HorizontalAlignment::Right,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("title render failed");

    draw_flips(display, flips, center.x, bottom - ROW_FROM_BOTTOM);

    let legend = legend::compose(hints(flips));

    best_fit_font(&BODY_FONTS, legend.as_str(), usable_width)
        .render_aligned(
            legend.as_str(),
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("hint render failed");
}

/// The legend for `flips`: which keys do something depends on whether the row
/// is full yet.
pub(crate) fn hints(flips: &Flips) -> &'static [Hint] {
    if flips.is_complete() {
        &READY
    } else {
        &FLIPPING
    }
}

/// Draws every slot at a fixed pitch, filled ones as a letter and the rest as
/// bare underline.
///
/// Cell by cell rather than as one string, the way `draw_alphabet` does it: each
/// slot carries its own colour, and the underline needs a per-cell `x` to sit
/// under.
fn draw_flips<D>(display: &mut D, flips: &Flips, center_x: i32, center_y: i32)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    let font = row_font();

    // Once every flip is in, the whole row goes cyan — the same way the wordlist
    // screen picks out the word the user did not type. Paired with the legend
    // swapping under it, that is what says `Confirm` now does something.
    let letter_color = if flips.is_complete() {
        ACCENT_COLOR
    } else {
        TEXT_COLOR
    };

    let width = SLOT_PITCH * FLIP_COUNT as i32;
    let first = center_x - width / 2 + SLOT_PITCH / 2;

    for index in 0..FLIP_COUNT {
        let x = first + index as i32 * SLOT_PITCH;

        if let Some(flip) = flips.flip(index) {
            let letter = match flip {
                Flip::Heads => 'H',
                Flip::Tails => 'T',
            };

            font.render_aligned(
                letter,
                Point::new(x, center_y),
                VerticalPosition::Center,
                HorizontalAlignment::Center,
                FontColor::Transparent(letter_color),
                display,
            )
            .expect("flip render failed");
        }

        // The bar under the slot the next flip lands in is the cursor, and needs
        // no state of its own: `count` is where the row has got to.
        let bar_color = if index == flips.count() {
            ACCENT_COLOR
        } else {
            DIM_COLOR
        };

        Rectangle::new(
            Point::new(x - SLOT_BAR_WIDTH as i32 / 2, center_y + SLOT_BAR_OFFSET),
            Size::new(SLOT_BAR_WIDTH, SLOT_BAR_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_fill(bar_color))
        .draw(display)
        .expect("slot bar render failed");
    }
}

/// The face the row is set in: the largest whose glyph fits one cell with
/// [`SLOT_GAP`] to spare.
fn row_font() -> &'static FontRenderer {
    best_fit_font(&LOGO_FONTS, 'H', (SLOT_PITCH - SLOT_GAP) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    use embedded_graphics::primitives::Rectangle;

    /// The panel this row is laid out for.
    const PANEL: Size = Size::new(240, 135);

    /// Height of `text` in `font`, or zero if it draws nothing.
    fn text_height(font: &FontRenderer, text: &str) -> i32 {
        font.get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
            .expect("measure failed")
            .bounding_box
            .map(|box_| box_.size.height as i32)
            .unwrap_or_default()
    }

    #[test]
    fn the_flip_row_fits_the_panel_width() {
        let usable = usable_width(&Rectangle::new(Point::zero(), PANEL));
        let width = SLOT_PITCH * FLIP_COUNT as i32;

        assert!(
            width <= usable as i32,
            "{FLIP_COUNT} slots at a {SLOT_PITCH}px pitch run to {width}px, \
             past the {usable}px between the margins",
        );
    }

    #[test]
    fn a_glyph_fits_its_cell() {
        // `best_fit_font` falls back to the smallest face rather than failing,
        // so a pitch too tight for even the smallest logo face would silently
        // give a row of glyphs running into each other rather than a build
        // error.
        let advance = row_font()
            .get_rendered_dimensions('H', Point::zero(), VerticalPosition::Center)
            .expect("measure failed")
            .advance
            .x;

        assert!(
            advance + SLOT_GAP <= SLOT_PITCH,
            "a {advance}px glyph plus a {SLOT_GAP}px gap does not fit a {SLOT_PITCH}px cell",
        );
    }

    #[test]
    fn the_underline_clears_the_glyph_above_it() {
        // The glyphs are drawn centred on the row, so half the face reaches down
        // towards the bar. A face tall enough to meet it would have the letters
        // sitting on their own underline.
        let half = text_height(row_font(), "H") / 2;

        assert!(
            half < SLOT_BAR_OFFSET,
            "a {half}px half-glyph reaches the bar at {SLOT_BAR_OFFSET}px",
        );
    }

    #[test]
    fn the_flip_row_stays_clear_of_the_hint_line() {
        // The row is pinned to the bottom edge and so is the hint, so the two
        // move together — but the row is measured from its middle and grows
        // downwards through the underline, which is what would meet the hint
        // first.
        let usable = usable_width(&Rectangle::new(Point::zero(), PANEL));
        let bottom = PANEL.height as i32;

        let row_bottom = bottom - ROW_FROM_BOTTOM + SLOT_BAR_OFFSET + SLOT_BAR_HEIGHT as i32;
        let legend = legend::compose(&FLIPPING);
        let hint_top = bottom
            - 6
            - text_height(
                best_fit_font(&BODY_FONTS, legend.as_str(), usable),
                legend.as_str(),
            );

        assert!(
            row_bottom <= hint_top,
            "the underlines reach y {row_bottom}, into the hint line at y {hint_top}",
        );
    }

    #[test]
    fn the_row_clears_the_header() {
        // The header is pinned to the top and the row is measured from the
        // bottom, so nothing in the drawing code notices when they meet.
        let header_bottom = HEADER_MARGIN + text_height(&HEADER_FONT, TITLE_TEXT);
        let row_top = PANEL.height as i32 - ROW_FROM_BOTTOM - text_height(row_font(), "H") / 2;

        assert!(
            row_top >= header_bottom,
            "the row starts at y {row_top}, into the header ending at y {header_bottom}",
        );
    }

    #[test]
    fn the_two_legends_are_drawn_in_the_same_face() {
        // The legend swaps the moment the last flip lands. If the two strings
        // chose different faces it would also change size, which reads as the
        // screen having jumped rather than as one word having changed.
        let usable = usable_width(&Rectangle::new(Point::zero(), PANEL));
        let flipping = legend::compose(&FLIPPING);
        let ready = legend::compose(&READY);

        assert!(
            core::ptr::eq(
                best_fit_font(&BODY_FONTS, flipping.as_str(), usable),
                best_fit_font(&BODY_FONTS, ready.as_str(), usable),
            ),
            "{flipping:?} and {ready:?} are set in different faces",
        );
    }
}
