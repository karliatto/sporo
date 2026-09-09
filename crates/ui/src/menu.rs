use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment},
};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use sporo_core::word_entry::{KEY_ADD, KEY_DELETE, KEY_DOWN, KEY_UP};

use crate::{
    best_fit_font, usable_width, ACCENT_COLOR, BACKGROUND_COLOR, BODY_FONTS, DIM_COLOR,
    HORIZONTAL_MARGIN, TEXT_COLOR,
};

const KEY_SELECT: char = KEY_ADD;
const KEY_BACK: char = KEY_DELETE;

const HINT_TEXT: &str = "2/8 move  5 select  * back";

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

/// What the menu can be asked to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    /// The word-entry flow: eleven words typed, the twelfth derived.
    GenerateMnemonic,
    /// Firmware version and the shape of the phrase it builds.
    About,
}

impl MenuItem {
    /// Every entry, in the order they are drawn and walked.
    pub const ALL: [Self; 2] = [Self::GenerateMnemonic, Self::About];

    /// Keep labels to ASCII: every face in [`BODY_FONTS`] is a u8g2 `_tr`
    /// variant, whose glyphs stop at the end of ASCII. A character outside it
    /// makes measuring fail, and [`best_fit_font`] reads a measuring failure as
    /// "not this font" — so the menu would quietly drop to the smallest face on
    /// the list rather than complain.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GenerateMnemonic => "Generate 12th word mnemonic",
            Self::About => "About",
        }
    }
}

/// What a keypress did, for the caller to act on.
///
/// Richer than the `bool` [`sporo_core::word_entry::WordEntry::handle_key`]
/// returns, because a menu has two ways out as well as a redraw. [`Self::Ignored`]
/// serves the same purpose as that `bool`: it keeps the full-screen blit off
/// keys that changed nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuEvent {
    /// Nothing to do, and nothing to redraw.
    Ignored,
    /// The cursor moved; the screen needs drawing again.
    Moved,
    /// An entry was chosen.
    Chose(MenuItem),
    /// The user backed out.
    Dismissed,
}

pub struct Menu {
    cursor: usize,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub const fn new() -> Self {
        Self { cursor: 0 }
    }

    pub const fn selected(&self) -> MenuItem {
        MenuItem::ALL[self.cursor]
    }

    /// Applies a keypress.
    ///
    /// The cursor wraps at both ends, as it does on the alphabet strip: with
    /// only a couple of entries, stopping dead at the last one is a worse
    /// surprise than coming back around.
    pub fn handle_key(&mut self, key: char) -> MenuEvent {
        match key {
            KEY_UP => {
                self.cursor = (self.cursor + MenuItem::ALL.len() - 1) % MenuItem::ALL.len();

                MenuEvent::Moved
            }
            KEY_DOWN => {
                self.cursor = (self.cursor + 1) % MenuItem::ALL.len();

                MenuEvent::Moved
            }
            KEY_SELECT => MenuEvent::Chose(self.selected()),
            KEY_BACK => MenuEvent::Dismissed,
            _ => MenuEvent::Ignored,
        }
    }
}

/// The entries stacked down the middle, each in a box of its own, keypad legend
/// along the bottom.
///
/// The selected box is filled and its label drawn in the background colour. That
/// is a change of shape and not only of colour, which is what the cursor bar on
/// the word screen exists to provide — so this screen needs no separate marker.
pub fn show_menu_screen<D>(display: &mut D, menu: &Menu)
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
    // characters against the other entry's twenty-seven, and boxes hugging their
    // text would leave the list ragged.
    let left = HORIZONTAL_MARGIN as i32;

    let rows = MenuItem::ALL.len() as i32;
    let first_row = center.y - (rows - 1) * ROW_HEIGHT / 2;

    for (index, item) in MenuItem::ALL.iter().enumerate() {
        let selected = index == menu.cursor;
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
            item.label(),
            Point::new(left + BOX_PADDING, y),
            VerticalPosition::Center,
            FontColor::Transparent(color),
            display,
        )
        .expect("label render failed");
    }

    best_fit_font(&BODY_FONTS, HINT_TEXT, usable_width)
        .render_aligned(
            HINT_TEXT,
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
        .map(|item| item.label())
        .max_by_key(|label| label.len())
        .expect("the menu is never empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_menu_starts_on_the_first_entry() {
        assert_eq!(Menu::new().selected(), MenuItem::GenerateMnemonic);
    }

    #[test]
    fn the_cursor_wraps_forwards() {
        let mut menu = Menu::new();

        assert_eq!(menu.handle_key(KEY_DOWN), MenuEvent::Moved);
        assert_eq!(menu.selected(), MenuItem::About);

        // Past the last entry is the first again, not a dead end.
        assert_eq!(menu.handle_key(KEY_DOWN), MenuEvent::Moved);
        assert_eq!(menu.selected(), MenuItem::GenerateMnemonic);
    }

    #[test]
    fn the_cursor_wraps_backwards() {
        let mut menu = Menu::new();

        assert_eq!(menu.handle_key(KEY_UP), MenuEvent::Moved);
        assert_eq!(menu.selected(), MenuItem::About);
    }

    #[test]
    fn select_reports_the_entry_under_the_cursor() {
        let mut menu = Menu::new();
        assert_eq!(
            menu.handle_key(KEY_SELECT),
            MenuEvent::Chose(MenuItem::GenerateMnemonic),
        );

        menu.handle_key(KEY_DOWN);
        assert_eq!(
            menu.handle_key(KEY_SELECT),
            MenuEvent::Chose(MenuItem::About),
        );
    }

    /// Choosing does not move the cursor: coming back from a screen should land
    /// on the entry that opened it.
    #[test]
    fn choosing_leaves_the_cursor_where_it_was() {
        let mut menu = Menu::new();
        menu.handle_key(KEY_DOWN);
        menu.handle_key(KEY_SELECT);

        assert_eq!(menu.selected(), MenuItem::About);
    }

    #[test]
    fn back_dismisses_the_menu() {
        assert_eq!(Menu::new().handle_key(KEY_BACK), MenuEvent::Dismissed);
    }

    /// Every other key has to be inert, or the screen repaints for nothing.
    #[test]
    fn unbound_keys_are_ignored() {
        let mut menu = Menu::new();

        for key in ['0', '1', '2', '3', '7', '8', '9', '#'] {
            assert_eq!(
                menu.handle_key(key),
                MenuEvent::Ignored,
                "{key:?} was bound"
            );
        }

        assert_eq!(menu.selected(), MenuItem::GenerateMnemonic);
    }

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
        let hint_top = bottom - 6 - text_height(HINT_TEXT, usable);

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
            assert!(!item.label().is_empty(), "{item:?} has no label");
        }
    }

    /// The cursor is drawn from `MenuItem::ALL`'s length, so the two must agree.
    #[test]
    fn the_cursor_reaches_every_entry() {
        let mut menu = Menu::new();

        for item in MenuItem::ALL {
            assert_eq!(menu.selected(), item);
            menu.handle_key(KEY_DOWN);
        }

        assert_eq!(menu.selected(), MenuItem::ALL[0], "the walk did not wrap");
    }
}
