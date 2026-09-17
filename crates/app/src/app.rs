use crate::{
    action::Action,
    generate::{Generate, Outcome},
    menu::{Menu, MenuEvent, MenuItem},
    view::View,
};

#[derive(Clone)]
pub struct App {
    version: &'static str,
    /// Hub state, outliving every screen: coming back from About lands on the
    /// entry that opened it.
    menu: Menu,
    screen: Screen,
}

/// The open screen, carrying the state of any workflow behind it.
///
/// Holding a workflow's state inside its variant is what makes leaving the
/// workflow forget it: there is nowhere else for that state to survive.
///
/// `Generate` is far larger than the other variants, which is what
/// `large_enum_variant` warns about. Its fix is to box the variant, which needs
/// an allocator this device does not have; and exactly one `App` exists, so the
/// bytes it would save are bytes the open workflow needs anyway.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum Screen {
    Home,
    Menu,
    About,
    Generate(Generate),
}

impl App {
    pub const fn new(version: &'static str) -> Self {
        Self {
            version,
            menu: Menu::new(),
            screen: Screen::Home,
        }
    }

    /// Applies a keypress. `None` is a key bound to no action. Returns whether
    /// the screen needs drawing again, so a press that changed nothing costs no
    /// full-screen blit.
    #[must_use]
    pub fn press(&mut self, action: Option<Action>) -> bool {
        // Any key leaves the home screen — including the ones bound to nothing,
        // since the screen says "press any key" — and only leaves it:
        // swallowing the press keeps it from also nudging the menu cursor off
        // the first entry.
        if let Screen::Home = self.screen {
            self.screen = Screen::Menu;

            return true;
        }

        // Past the home screen a key bound to nothing does nothing.
        let Some(action) = action else {
            return false;
        };

        match &mut self.screen {
            Screen::Home => unreachable!("handled above"),
            Screen::Menu => match self.menu.press(action) {
                MenuEvent::Ignored => false,
                MenuEvent::Moved => true,
                MenuEvent::Chose(MenuItem::GenerateMnemonic(length)) => {
                    self.screen = Screen::Generate(Generate::new(length));

                    true
                }
                MenuEvent::Chose(MenuItem::About) => {
                    self.screen = Screen::About;

                    true
                }
                MenuEvent::Dismissed => {
                    self.screen = Screen::Home;

                    true
                }
            },
            // Nothing to pick; `Back` is the way back, as it is everywhere else.
            Screen::About => {
                if action == Action::Back {
                    self.screen = Screen::Menu;
                }

                action == Action::Back
            }
            Screen::Generate(generate) => match generate.press(action) {
                Outcome::Unchanged => false,
                Outcome::Redraw => true,
                Outcome::Exit => {
                    self.screen = Screen::Menu;

                    true
                }
            },
        }
    }

    /// Starts over from nothing — the board button. The caller always redraws.
    ///
    /// A new `App` rather than a list of fields to clear, so nothing added later
    /// can be forgotten by it.
    pub fn reset(&mut self) {
        *self = Self::new(self.version);
    }

