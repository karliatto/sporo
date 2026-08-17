use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use heapless::String;
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use sporo_core::{
    bip39_wordlist::LetterSet,
    word_entry::{WordEntry, ALPHABET, ALPHABET_TEXT, MAX_WORD_LEN, WORD_COUNT},
};

use crate::{
    best_fit_font, usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, DIM_COLOR,
    HEADER_FONT, HORIZONTAL_MARGIN, LOGO_FONTS, TEXT_COLOR, WARNING_COLOR,
};

/// The keypad legend along the bottom of the word screen.
const HINT_TEXT: &str = "4/6 pick  5 add  * del  # next";

/// Shown when `#` is pressed on a spelling several words share. Since a spelling
/// that matches nothing cannot be typed, that is the only way to get here, so
/// the instruction is to carry on rather than to correct anything.
const REJECT_TEXT: &str = "several words start so";

/// Gap between the top edge and the progress line.
const HEADER_MARGIN: i32 = 4;

/// Distance from the bottom edge up to the middle of the alphabet strip, which
/// leaves the space below it for the hint line.
const ALPHABET_FROM_BOTTOM: i32 = 40;

/// The bar marking the selected letter, measured down from the middle of the
/// strip. Colour alone is too easy to lose at this size.
const CURSOR_BAR_OFFSET: i32 = 8;
const CURSOR_BAR_HEIGHT: u32 = 2;

/// Progress along the top, the word being spelled out across the middle, and
/// the alphabet the cursor walks below it. Standing in for the recovery-phrase
/// screen the real firmware has.
pub fn show_word_screen<D>(display: &mut D, entry: &WordEntry)
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
            format_args!("{}/{}", entry.word_number(), WORD_COUNT),
            Point::new(HORIZONTAL_MARGIN as i32, HEADER_MARGIN),
            VerticalPosition::Top,
            HorizontalAlignment::Left,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("progress render failed");

    // The word just accepted, kept in the corner as a check that it went in as
    // intended
    if let Some(previous) = entry.accepted().last() {
        HEADER_FONT
            .render_aligned(
                previous.as_str(),
                Point::new(right, HEADER_MARGIN),
                VerticalPosition::Top,
                HorizontalAlignment::Right,
                FontColor::Transparent(TEXT_COLOR),
                display,
            )
            .expect("previous word render failed");
    }

    // A refused word is coloured rather than moved or cleared: it is still the
    // word being spelled, and the next keypress carries on from it.
    let word_color = if entry.rejected() {
        WARNING_COLOR
    } else {
        TEXT_COLOR
    };

    draw_word(
        display,
        entry.current(),
        entry.selected(),
        word_color,
        Point::new(center.x, center.y - 12),
        usable_width,
    );
    draw_alphabet(
        display,
        entry.cursor(),
        entry.reachable(),
        center.x,
        bottom - ALPHABET_FROM_BOTTOM,
        usable_width,
    );

    let (hint, hint_color) = if entry.rejected() {
        (REJECT_TEXT, WARNING_COLOR)
    } else {
        (HINT_TEXT, ACCENT_COLOR)
    };

    best_fit_font(&BODY_FONTS, hint, usable_width)
        .render_aligned(
            hint,
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(hint_color),
            display,
        )
        .expect("hint render failed");
}

/// Word and preview are sized and centred as one, so the word drifts left as it
/// grows instead of the preview appearing to shove it sideways.
fn draw_word<D>(
    display: &mut D,
    word: &str,
    preview: Option<char>,
    color: Rgb565,
    center: Point,
    usable_width: u32,
) where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    let mut whole: String<{ MAX_WORD_LEN + 1 }> = String::new();
    let _ = whole.push_str(word);
    if let Some(letter) = preview {
        let _ = whole.push(letter);
    }

    let font = best_fit_font(&LOGO_FONTS, whole.as_str(), usable_width);
    let width = font
        .get_rendered_dimensions(whole.as_str(), Point::zero(), VerticalPosition::Center)
        .expect("word measure failed")
        .advance
        .x;

    let mut pen = Point::new(center.x - width / 2, center.y);

    pen.x += font
        .render(
            word,
            pen,
            VerticalPosition::Center,
            FontColor::Transparent(color),
            display,
        )
        .expect("word render failed")
        .advance
        .x;

    if let Some(letter) = preview {
        font.render(
            letter,
            pen,
            VerticalPosition::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("preview render failed");
    }
}

/// Draws the whole alphabet on one line with the cursor's letter picked out.
fn draw_alphabet<D>(
    display: &mut D,
    cursor: Option<usize>,
    reachable: LetterSet,
    center_x: i32,
    center_y: i32,
    usable_width: u32,
) where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    let font = best_fit_font(&BODY_FONTS, ALPHABET_TEXT, usable_width);
    let width = font
        .get_rendered_dimensions(ALPHABET_TEXT, Point::zero(), VerticalPosition::Center)
        .expect("alphabet measure failed")
        .advance
        .x;

    let mut pen = Point::new(center_x - width / 2, center_y);

    for (index, letter) in ALPHABET.iter().enumerate() {
        let selected = cursor == Some(index);
        let color = match (selected, reachable.contains(index)) {
            (true, _) => ACCENT_COLOR,
            (false, true) => TEXT_COLOR,
            (false, false) => DIM_COLOR,
        };

        let advance = font
            .render(
                *letter as char,
                pen,
                VerticalPosition::Center,
                FontColor::Transparent(color),
                display,
            )
            .expect("alphabet render failed")
            .advance
            .x;

        if selected {
            // The face is monospaced, so the advance is the letter's cell and
            // the bar lines up under it exactly.
            Rectangle::new(
                Point::new(pen.x, center_y + CURSOR_BAR_OFFSET),
                Size::new(advance.max(0) as u32, CURSOR_BAR_HEIGHT),
            )
            .into_styled(PrimitiveStyle::with_fill(ACCENT_COLOR))
            .draw(display)
            .expect("cursor bar render failed");
        }

        pen.x += advance;
    }
}
