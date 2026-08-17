use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use crate::{
    best_fit_font, usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, LOGO_FONTS, TEXT_COLOR,
};

const LOGO_TEXT: &str = "SPORO";
const INSTRUCTION_TEXT: &str = "press any key";

/// The idle screen: brand mark just above centre, instruction pinned to the
/// bottom edge. Modelled on `showHomeScreen` in `../src/screen/tft.cpp`.
pub fn show_home_screen<D>(display: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    // Nudged up slightly so the block of text reads as centred once the
    // instruction line at the bottom is taken into account.
    best_fit_font(&LOGO_FONTS, LOGO_TEXT, usable_width)
        .render_aligned(
            LOGO_TEXT,
            Point::new(center.x, center.y - 6),
            VerticalPosition::Center,
            HorizontalAlignment::Center,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("logo render failed");

    best_fit_font(&BODY_FONTS, INSTRUCTION_TEXT, usable_width)
        .render_aligned(
            INSTRUCTION_TEXT,
            Point::new(center.x, bottom - 6),
            VerticalPosition::Bottom,
            HorizontalAlignment::Center,
            FontColor::Transparent(ACCENT_COLOR),
            display,
        )
        .expect("instruction render failed");
}