    pub fn view(&self) -> View<'_> {
        match &self.screen {
            Screen::Home => View::Home,
            Screen::Menu => View::Menu {
                selected: self.menu.selected(),
            },
            Screen::About => View::About {
                version: self.version,
            },
            Screen::Generate(generate) => generate.view(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sporo_core::{
        bip39::{Mnemonic, SeedLength},
        flips::Flips,
    };

    use crate::word_entry::WordEntry;

    const VERSION: &str = "1.2.3";

    use SeedLength::{Words12, Words24};

    /// A name for the screen on show, for assertion messages: `View` is not
    /// `Debug`, on purpose.
    fn screen_name(app: &App) -> &'static str {
        match app.view() {
            View::Home => "home",
            View::Menu { .. } => "menu",
            View::About { .. } => "about",
            View::Words(_) => "words",
            View::Coin(_) => "coin",
            View::Phrase { .. } => "phrase",
        }
    }

    fn press(app: &mut App, action: Action) -> bool {
        app.press(Some(action))
    }

    fn selected(app: &App) -> MenuItem {
        match app.view() {
            View::Menu { selected } => selected,
            _ => panic!("expected the menu, on {}", screen_name(app)),
        }
    }

    fn words(app: &App) -> &WordEntry {
        match app.view() {
            View::Words(words) => words,
            _ => panic!("expected the word screen, on {}", screen_name(app)),
        }
    }

    fn flips(app: &App) -> &Flips {
        match app.view() {
            View::Coin(flips) => flips,
            _ => panic!("expected the coin screen, on {}", screen_name(app)),
        }
    }

    fn phrase(app: &App) -> Mnemonic {
        match app.view() {
            View::Phrase { mnemonic, .. } => *mnemonic,
            _ => panic!("expected the phrase, on {}", screen_name(app)),
        }
    }

    fn page(app: &App) -> usize {
        match app.view() {
            View::Phrase { page, .. } => page,
            _ => panic!("expected the phrase, on {}", screen_name(app)),
        }
    }

    /// From a new app, through the menu, onto the about screen. It is the last
    /// entry, so `Up` reaches it by wrapping.
    fn open_about() -> App {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        press(&mut app, Action::Up);
        assert_eq!(selected(&app), MenuItem::About);
        press(&mut app, Action::Select);

        app
    }

    /// From a new app, through the menu, onto an empty word screen for a phrase
    /// of `length`.
    fn open_generate(length: SeedLength) -> App {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        while selected(&app) != MenuItem::GenerateMnemonic(length) {
            press(&mut app, Action::Down);
        }
        press(&mut app, Action::Select);

        app
    }

    /// Types `letters` the way a user would: walking the letter cursor with
    /// `Right`, adding with `Select`.
    fn spell(app: &mut App, letters: &str) {
        for letter in letters.chars().map(|letter| letter.to_ascii_uppercase()) {
            for _ in 0..26 {
                if words(app).selected() == Some(letter) {
                    break;
                }
                press(app, Action::Right);
            }
            assert!(press(app, Action::Select), "{letter:?} was not added");
        }
    }

    fn spell_word(app: &mut App, word: &str) {
        spell(app, word);
        assert!(press(app, Action::Confirm), "{word:?} was not accepted");
    }

    fn at_the_coin_screen(length: SeedLength) -> App {
        let mut app = open_generate(length);
        for _ in 0..length.entered_words() {
            spell_word(&mut app, "abandon");
        }

        app
    }

    /// Every word but the last "abandon", and `pattern` as flips — `1` heads,
    /// `0` tails — confirmed onto the phrase. The pattern's length picks the
    /// phrase's: seven flips for 12 words, three for 24.
    fn at_the_phrase(pattern: &str) -> App {
        let length = SeedLength::ALL
            .into_iter()
            .find(|length| length.final_word_entropy_bits() == pattern.len())
            .expect("a pattern fills some phrase length's row");

        let mut app = at_the_coin_screen(length);
        for digit in pattern.chars() {
            press(
                &mut app,
                if digit == '1' {
                    Action::Heads
                } else {
                    Action::Tails
                },
            );
        }
        assert!(press(&mut app, Action::Confirm));

        app
    }

    /// An all-tails pattern for `length`.
    fn tails(length: SeedLength) -> &'static str {
        &"0000000"[..length.final_word_entropy_bits()]
    }

    #[test]
    fn a_new_app_shows_the_home_screen() {
        assert_eq!(screen_name(&App::new(VERSION)), "home");
    }

    #[test]
    fn any_key_leaves_home_for_the_menu() {
        for action in Action::ALL {
            let mut app = App::new(VERSION);

            assert!(press(&mut app, action));
            assert_eq!(screen_name(&app), "menu", "after {action:?}");
        }
    }

    #[test]
    fn a_key_bound_to_nothing_still_leaves_home() {
        // The home screen says "press any key", and three keys on the pad are
        // bound to no action at all.
        let mut app = App::new(VERSION);

        assert!(app.press(None));
        assert_eq!(screen_name(&app), "menu");
    }

    #[test]
    fn leaving_home_does_not_move_the_menu_cursor() {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Down);

