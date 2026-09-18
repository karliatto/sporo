use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment},
};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use sporo_app::action::Action;
use sporo_core::bip39::SeedLength;

use crate::{
    best_fit_font,
    legend::{self, Hint},
    usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, DIM_COLOR, HEADER_FONT,
    HORIZONTAL_MARGIN, TEXT_COLOR,
};

/// The keypad legend along the bottom. The same three as the menu's: this is a
/// list with a cursor on it, so it is driven like one.
pub(crate) const HINTS: [Hint; 3] = [
    Hint::new(&[Action::Up, Action::Down], "move"),
    Hint::new(&[Action::Select], "select"),
    Hint::new(&[Action::Back], "back"),
];

/// What the question is, above the choices.
const TITLE_TEXT: &str = "phrase length";

/// Gap between the top edge and the title.
const TITLE_MARGIN: i32 = 4;

/// Matching the menu's, so a list of boxes is a list of boxes wherever it shows.
const ROW_HEIGHT: i32 = 24;
const BOX_HEIGHT: u32 = 20;
const BOX_RADIUS: u32 = 3;
const BOX_PADDING: i32 = 6;

/// What a length is called on the panel. ASCII only, for the reason the menu's
/// labels are — see the note on [`crate::menu::label`].
const fn label(length: SeedLength) -> &'static str {
    match length {
        SeedLength::Words12 => "12 words",
        SeedLength::Words24 => "24 words",
    }
}

/// The two lengths stacked down the middle, drawn like menu entries because they
/// behave like them.
///
/// Only two rows, so unlike the menu this one has room to spare and a title
/// above it saying what is being picked.
pub(crate) fn show_length_screen<D>(display: &mut D, selected: SeedLength)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    HEADER_FONT
        .render_aligned(
            TITLE_TEXT,
            Point::new(center.x, TITLE_MARGIN),
            VerticalPosition::Top,
            HorizontalAlignment::Center,
            FontColor::Transparent(TEXT_COLOR),
            display,
        )
        .expect("title render failed");

    // One face for both, chosen from the longer, so the two rows read as one
    // list rather than as two sizes of thing.
    let font = best_fit_font(&BODY_FONTS, longest_label(), usable_width);
    let left = HORIZONTAL_MARGIN as i32;

    let rows = SeedLength::ALL.len() as i32;
    let first_row = center.y - (rows - 1) * ROW_HEIGHT / 2;

    for (index, length) in SeedLength::ALL.iter().enumerate() {
        let selected = *length == selected;
        let y = first_row + index as i32 * ROW_HEIGHT;

        let outline = Rectangle::new(
            Point::new(left, y - BOX_HEIGHT as i32 / 2),
            Size::new(usable_width, BOX_HEIGHT),
        );

        let style = if selected {
            PrimitiveStyleBuilder::new()
                .fill_color(ACCENT_COLOR)
                .build()
        } else {
            PrimitiveStyleBuilder::new()
                .stroke_color(DIM_COLOR)
                .stroke_width(1)
                .stroke_alignment(StrokeAlignment::Inside)
                .build()
        };

        RoundedRectangle::with_equal_corners(outline, Size::new(BOX_RADIUS, BOX_RADIUS))
            .into_styled(style)
            .draw(display)
            .expect("length box render failed");

        // Inverted rather than tinted, as the menu's selection is.
        let color = if selected {
            BACKGROUND_COLOR
        } else {
            TEXT_COLOR
        };

        font.render(
            label(*length),
            Point::new(left + BOX_PADDING, y),
            VerticalPosition::Center,
            FontColor::Transparent(color),
            display,
        )
        .expect("label render failed");
    }

    let legend = legend::compose(&HINTS);
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

/// The label that decides the font for both of them.
fn longest_label() -> &'static str {
    SeedLength::ALL
        .iter()
        .map(|length| label(*length))
        .max_by_key(|label| label.len())
        .expect("there is always a length")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same growing-towards-each-other problem the menu has, checked the
    /// same way. Two rows leave room to spare, which is the point of checking:
    /// it says so rather than leaving it to be assumed.
    #[test]
    fn the_rows_stay_clear_of_the_hint_line() {
        const PANEL: Size = Size::new(240, 135);

        let bounds = Rectangle::new(Point::zero(), PANEL);
        let usable = usable_width(&bounds);
        let bottom = PANEL.height as i32;

        let rows = SeedLength::ALL.len() as i32;
        let first_row = bounds.center().y - (rows - 1) * ROW_HEIGHT / 2;
        let last_row = first_row + (rows - 1) * ROW_HEIGHT;

        let boxes_bottom = last_row + BOX_HEIGHT as i32 / 2;
        let hint_top = bottom - 6 - text_height(legend::compose(&HINTS).as_str(), usable);

        assert!(
            boxes_bottom <= hint_top,
            "{rows} boxes reach y {boxes_bottom}, into the hint line at y {hint_top}",
        );
    }

    #[test]
    fn a_box_is_taller_than_the_label_in_it() {
        const PANEL: Size = Size::new(240, 135);

        let usable = usable_width(&Rectangle::new(Point::zero(), PANEL));
        let label = text_height(longest_label(), usable);

        assert!(
            label < BOX_HEIGHT as i32,
            "a {label}px label does not fit a {BOX_HEIGHT}px box",
        );
    }

    #[test]
    fn every_length_has_a_label() {
        for length in SeedLength::ALL {
            assert!(!label(length).is_empty(), "{length:?} has no label");
        }
    }

    fn text_height(text: &str, usable: u32) -> i32 {
        best_fit_font(&BODY_FONTS, text, usable)
            .get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
            .expect("measure failed")
            .bounding_box
            .map(|box_| box_.size.height as i32)
            .unwrap_or_default()
    }
}
