//! Does what is drawn actually land on the panel?

use core::convert::Infallible;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

use sporo_app::{action::Action, menu::MenuItem, view::View, word_entry::WordEntry};
use sporo_core::{
    bip39::{Mnemonic, WORD_COUNT, WORD_COUNT_TOTAL},
    bip39_wordlist::{self, ALPHABET},
    flips::{Flip, Flips, FLIP_COUNT},
};
use sporo_ui::{render, BACKGROUND_COLOR};

/// The panel the firmware drives: 135x240 rotated 90 degrees.
const PANEL: Size = Size::new(240, 135);

/// A `DrawTarget` that keeps the bounding box of every non-background pixel.
///
/// Deliberately does not clip to its own size — clipping is what a real driver
/// does, and it is exactly the evidence being looked for here.
struct Recorder {
    size: Size,
    ink: Option<(i32, i32, i32, i32)>,
}

impl Recorder {
    fn new(size: Size) -> Self {
        Self { size, ink: None }
    }

    fn record(&mut self, point: Point) {
        self.ink = Some(match self.ink {
            None => (point.x, point.y, point.x, point.y),
            Some((left, top, right, bottom)) => (
                left.min(point.x),
                top.min(point.y),
                right.max(point.x),
                bottom.max(point.y),
            ),
        });
    }

    /// `(left, top, right, bottom)`, inclusive, or `None` if nothing was drawn.
    fn ink(&self) -> Option<(i32, i32, i32, i32)> {
        self.ink
    }

    /// Whether every drawn pixel landed on the panel.
    fn within_panel(&self) -> bool {
        let Some((left, top, right, bottom)) = self.ink() else {
            return true;
        };

        left >= 0 && top >= 0 && right < self.size.width as i32 && bottom < self.size.height as i32
    }

    /// Panics unless every drawn pixel landed on the panel.
    fn assert_within_panel(&self, what: &str) {
        let (left, top, right, bottom) = self
            .ink()
            .unwrap_or_else(|| panic!("{what} drew nothing at all"));

        assert!(
            self.within_panel(),
            "{what} drew outside {}x{}: ink spans x {left}..={right}, y {top}..={bottom}",
            self.size.width,
            self.size.height,
        );
    }
}

impl OriginDimensions for Recorder {
    fn size(&self) -> Size {
        self.size
    }
}

impl DrawTarget for Recorder {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if color != BACKGROUND_COLOR {
                self.record(point);
            }
        }

        Ok(())
    }
}

/// Spells `word` the way a user would — walking the cursor with `Right` and
/// adding with `Select` — since that is the only way in from outside the crate.
fn spell(entry: &mut WordEntry, word: &str) {
    for letter in word.chars() {
        let target = ALPHABET
            .iter()
            .position(|candidate| *candidate == letter.to_ascii_uppercase() as u8)
            .expect("wordlist letters are a-z");

        for _ in 0..ALPHABET.len() {
            if entry.cursor() == Some(target) {
                break;
            }
            entry.press(Action::Right);
        }

        assert!(entry.press(Action::Select), "{letter:?} was not added");
    }
}

fn longest_word() -> &'static str {
    bip39_wordlist::words()
        .max_by_key(|word| word.len())
        .expect("the wordlist is not empty")
}

#[test]
fn the_home_screen_fits_the_panel() {
    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Home);

    display.assert_within_panel("the home screen");
}

#[test]
fn a_fresh_word_screen_fits_the_panel() {
    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Words(&WordEntry::new()));

    display.assert_within_panel("an empty word screen");
}

/// The widest the middle line ever gets: `draw_word` sizes the word and the
/// preview letter as one run, so the longest word plus a preview is the case
/// that would overflow first.
#[test]
fn the_longest_word_plus_its_preview_fits_the_panel() {
    let longest = longest_word();

    // Stop one letter short, so a preview letter is still on offer to the right
    // of the word — that is what makes this the widest line, not just the word.
    let mut entry = WordEntry::new();
    spell(&mut entry, &longest[..longest.len() - 1]);
    assert!(
        entry.selected().is_some(),
        "expected a preview letter after {:?}",
        entry.current(),
    );

    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Words(&entry));

    display.assert_within_panel("the longest word with a preview");
}