        assert_eq!(selected(&app), MenuItem::ALL[0]);
    }

    #[test]
    fn keys_bound_to_nothing_do_nothing_past_home() {
        let mut menu = App::new(VERSION);
        press(&mut menu, Action::Select);

        for (mut app, expected) in [
            (menu, "menu"),
            (open_about(), "about"),
            (open_generate(Words12), "words"),
            (at_the_coin_screen(Words12), "coin"),
            (at_the_phrase(tails(Words12)), "phrase"),
            (at_the_phrase(tails(Words24)), "phrase"),
        ] {
            assert!(!app.press(None), "a redraw was asked for on {expected}");
            assert_eq!(screen_name(&app), expected);
        }
    }

    #[test]
    fn back_on_the_menu_returns_home() {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);

        assert!(press(&mut app, Action::Back));
        assert_eq!(screen_name(&app), "home");
    }

    #[test]
    fn the_menu_cursor_survives_a_visit_to_about() {
        let mut app = open_about();
        assert_eq!(screen_name(&app), "about");

        press(&mut app, Action::Back);

        // Coming back lands on the entry that opened the screen.
        assert_eq!(selected(&app), MenuItem::About);
    }

    #[test]
    fn the_menu_cursor_survives_a_trip_home() {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        press(&mut app, Action::Down);
        press(&mut app, Action::Back);
        press(&mut app, Action::Select);

        assert_eq!(selected(&app), MenuItem::ALL[1]);
    }

    #[test]
    fn back_on_about_returns_to_the_menu() {
        let mut app = open_about();

        match app.view() {
            View::About { version } => assert_eq!(version, VERSION),
            _ => panic!("expected about, on {}", screen_name(&app)),
        }

        // Nothing else on the about screen does anything.
        assert!(!press(&mut app, Action::Select));
        assert!(press(&mut app, Action::Back));
        assert_eq!(screen_name(&app), "menu");
    }

    #[test]
    fn choosing_generate_opens_an_empty_word_screen_for_that_length() {
        for length in SeedLength::ALL {
            let app = open_generate(length);

            assert!(words(&app).is_empty());
            assert_eq!(words(&app).selected(), Some('A'));
            assert_eq!(words(&app).word_count(), length.entered_words());
        }
    }

    #[test]
    fn back_on_an_empty_first_word_returns_to_the_menu() {
        for length in SeedLength::ALL {
            let mut app = open_generate(length);

            assert!(press(&mut app, Action::Back));
            assert_eq!(screen_name(&app), "menu");
            // On the entry that was chosen, so trying again is one press.
            assert_eq!(selected(&app), MenuItem::GenerateMnemonic(length));
        }
    }

    #[test]
    fn back_with_a_letter_typed_deletes_it_rather_than_leaving() {
        let mut app = open_generate(Words12);
        press(&mut app, Action::Select);

        assert!(press(&mut app, Action::Back));
        assert_eq!(screen_name(&app), "words");
        assert!(words(&app).is_empty());
    }

    #[test]
    fn accepting_the_second_to_last_word_opens_the_coin_screen() {
        for length in SeedLength::ALL {
            let mut app = open_generate(length);
            for _ in 0..length.entered_words() - 1 {
                spell_word(&mut app, "abandon");
            }
            assert_eq!(screen_name(&app), "words");

            spell_word(&mut app, "abandon");

            assert_eq!(flips(&app).count(), 0);
            assert_eq!(flips(&app).required(), length.final_word_entropy_bits());
        }
    }

    #[test]
    fn confirm_is_inert_until_the_last_flip() {
        for length in SeedLength::ALL {
            let mut app = at_the_coin_screen(length);

            for _ in 0..length.final_word_entropy_bits() {
                assert!(!press(&mut app, Action::Confirm));
                assert_eq!(screen_name(&app), "coin");
                press(&mut app, Action::Heads);
            }

            assert!(press(&mut app, Action::Confirm));
            assert_eq!(screen_name(&app), "phrase");
        }
    }

    #[test]
    fn a_full_row_takes_no_more_flips() {
        for length in SeedLength::ALL {
            let mut app = at_the_coin_screen(length);
            let required = length.final_word_entropy_bits();
            for _ in 0..required {
                press(&mut app, Action::Heads);
            }

            assert!(!press(&mut app, Action::Tails));
            assert_eq!(flips(&app).entropy(), Some((1 << required) - 1));
        }
    }

    #[test]
    fn confirming_the_flips_shows_the_completed_phrase() {
        // BIP-39's own vectors: eleven "abandon" and seven zero bits end in
        // "about"; twenty-three and three end in "art".
        for (length, expected) in [(Words12, "about"), (Words24, "art")] {
            let app = at_the_phrase(tails(length));

            let mnemonic = phrase(&app);
            let entered = length.entered_words();
            assert_eq!(mnemonic.words().len(), length.total_words());
            assert!(mnemonic.words()[..entered]
                .iter()
                .all(|word| *word == "abandon"));
            assert_eq!(mnemonic.final_word(), expected);
            assert_eq!(page(&app), 0);
        }
    }

    #[test]
    fn left_and_right_turn_the_page_of_a_24_word_phrase() {
        let mut app = at_the_phrase(tails(Words24));
        assert_eq!(page(&app), 0);

        assert!(press(&mut app, Action::Right));
        assert_eq!(page(&app), 1);

        // Two pages, so going on from the last comes back to the first.
        assert!(press(&mut app, Action::Right));
        assert_eq!(page(&app), 0);

        assert!(press(&mut app, Action::Left));
        assert_eq!(page(&app), 1);
        assert!(press(&mut app, Action::Left));
        assert_eq!(page(&app), 0);
    }

    #[test]
    fn a_12_word_phrase_has_no_page_to_turn() {
        let mut app = at_the_phrase(tails(Words12));

        // One page: the keys do nothing, and cost no redraw.
        assert!(!press(&mut app, Action::Right));
        assert!(!press(&mut app, Action::Left));
        assert_eq!(page(&app), 0);
    }

    #[test]
    fn back_on_the_phrase_reopens_the_last_entered_word() {
        for length in SeedLength::ALL {
            let mut app = at_the_phrase(tails(length));
            // From the second page too, not only the first.
            press(&mut app, Action::Right);

            assert!(press(&mut app, Action::Back));

            assert_eq!(words(&app).current(), "ABANDON");
            assert_eq!(words(&app).accepted().len(), length.entered_words() - 1);
        }
    }

    #[test]
    fn back_on_a_coin_screen_with_no_flips_reopens_the_last_entered_word() {
        for length in SeedLength::ALL {
            let mut app = at_the_coin_screen(length);

            assert!(press(&mut app, Action::Back));

            assert_eq!(words(&app).current(), "ABANDON");
        }
    }

    #[test]
    fn back_with_flips_in_takes_one_back_rather_than_leaving() {
        let mut app = at_the_coin_screen(Words12);
        press(&mut app, Action::Heads);
        press(&mut app, Action::Tails);

        assert!(press(&mut app, Action::Back));

        assert_eq!(flips(&app).count(), 1);
    }

    #[test]
    fn returning_from_the_phrase_keeps_the_flips() {
        for (pattern, expected) in [("1011010", 0b101_1010), ("101", 0b101)] {
            let mut app = at_the_phrase(pattern);
            press(&mut app, Action::Back);
            press(&mut app, Action::Confirm);

            // The same flips are on screen, so the user can see the final word
            // is not about to change under them.
            assert_eq!(flips(&app).entropy(), Some(expected));
        }
    }

    #[test]
    fn a_phrase_rebuilt_after_an_edit_is_the_same_phrase() {
        for pattern in ["1011010", "101"] {
            let mut app = at_the_phrase(pattern);
            let first = phrase(&app);
            press(&mut app, Action::Right);

            press(&mut app, Action::Back);
            press(&mut app, Action::Confirm);
            press(&mut app, Action::Confirm);

            assert_eq!(phrase(&app), first);
            // Rebuilt from the start, rather than on the page left.
            assert_eq!(page(&app), 0);
        }
    }

    #[test]
    fn backing_out_to_the_menu_forgets_the_flips() {
        let mut app = at_the_phrase("1011010");

        // All the way out: reopen the last word, then delete every letter of
        // every word until `Back` has nothing left and leaves.
        for _ in 0..1_000 {
            if screen_name(&app) == "menu" {
                break;
            }
            press(&mut app, Action::Back);
        }
        assert_eq!(screen_name(&app), "menu");

        press(&mut app, Action::Select);
        for _ in 0..Words12.entered_words() {
            spell_word(&mut app, "abandon");
        }

        // A new phrase gets new flips. Before workflows owned their state, the
        // old seven were still here and confirmed straight into the new phrase.
        assert_eq!(flips(&app).count(), 0);
    }

    #[test]
    fn generating_again_starts_from_a_fresh_phrase() {
        let mut app = open_generate(Words12);
        press(&mut app, Action::Right);
        press(&mut app, Action::Right);
        press(&mut app, Action::Back);
        assert_eq!(screen_name(&app), "menu");

        press(&mut app, Action::Select);

        // The letter cursor starts on A again, not wherever it was left.
        assert!(words(&app).is_empty());
        assert_eq!(words(&app).selected(), Some('A'));
    }

    #[test]
    fn reset_from_anywhere_shows_home() {
        for mut app in [
            App::new(VERSION),
            open_about(),
            open_generate(Words12),
            at_the_coin_screen(Words12),
            at_the_phrase(tails(Words12)),
            at_the_phrase(tails(Words24)),
        ] {
            app.reset();

            assert_eq!(screen_name(&app), "home");
        }
    }

    #[test]
    fn reset_puts_the_menu_cursor_back_on_the_first_entry() {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        press(&mut app, Action::Down);

        app.reset();
        press(&mut app, Action::Select);

        // The board button is "start over", and resuming on whatever was last
        // picked is not that.
        assert_eq!(selected(&app), MenuItem::ALL[0]);
    }

    #[test]
    fn reset_mid_phrase_forgets_the_words_and_the_flips() {
        let mut app = at_the_coin_screen(Words12);
        press(&mut app, Action::Heads);

        app.reset();
        press(&mut app, Action::Select);
        press(&mut app, Action::Select);

        assert!(words(&app).is_empty());
    }

    #[test]
    fn a_key_that_changes_nothing_asks_for_no_redraw() {
        // Every redraw is a full-screen blit, so these matter on the device.
        let mut app = open_generate(Words12);
        spell(&mut app, "abando");

        // "abando" continues only into "abandon", so the cursor has nowhere to
        // go.
        assert!(!press(&mut app, Action::Right));
        assert!(!press(&mut app, Action::Heads));

        let mut coin = at_the_coin_screen(Words12);
        assert!(!press(&mut coin, Action::Select));
    }
}
