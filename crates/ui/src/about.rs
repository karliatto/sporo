use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use heapless::String;
use u8g2_fonts::{
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer,
};

use crate::{
    best_fit_font, usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, LOGO_FONTS,
    TEXT_COLOR, WARNING_COLOR,
};

const LOGO_TEXT: &str = "SPORO";
const HINT_TEXT: &str = "* back";

const MESSAGE: [&str; 4] = [
    "Bitcoin tools device.",
    "Do your own research and",
    "don't trust this device.",
    "Your are the responsible",
];

/// Distance from the top edge to the top of the brand mark, and from the bottom
/// edge to the bottom of the `*` legend.
const LOGO_TOP: i32 = 6;
const HINT_BOTTOM: i32 = 6;
const LARGEST_LOGO: usize = 2;

/// Space between one line of the body and the next.
const ROW_HEIGHT: i32 = 14;

/// Enough for the longest line this screen composes, which is the firmware one.
const MAX_LINE: usize = 32;

pub fn show_about_screen<D>(display: &mut D, version: &str)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    let logo_font = best_fit_font(&LOGO_FONTS[LARGEST_LOGO..], LOGO_TEXT, usable_width);
    let hint_font = best_fit_font(&BODY_FONTS, HINT_TEXT, usable_width);

    logo_font
        .render_aligned(
            LOGO_TEXT,
            Point::new(center.x, LOGO_TOP),
            VerticalPosition::Top,
            HorizontalAlignment::Center,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("logo render failed");

    let mut firmware: String<MAX_LINE> = String::new();
    let _ = write_line(&mut firmware, format_args!("firmware {version}"));

    let lines = [firmware.as_str()];

    // One face for every line, from the longest, so the block reads as one
    // paragraph rather than a caption stacked on a warning.
    let longest = lines
        .iter()
        .chain(MESSAGE.iter())
        .max_by_key(|line| line.len())
        .expect("there is always a line");
    let font = best_fit_font(&BODY_FONTS, *longest, usable_width);

    // Centred in what the brand mark and the legend leave behind rather than on
    // the screen: the mark is much the taller of the two, so screen-centre would
    // sit the paragraph low and crowd the legend.
    let rows = (lines.len() + MESSAGE.len()) as i32;
    let first_row = block_center(bottom, logo_font, hint_font) - (rows - 1) * ROW_HEIGHT / 2;

    let colored = lines
        .iter()
        .map(|line| (*line, TEXT_COLOR))
        .chain(MESSAGE.iter().map(|line| (*line, WARNING_COLOR)));

    for (index, (line, color)) in colored.enumerate() {
        font.render_aligned(
            line,
            Point::new(center.x, first_row + index as i32 * ROW_HEIGHT),
            VerticalPosition::Center,
            HorizontalAlignment::Center,
            FontColor::Transparent(color),
            display,
        )
        .expect("line render failed");
    }

    hint_font
        .render_aligned(
            HINT_TEXT,
            Point::new(center.x, bottom - HINT_BOTTOM),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("hint render failed");
}

/// Middle of the band between the bottom of the brand mark and the top of the
/// `*` legend, which is the space the body has to share.
fn block_center(bottom: i32, logo_font: &FontRenderer, hint_font: &FontRenderer) -> i32 {
    let top = LOGO_TOP + text_height(logo_font, LOGO_TEXT);
    let floor = bottom - HINT_BOTTOM - text_height(hint_font, HINT_TEXT);

    (top + floor) / 2
}

/// Height of `text` in `font`, or zero if it draws nothing.
fn text_height(font: &FontRenderer, text: &str) -> i32 {
    font.get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
        .expect("measure failed")
        .bounding_box
        .map(|box_| box_.size.height as i32)
        .unwrap_or_default()
}

/// Formats into a fixed-capacity string. A line too long for [`MAX_LINE`] is
/// truncated rather than dropped: a clipped fact is still readable, and this
/// screen is not worth a panic.
fn write_line(target: &mut String<MAX_LINE>, args: core::fmt::Arguments) -> core::fmt::Result {
    use core::fmt::Write;

    target.write_fmt(args)
}
