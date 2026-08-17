use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use sporo_core::bip39::{Mnemonic, WORD_COUNT_TOTAL};

use crate::{
    best_fit_font, usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, HEADER_FONT,
    HORIZONTAL_MARGIN, TEXT_COLOR,
};

const DONE_TEXT: &str = "phrase complete  * to edit";

const WORDLIST_ROWS: usize = 6;
const WORDLIST_COLUMNS: usize = WORD_COUNT_TOTAL / WORDLIST_ROWS;

const WORDLIST_NUMBER_WIDTH: i32 = 13;
const WORDLIST_NUMBER_GAP: i32 = 4;

/// Gap above the first row, and the strip left free at the bottom for the hint.
const WORDLIST_TOP: i32 = 5;
const WORDLIST_HINT_SPACE: i32 = 16;

// Three columns would silently overlap rather than fail to build.
const _: () = assert!(WORDLIST_ROWS * WORDLIST_COLUMNS == WORD_COUNT_TOTAL);

pub fn show_wordlist_screen<D>(display: &mut D, mnemonic: &Mnemonic)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    let column_width = bounds.size.width as i32 / WORDLIST_COLUMNS as i32;
    let row_height = (bottom - WORDLIST_TOP - WORDLIST_HINT_SPACE) / WORDLIST_ROWS as i32;

    for (position, word) in mnemonic.iter().enumerate() {
        // Column-major: 1-6 down the left, 7-12 down the right.
        let column = position / WORDLIST_ROWS;
        let row = position % WORDLIST_ROWS;

        let left = column as i32 * column_width + HORIZONTAL_MARGIN as i32;
        let y = WORDLIST_TOP + row as i32 * row_height;

        // The number is right-aligned and the word left-aligned from a fixed
        // offset, so the words line up in a column whether the number is one
        // digit or two.
        HEADER_FONT
            .render_aligned(
                format_args!("{}", position + 1),
                Point::new(left + WORDLIST_NUMBER_WIDTH, y),
                VerticalPosition::Top,
                HorizontalAlignment::Right,
                FontColor::Transparent(ACCENT_COLOR),
                display,
            )
            .expect("word number render failed");

        let color = if position == WORD_COUNT_TOTAL - 1 {
            ACCENT_COLOR
        } else {
            TEXT_COLOR
        };

        HEADER_FONT
            .render_aligned(
                *word,
                Point::new(left + WORDLIST_NUMBER_WIDTH + WORDLIST_NUMBER_GAP, y),
                VerticalPosition::Top,
                HorizontalAlignment::Left,
                FontColor::Transparent(color),
                display,
            )
            .expect("word render failed");
    }

    best_fit_font(&BODY_FONTS, DONE_TEXT, usable_width)
        .render_aligned(
            DONE_TEXT,
            Point::new(center.x, bottom - 4),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("hint render failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel this grid is laid out for.
    const PANEL_WIDTH: i32 = 240;
    const PANEL_HEIGHT: i32 = 135;

    fn advance(text: &str) -> i32 {
        HEADER_FONT
            .get_rendered_dimensions(text, Point::zero(), VerticalPosition::Top)
            .expect("measure failed")
            .advance
            .x
    }

    fn column_left(column: i32) -> i32 {
        column * (PANEL_WIDTH / WORDLIST_COLUMNS as i32) + HORIZONTAL_MARGIN as i32
    }

    /// Rendered width of the widest word the list can put in the grid.
    fn longest_word() -> i32 {
        sporo_core::bip39_wordlist::words()
            .map(advance)
            .max()
            .expect("the wordlist is not empty")
    }

    /// `row_height` is a truncating division, so six rows can only ever come out
    /// shorter than the space they were given — never longer, which is what
    /// would push the last row into the hint line.
    #[test]
    fn the_rows_stay_clear_of_the_hint_line() {
        let available = PANEL_HEIGHT - WORDLIST_TOP - WORDLIST_HINT_SPACE;
        let row_height = available / WORDLIST_ROWS as i32;

        assert!(row_height > 0, "six rows do not fit at all");
        assert!(
            WORDLIST_TOP + WORDLIST_ROWS as i32 * row_height <= PANEL_HEIGHT - WORDLIST_HINT_SPACE,
            "the last row overlaps the hint line",
        );
    }

    /// Numbers are right-aligned to a point 13px into their column while
    /// rendering 18px wide, so 10-12 start 5px left of it. In the first column
    /// that overhang comes out of the left margin, and has to stay on the panel.
    #[test]
    fn a_two_digit_number_does_not_fall_off_the_left_edge() {
        let start = column_left(0) + WORDLIST_NUMBER_WIDTH - advance("12");

        assert!(
            start >= 0,
            "the number in row 10 starts off-panel at x {start}"
        );
    }

    /// The same overhang in the second column lands next to the first column's
    /// words instead of a margin, which is the collision worth guarding: it is
    /// what would happen first if the wordlist ever grew a third column, or the
    /// words a longer entry.
    #[test]
    fn the_longest_word_does_not_reach_the_next_column_number() {
        let longest = longest_word();

        let word_end = column_left(0) + WORDLIST_NUMBER_WIDTH + WORDLIST_NUMBER_GAP + longest;
        let next_number_start = column_left(1) + WORDLIST_NUMBER_WIDTH - advance("12");

        assert!(
            word_end <= next_number_start,
            "a {longest}px word runs to x {word_end}, into the number at x {next_number_start}",
        );
    }

    /// Every word must also stay inside the panel in the rightmost column.
    #[test]
    fn the_longest_word_fits_the_last_column() {
        let end = column_left(WORDLIST_COLUMNS as i32 - 1)
            + WORDLIST_NUMBER_WIDTH
            + WORDLIST_NUMBER_GAP
            + longest_word();

        assert!(end <= PANEL_WIDTH, "the last column runs to x {end}");
    }
}
