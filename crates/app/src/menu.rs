//! The hub the device returns to between workflows.
//!
//! State only: which entry the cursor is on, and what an action does to it.
//! What each entry is *called* on the panel is the screen's business.

use sporo_core::bip39::SeedLength;

use crate::action::Action;

/// What the menu can be asked to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    /// The word-entry flow for a phrase of the given length: every word but the
    /// last typed, a coin flipped per bit the last word carries, and the last
    /// word derived from both — eleven words and seven flips for 12, twenty-three
    /// and three for 24.
    GenerateMnemonic(SeedLength),
    /// Firmware version and the shape of the phrase it builds.
    About,
}

impl MenuItem {
    /// Every entry, in the order they are drawn and walked.
    pub const ALL: [Self; 3] = [
        Self::GenerateMnemonic(SeedLength::Words12),
        Self::GenerateMnemonic(SeedLength::Words24),
        Self::About,
    ];
}

/// What an action did, for the caller to act on.
///
/// Richer than the `bool` [`crate::word_entry::WordEntry::press`] returns,
/// because a menu has two ways out as well as a redraw. [`Self::Ignored`] serves
/// the same purpose as that `bool`: it keeps the full-screen blit off actions
/// that changed nothing.
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

#[derive(Clone)]
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

    /// Applies an action.
    ///
    /// The cursor wraps at both ends, as it does on the alphabet strip: with
    /// only a couple of entries, stopping dead at the last one is a worse
    /// surprise than coming back around.
    pub fn press(&mut self, action: Action) -> MenuEvent {
        match action {
            Action::Up => {
                self.cursor = (self.cursor + MenuItem::ALL.len() - 1) % MenuItem::ALL.len();

                MenuEvent::Moved
            }
            Action::Down => {
                self.cursor = (self.cursor + 1) % MenuItem::ALL.len();

                MenuEvent::Moved
            }
            Action::Select => MenuEvent::Chose(self.selected()),
            Action::Back => MenuEvent::Dismissed,
            _ => MenuEvent::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_menu_starts_on_the_first_entry() {
        assert_eq!(Menu::new().selected(), MenuItem::ALL[0]);
    }

    #[test]
    fn the_cursor_wraps_forwards() {
        let mut menu = Menu::new();

        assert_eq!(menu.press(Action::Down), MenuEvent::Moved);
        assert_eq!(
            menu.selected(),
            MenuItem::GenerateMnemonic(SeedLength::Words24)
        );
        menu.press(Action::Down);
        assert_eq!(menu.selected(), MenuItem::About);

        // Past the last entry is the first again, not a dead end.
        assert_eq!(menu.press(Action::Down), MenuEvent::Moved);
        assert_eq!(
            menu.selected(),
            MenuItem::GenerateMnemonic(SeedLength::Words12)
        );
    }

    #[test]
    fn the_cursor_wraps_backwards() {
        let mut menu = Menu::new();

        assert_eq!(menu.press(Action::Up), MenuEvent::Moved);
        assert_eq!(menu.selected(), MenuItem::About);
    }

    #[test]
    fn select_reports_the_entry_under_the_cursor() {
        let mut menu = Menu::new();
        for item in MenuItem::ALL {
            assert_eq!(menu.press(Action::Select), MenuEvent::Chose(item));
            menu.press(Action::Down);
        }
    }

    /// Choosing does not move the cursor: coming back from a screen should land
    /// on the entry that opened it.
    #[test]
    fn choosing_leaves_the_cursor_where_it_was() {
        let mut menu = Menu::new();
        menu.press(Action::Up);
        menu.press(Action::Select);

        assert_eq!(menu.selected(), MenuItem::About);
    }

    #[test]
    fn back_dismisses_the_menu() {
        assert_eq!(Menu::new().press(Action::Back), MenuEvent::Dismissed);
    }

    /// Every other action has to be inert, or the screen repaints for nothing.
    #[test]
    fn actions_the_menu_does_not_use_are_ignored() {
        let mut menu = Menu::new();

        for action in [
            Action::Left,
            Action::Right,
            Action::Confirm,
            Action::Heads,
            Action::Tails,
        ] {
            assert_eq!(
                menu.press(action),
                MenuEvent::Ignored,
                "{action:?} did something"
            );
        }

        assert_eq!(menu.selected(), MenuItem::ALL[0]);
    }

    /// The cursor is walked over `MenuItem::ALL`'s length, so the two must agree.
    #[test]
    fn the_cursor_reaches_every_entry() {
        let mut menu = Menu::new();

        for item in MenuItem::ALL {
            assert_eq!(menu.selected(), item);
            menu.press(Action::Down);
        }

        assert_eq!(menu.selected(), MenuItem::ALL[0], "the walk did not wrap");
    }
}
