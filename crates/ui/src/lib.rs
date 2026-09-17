//! What the user sees and presses: the screens, the look they share, and the
//! keymap that turns printed keys into actions.
//!
//! [`render`] draws whatever `sporo-app` says is showing; the screens behind it
//! hold no state and decide nothing. [`keymap`] is the one place a key character
//! means something, and every legend on the panel is composed from it.
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

mod about;
mod coin;
mod home;
pub mod keymap;
mod legend;
mod menu;
mod word;
mod wordlist;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use sporo_app::view::View;
use u8g2_fonts::{fonts, types::VerticalPosition, Content, FontRenderer};

/// Draws `view` over the whole panel.
///
/// The only way in: the screens themselves are private, so whatever the
/// application says is showing is what gets drawn, and nothing else can be.
pub fn render<D>(display: &mut D, view: &View<'_>)
where
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    match *view {
        View::Home => home::show_home_screen(display),
        View::Menu { selected } => menu::show_menu_screen(display, selected),
        View::About { version } => about::show_about_screen(display, version),
        View::Words(words) => word::show_word_screen(display, words),
        View::Coin(flips) => coin::show_coin_screen(display, flips),
        View::Phrase { mnemonic, page } => wordlist::show_wordlist_screen(display, mnemonic, page),
    }
}

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

#[cfg(test)]
mod tests {
    use sporo_app::{action::Action, app::App, menu::MenuItem};
    use sporo_core::bip39::SeedLength;

    use super::*;
    use crate::legend::{self, Hint};

    /// The legend a view is drawn with. Home has none: it says "press any key",
    /// which names no binding.
    fn hints_for(view: &View<'_>) -> &'static [Hint] {
        match *view {
            View::Home => &[],
            View::Menu { .. } => &menu::HINTS,
            View::About { .. } => &about::HINTS,
            View::Words(_) => &word::HINTS,
            View::Coin(flips) => coin::hints(flips),
            View::Phrase { mnemonic, page } => wordlist::hints(mnemonic, page),
        }
    }

    fn press(app: &mut App, action: Action) {
        let _ = app.press(Some(action));
    }

    /// Types `letters` on the word screen, walking the cursor with `Right`.
    fn spell(app: &mut App, letters: &str) {
        for letter in letters.chars() {
            for _ in 0..26 {
                match app.view() {
                    View::Words(words) if words.selected() == Some(letter) => break,
                    View::Words(_) => press(app, Action::Right),
                    _ => panic!("expected the word screen while spelling"),
                }
            }
            press(app, Action::Select);
        }
    }

    #[test]
    fn a_legend_offers_exactly_the_keys_that_do_something() {
        // A legend that names a key doing nothing sends the user hunting; one
        // that leaves out a key that does something hides a feature. Checked
        // against the application itself, by pressing every action on a copy of
        // one sample state per screen — sample states, not every state, because
        // some legends are legitimately inert in places (`# next` on an empty
        // word, say).
        let mut menu = App::new("0.0.0");
        press(&mut menu, Action::Select);

        let mut about = menu.clone();
        press(&mut about, Action::Up);
        press(&mut about, Action::Select);

        let mut words = menu.clone();
        press(&mut words, Action::Select);
        // "AB" has six letters after it and several words, so every word-screen
        // action does something: move, add, delete, and a refused accept.
        spell(&mut words, "AB");

        let mut samples = std::vec![(menu, "menu"), (about, "about"), (words, "words"),];

        for length in SeedLength::ALL {
            let mut coin = samples[0].0.clone();
            while !matches!(
                coin.view(),
                View::Menu { selected } if selected == MenuItem::GenerateMnemonic(length)
            ) {
                press(&mut coin, Action::Down);
            }
            press(&mut coin, Action::Select);
            for _ in 0..length.entered_words() {
                spell(&mut coin, "ABANDON");
                press(&mut coin, Action::Confirm);
            }

            let mut full = coin.clone();
            for _ in 0..length.final_word_entropy_bits() {
                press(&mut full, Action::Heads);
            }

            let mut phrase = full.clone();
            press(&mut phrase, Action::Confirm);

            // A 24-word phrase's second page, since its legend is its own.
            let mut turned = phrase.clone();
            press(&mut turned, Action::Right);

            samples.push((coin, "coin, no flips"));
            samples.push((full, "coin, every flip"));
            samples.push((phrase, "phrase"));
            samples.push((turned, "phrase, after a page turn"));
        }

        for (app, screen) in samples {
            let hints = hints_for(&app.view());

            for action in Action::ALL {
                let did_something = app.clone().press(Some(action));
                let offered = hints.iter().any(|hint| hint.offers(action));

                assert_eq!(
                    did_something,
                    offered,
                    "on the {screen} screen, {action:?} {} but the legend {}",
                    if did_something {
                        "does something"
                    } else {
                        "does nothing"
                    },
                    if offered {
                        "offers it"
                    } else {
                        "leaves it out"
                    },
                );
            }
        }
    }

    #[test]
    fn the_legends_read_as_they_did_when_typed_by_hand() {
        // Composed from the keymap now, so a rebinding changes these on its own.
        // Pinned anyway: the day one of them changes should be a deliberate
        // edit here, reviewed as a change to what the panel says, not a side
        // effect nobody looked at.
        assert_eq!(
            legend::compose(&crate::word::HINTS),
            "4/6 pick  5 add  * del  # next"
        );
        assert_eq!(
            legend::compose(&crate::menu::HINTS),
            "2/8 move  5 select  * back"
        );
        assert_eq!(
            legend::compose(&crate::coin::FLIPPING),
            "1 heads  0 tails  * undo"
        );
        assert_eq!(
            legend::compose(&crate::coin::READY),
            "* undo a flip  # accept"
        );
        assert_eq!(
            legend::compose(&crate::wordlist::HINTS),
            "phrase complete  * to edit"
        );
        assert_eq!(
            legend::compose(&crate::wordlist::PAGED_HINTS[0]),
            "1/2  4/6 page  * to edit"
        );
        assert_eq!(
            legend::compose(&crate::wordlist::PAGED_HINTS[1]),
            "2/2  4/6 page  * to edit"
        );
        assert_eq!(legend::compose(&crate::about::HINTS), "* back");
    }
}
