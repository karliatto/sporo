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
                MenuEvent::Chose(MenuItem::GenerateMnemonic) => {
                    self.screen = Screen::Generate(Generate::new());

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
        bip39::{Mnemonic, WORD_COUNT},
        flips::{Flips, FLIP_COUNT},
    };

    use crate::word_entry::WordEntry;

    const VERSION: &str = "1.2.3";

    /// A name for the screen on show, for assertion messages: `View` is not
    /// `Debug`, on purpose.
    fn screen_name(app: &App) -> &'static str {
        match app.view() {
            View::Home => "home",
            View::Menu { .. } => "menu",
            View::About { .. } => "about",
            View::Words(_) => "words",
            View::Coin(_) => "coin",
            View::Phrase(_) => "phrase",
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
            View::Phrase(mnemonic) => *mnemonic,
            _ => panic!("expected the phrase, on {}", screen_name(app)),
        }
    }

    /// From a new app, through the menu, onto an empty word screen.
    fn open_generate() -> App {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        assert_eq!(selected(&app), MenuItem::GenerateMnemonic);
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

    fn at_the_coin_screen() -> App {
        let mut app = open_generate();
        for _ in 0..WORD_COUNT {
            spell_word(&mut app, "abandon");
        }

        app
    }

    /// Eleven "abandon"s and `pattern` as flips — `1` heads, `0` tails —
    /// confirmed onto the phrase.
    fn at_the_phrase(pattern: &str) -> App {
        let mut app = at_the_coin_screen();
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

        assert_eq!(selected(&app), MenuItem::GenerateMnemonic);
    }

    #[test]
    fn keys_bound_to_nothing_do_nothing_past_home() {
        let mut menu = App::new(VERSION);
        press(&mut menu, Action::Select);

        let mut about = menu.clone();
        press(&mut about, Action::Down);
        press(&mut about, Action::Select);

        for (mut app, expected) in [
            (menu, "menu"),
            (about, "about"),
            (open_generate(), "words"),
            (at_the_coin_screen(), "coin"),
            (at_the_phrase("0000000"), "phrase"),
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
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        press(&mut app, Action::Down);
        press(&mut app, Action::Select);
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

        assert_eq!(selected(&app), MenuItem::About);
    }

    #[test]
    fn back_on_about_returns_to_the_menu() {
        let mut app = App::new(VERSION);
        press(&mut app, Action::Select);
        press(&mut app, Action::Down);
        press(&mut app, Action::Select);

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
    fn choosing_generate_opens_an_empty_word_screen() {
        let app = open_generate();

        assert!(words(&app).is_empty());
        assert_eq!(words(&app).selected(), Some('A'));
    }

    #[test]
    fn back_on_an_empty_first_word_returns_to_the_menu() {
        let mut app = open_generate();

        assert!(press(&mut app, Action::Back));
        assert_eq!(screen_name(&app), "menu");
    }

    #[test]
    fn back_with_a_letter_typed_deletes_it_rather_than_leaving() {
        let mut app = open_generate();
        press(&mut app, Action::Select);

        assert!(press(&mut app, Action::Back));
        assert_eq!(screen_name(&app), "words");
        assert!(words(&app).is_empty());
    }

    #[test]
    fn accepting_the_eleventh_word_opens_the_coin_screen() {
        let mut app = open_generate();
        for _ in 0..WORD_COUNT - 1 {
            spell_word(&mut app, "abandon");
        }
        assert_eq!(screen_name(&app), "words");

        spell_word(&mut app, "abandon");

        assert_eq!(flips(&app).count(), 0);
    }

    #[test]
    fn confirm_is_inert_until_the_seventh_flip() {
        let mut app = at_the_coin_screen();

        for _ in 0..FLIP_COUNT {
            assert!(!press(&mut app, Action::Confirm));
            assert_eq!(screen_name(&app), "coin");
            press(&mut app, Action::Heads);
        }

        assert!(press(&mut app, Action::Confirm));
        assert_eq!(screen_name(&app), "phrase");
    }

    #[test]
    fn a_full_row_takes_no_more_flips() {
        let mut app = at_the_coin_screen();
        for _ in 0..FLIP_COUNT {
            press(&mut app, Action::Heads);
        }

        assert!(!press(&mut app, Action::Tails));
        assert_eq!(flips(&app).entropy(), Some(0b111_1111));
    }

    #[test]
    fn confirming_the_flips_shows_the_completed_phrase() {
        // BIP-39's own vector: eleven "abandon" and seven zero bits end in
        // "about".
        let app = at_the_phrase("0000000");

        let mnemonic = phrase(&app);
        assert_eq!(mnemonic[..WORD_COUNT], ["abandon"; WORD_COUNT]);
        assert_eq!(mnemonic[WORD_COUNT], "about");
    }

    #[test]
    fn back_on_the_phrase_reopens_the_eleventh_word() {
        let mut app = at_the_phrase("0000000");

        assert!(press(&mut app, Action::Back));

        assert_eq!(words(&app).current(), "ABANDON");
        assert_eq!(words(&app).accepted().len(), WORD_COUNT - 1);
    }

    #[test]
    fn back_on_a_coin_screen_with_no_flips_reopens_the_eleventh_word() {
        let mut app = at_the_coin_screen();

        assert!(press(&mut app, Action::Back));

        assert_eq!(words(&app).current(), "ABANDON");
    }

    #[test]
    fn back_with_flips_in_takes_one_back_rather_than_leaving() {
        let mut app = at_the_coin_screen();
        press(&mut app, Action::Heads);
        press(&mut app, Action::Tails);

        assert!(press(&mut app, Action::Back));

        assert_eq!(flips(&app).count(), 1);
    }

    #[test]
    fn returning_from_the_phrase_keeps_the_flips() {
        let mut app = at_the_phrase("1011010");
        press(&mut app, Action::Back);
        press(&mut app, Action::Confirm);

        // The same seven flips are on screen, so the user can see the final
        // word is not about to change under them.
        assert_eq!(flips(&app).entropy(), Some(0b101_1010));
    }

    #[test]
    fn a_phrase_rebuilt_after_an_edit_is_the_same_phrase() {
        let mut app = at_the_phrase("1011010");
        let first = phrase(&app);

        press(&mut app, Action::Back);
        press(&mut app, Action::Confirm);
        press(&mut app, Action::Confirm);

        assert_eq!(phrase(&app), first);
    }

    #[test]
    fn backing_out_to_the_menu_forgets_the_flips() {
        let mut app = at_the_phrase("1011010");

        // All the way out: reopen the eleventh word, then delete every letter of
        // every word until `Back` has nothing left and leaves.
        for _ in 0..1_000 {
            if screen_name(&app) == "menu" {
                break;
            }
            press(&mut app, Action::Back);
        }
        assert_eq!(screen_name(&app), "menu");

        press(&mut app, Action::Select);
        for _ in 0..WORD_COUNT {
            spell_word(&mut app, "abandon");
        }

        // A new phrase gets new flips. Before workflows owned their state, the
        // old seven were still here and confirmed straight into the new phrase.
        assert_eq!(flips(&app).count(), 0);
    }

    #[test]
    fn generating_again_starts_from_a_fresh_phrase() {
        let mut app = open_generate();
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
        let mut about = App::new(VERSION);
        press(&mut about, Action::Select);
        press(&mut about, Action::Down);
        press(&mut about, Action::Select);

        for mut app in [
            App::new(VERSION),
            about,
            open_generate(),
            at_the_coin_screen(),
            at_the_phrase("0000000"),
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
        assert_eq!(selected(&app), MenuItem::GenerateMnemonic);
    }

    #[test]
    fn reset_mid_phrase_forgets_the_words_and_the_flips() {
        let mut app = at_the_coin_screen();
        press(&mut app, Action::Heads);

        app.reset();
        press(&mut app, Action::Select);
        press(&mut app, Action::Select);

        assert!(words(&app).is_empty());
    }

    #[test]
    fn a_key_that_changes_nothing_asks_for_no_redraw() {
        // Every redraw is a full-screen blit, so these matter on the device.
        let mut app = open_generate();
        spell(&mut app, "abando");

        // "abando" continues only into "abandon", so the cursor has nowhere to
        // go.
        assert!(!press(&mut app, Action::Right));
        assert!(!press(&mut app, Action::Heads));

        let mut coin = at_the_coin_screen();
        assert!(!press(&mut coin, Action::Select));
    }
}