/// The header carries the previously accepted word in the top-right corner, so
/// a full-length one there is its own width case.
#[test]
fn a_word_screen_carrying_the_longest_previous_word_fits_the_panel() {
    let mut entry = WordEntry::new();
    spell(&mut entry, longest_word());
    assert!(entry.press(Action::Confirm));
    assert!(!entry.rejected(), "the longest word was refused");

    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Words(&entry));

    display.assert_within_panel("a word screen showing the previous word");
}

#[test]
fn the_wordlist_screen_fits_the_panel() {
    let longest = longest_word();
    let mnemonic: Mnemonic = [longest; WORD_COUNT_TOTAL];

    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Phrase(&mnemonic));

    display.assert_within_panel("the wordlist screen");
}

/// The alphabet strip is 26 cells on one line, the longest fixed run any screen
/// draws, and the reason `BODY_FONTS` carries faces narrower than Courier at
/// all. Checked against the phrase's last word, where the strip is at its
/// widest because every letter is still live.
#[test]
fn the_alphabet_strip_fits_the_panel() {
    let mut entry = WordEntry::new();
    for _ in 0..WORD_COUNT - 1 {
        spell(&mut entry, "abandon");
        assert!(entry.press(Action::Confirm));
    }

    let mut display = Recorder::new(PANEL);
    render(&mut display, &View::Words(&entry));

    display.assert_within_panel("the alphabet strip on the last word");
}

/// Every entry is drawn in one face chosen from the longest label, so the
/// cursor's position changes what is accented but not what is measured. Checked
/// at each position anyway: the cursor is drawn from its own column, and it is
/// the one thing that moves.
#[test]
fn the_menu_screen_fits_the_panel_at_every_cursor_position() {
    for selected in MenuItem::ALL {
        let mut display = Recorder::new(PANEL);
        render(&mut display, &View::Menu { selected });

        display.assert_within_panel("the menu screen");
    }
}

/// The row is drawn from a fixed pitch, so it is the same width at every count;
/// what changes is which cells carry a glyph. Checked empty and full because the
/// header counter and the hint line both change with it, and the full row is the
/// one where every cell is inked.
#[test]
fn a_coin_screen_fits_the_panel_at_every_count() {
    let mut flips = Flips::new();

    for count in 0..=FLIP_COUNT {
        assert_eq!(flips.count(), count);

        let mut display = Recorder::new(PANEL);
        render(&mut display, &View::Coin(&flips));

        display.assert_within_panel("the coin screen");

        // Alternating, so both glyphs are measured: `H` and `T` need not be the
        // same width in a proportional face.
        flips.record(if count % 2 == 0 {
            Flip::Heads
        } else {
            Flip::Tails
        });
    }
}

/// The about screen composes its lines at runtime, and the version is the one
/// part not fixed at compile time — so it is measured with a longer one than
/// the firmware carries today.
#[test]
fn the_about_screen_fits_the_panel() {
    for version in ["0.1.0", "10.20.30-rc1"] {
        let mut display = Recorder::new(PANEL);
        render(&mut display, &View::About { version });

        display.assert_within_panel("the about screen");
    }
}

/// The tests above are only worth having if overflow is something [`Recorder`]
/// can actually see. It would not be, if u8g2-fonts clipped to the target's
/// bounds the way a real driver does — every screen would then pass by
/// construction and the suite would quietly mean nothing.
///
/// So: shrink the panel until the fixed offsets in `word.rs` cannot hold, and
/// require the check to notice. This asserts the check has teeth, not that a
/// 128x64 panel is supported — the layout constants are tuned to one display,
/// and adapting them is a separate job from measuring them.
#[test]
fn the_panel_check_catches_a_screen_that_does_not_fit() {
    let mut display = Recorder::new(Size::new(128, 64));
    render(&mut display, &View::Words(&WordEntry::new()));

    assert!(
        !display.within_panel(),
        "the word screen fitted a panel it is not laid out for, \
         so this suite can no longer tell a fit from an overflow: {:?}",
        display.ink(),
    );
}
