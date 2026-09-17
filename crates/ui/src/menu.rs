use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment},
};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use sporo_app::{action::Action, menu::MenuItem};
use sporo_core::bip39::SeedLength;

use crate::{
    best_fit_font,
    legend::{self, Hint},
    usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, DIM_COLOR, HORIZONTAL_MARGIN,
    TEXT_COLOR,
};

/// The keypad legend along the bottom of the menu.
pub(crate) const HINTS: [Hint; 3] = [
    Hint::new(&[Action::Up, Action::Down], "move"),
    Hint::new(&[Action::Select], "select"),
    Hint::new(&[Action::Back], "back"),
];

/// Space between the middle of one entry's box and the middle of the next.
/// Four pixels more than [`BOX_HEIGHT`], so the boxes read as separate rather
/// than as one block with lines through it.
const ROW_HEIGHT: i32 = 24;

/// Height of the box drawn around an entry: the body face is around thirteen
/// pixels tall, and the rest is padding.
const BOX_HEIGHT: u32 = 20;

/// Corner rounding, and the gap between a box's left edge and its label.
const BOX_RADIUS: u32 = 3;
const BOX_PADDING: i32 = 6;

/// What an entry is called on the panel.
///
/// Keep labels to ASCII: every face in [`BODY_FONTS`] is a u8g2 `_tr` variant,
/// whose glyphs stop at the end of ASCII. A character outside it makes measuring
/// fail, and [`best_fit_font`] reads a measuring failure as "not this font" — so
/// the menu would quietly drop to the smallest face on the list rather than
/// complain.
pub(crate) const fn label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::GenerateMnemonic(SeedLength::Words12) => "Generate 12th word",
        MenuItem::GenerateMnemonic(SeedLength::Words24) => "Generate 24th word",
        MenuItem::About => "About",
    }
}

/// The entries stacked down the middle, each in a box of its own, keypad legend
/// along the bottom.
///
/// The selected box is filled and its label drawn in the background colour. That
/// is a change of shape and not only of colour, which is what the cursor bar on
/// the word screen exists to provide — so this screen needs no separate marker.
pub(crate) fn show_menu_screen<D>(display: &mut D, selected: MenuItem)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    display.clear(BACKGROUND_COLOR).expect("clear failed");

    let bounds = display.bounding_box();
    let center = bounds.center();
    let bottom = bounds.size.height as i32;
    let usable_width = usable_width(&bounds);

    // One face for every entry, chosen from the longest of them. Sizing each row
    // on its own would render "About" large and the long entry small, which
    // reads as two different kinds of thing rather than one list.
    let font = best_fit_font(&BODY_FONTS, longest_label(), usable_width);

    // Every box is the full width, not sized to its label: "About" is five
    // characters against the others' eighteen, and boxes hugging their
    // text would leave the list ragged.
    let left = HORIZONTAL_MARGIN as i32;

    let rows = MenuItem::ALL.len() as i32;
    let first_row = center.y - (rows - 1) * ROW_HEIGHT / 2;

    for (index, item) in MenuItem::ALL.iter().enumerate() {
        let selected = *item == selected;
        let y = first_row + index as i32 * ROW_HEIGHT;

        let outline = Rectangle::new(
            Point::new(left, y - BOX_HEIGHT as i32 / 2),
            Size::new(usable_width, BOX_HEIGHT),
        );

        // Stroked on the inside, so an unselected box occupies exactly the
        // rectangle asked for. The default alignment straddles the boundary and
        // would put half a pixel outside it.
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
            .expect("entry box render failed");

        // Drawn after the box, and in the background colour on top of the fill,
        // so the selected entry reads as inverted rather than tinted.
        let color = if selected {
            BACKGROUND_COLOR
        } else {
            TEXT_COLOR
        };

        font.render(
            label(*item),
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

/// The entry that decides the font for all of them.
fn longest_label() -> &'static str {
    MenuItem::ALL
        .iter()
        .map(|item| label(*item))
        .max_by_key(|label| label.len())
        .expect("the menu is never empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boxes are centred and the hint is pinned to the bottom, so the two
    /// grow towards each other as entries are added. Nothing in the drawing code
    /// notices when they meet — it would just overlap — so it is checked here.
    ///
    /// Measured against [`BOX_HEIGHT`] rather than the label: the box is the
    /// taller of the two, and it is the box that would touch the hint first.
    #[test]
    fn the_entries_stay_clear_of_the_hint_line() {
        const PANEL: Size = Size::new(240, 135);

        let bounds = Rectangle::new(Point::zero(), PANEL);
        let usable = usable_width(&bounds);
        let bottom = PANEL.height as i32;

        let rows = MenuItem::ALL.len() as i32;
        let first_row = bounds.center().y - (rows - 1) * ROW_HEIGHT / 2;
        let last_row = first_row + (rows - 1) * ROW_HEIGHT;

        let boxes_bottom = last_row + BOX_HEIGHT as i32 / 2;
        let hint_top = bottom - 6 - text_height(legend::compose(&HINTS).as_str(), usable);

        assert!(
            boxes_bottom <= hint_top,
            "{rows} boxes reach y {boxes_bottom}, into the hint line at y {hint_top}",
        );
    }

    /// A box has to be tall enough for the face the labels are drawn in, or the
    /// text is clipped by its own frame.
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

    /// Height of `text` in the face the menu would pick for it.
    fn text_height(text: &str, usable: u32) -> i32 {
        best_fit_font(&BODY_FONTS, text, usable)
            .get_rendered_dimensions(text, Point::zero(), VerticalPosition::Center)
            .expect("measure failed")
            .bounding_box
            .map(|box_| box_.size.height as i32)
            .unwrap_or_default()
    }

    #[test]
    fn every_entry_has_a_label() {
        for item in MenuItem::ALL {
            assert!(!label(item).is_empty(), "{item:?} has no label");
        }
    }
}
