use sporo_core::{
    bip39::{self, Mnemonic, SeedLength},
    flips::{Flip, Flips},
};

use crate::{
    action::Action,
    view::{self, View},
    word_entry::WordEntry,
};

/// The generate workflow: every word but the last typed, a coin flipped for
/// each bit of entropy the last word carries, and that word derived from both.
/// The same steps for every [`SeedLength`]; only the counts differ.
#[derive(Clone)]
pub(crate) struct Generate {
    length: SeedLength,
    words: WordEntry,
    /// Kept for the life of the phrase, not per visit to the coin screen.
    /// Going back from the phrase to check a word and accepting it again lands
    /// on the same flips, so the final word does not change under a user who
    /// has already written it down — and they can see that it did not.
    flips: Flips,
    step: Step,
}

/// `Phrase` is far larger than the other variants, room enough for 24 words,
/// which is what `large_enum_variant` warns about. Boxing it needs an allocator
/// this device does not have, and the phrase has to be held somewhere while it
/// is on screen — see the same trade-off on `Screen` in `app.rs`.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum Step {
    Words,
    Coin,
    /// Derived on the way in rather than on every redraw, and held only while
    /// it is on screen: leaving this step drops it.
    Phrase {
        mnemonic: Mnemonic,
        page: usize,
    },
}

/// What an action did to the workflow, for the application to act on.
pub(crate) enum Outcome {
    /// Nothing changed; the screen does not need drawing again.
    Unchanged,
    Redraw,
    /// `Back` with nothing left to take back: the user left the workflow.
    Exit,
}

impl Generate {
    pub(crate) fn new(length: SeedLength) -> Self {
        Self {
            length,
            words: WordEntry::new(length),
            flips: Flips::new(length),
            step: Step::Words,
        }
    }

    pub(crate) fn press(&mut self, action: Action) -> Outcome {
        let changed = match &mut self.step {
            Step::Words => {
                if action == Action::Back && self.words.is_empty() {
                    return Outcome::Exit;
                }

                let changed = self.words.press(action);

                // The second-to-last word is the last the keypad can spell.
                // The bits the final word carries come off the coin screen,
                // which opens here.
                if changed && self.words.is_complete() {
                    self.step = Step::Coin;
                }

                changed
            }
            Step::Coin => match action {
                Action::Heads => self.flips.record(Flip::Heads),
                Action::Tails => self.flips.record(Flip::Tails),
                // Backspace first, and step back only once there is nothing
                // left to take back — the way `Back` walks out of a word one
                // letter at a time rather than abandoning it in one press.
                Action::Back => {
                    if !self.flips.undo() {
                        self.reopen_last_word();
                    }

                    true
                }
                Action::Confirm => match self.flips.entropy() {
                    Some(entropy) => {
                        let mnemonic = bip39::complete(self.length, self.words.accepted(), entropy)
                            .expect("every accepted word was resolved against the wordlist");
                        self.step = Step::Phrase { mnemonic, page: 0 };

                        true
                    }
                    None => false,
                },
                _ => false,
            },
            // `Left` and `Right` turn the page on a phrase too long for one;
            // `Back` reopens the last word for editing, exactly as it does from
            // the coin screen.
            Step::Phrase { mnemonic, page } => {
                let pages = view::phrase_pages(mnemonic);

                match action {
                    Action::Back => {
                        self.reopen_last_word();

                        true
                    }
                    // Wraps, as the menu cursor does: with two pages, either
                    // key is the other page.
                    Action::Left | Action::Right if pages > 1 => {
                        *page = if action == Action::Right {
                            (*page + 1) % pages
                        } else {
                            (*page + pages - 1) % pages
                        };

                        true
                    }
                    _ => false,
                }
            }
        };

        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    pub(crate) fn view(&self) -> View<'_> {
        match &self.step {
            Step::Words => View::Words(&self.words),
            Step::Coin => View::Coin(&self.flips),
            Step::Phrase { mnemonic, page } => View::Phrase {
                mnemonic,
                page: *page,
            },
        }
    }

    /// Steps back from past the words to the last of them, open for editing.
    ///
    /// Opening it, rather than showing the word screen with every word in and
    /// no letter on offer, is what keeps the step back from being a dead end.
    fn reopen_last_word(&mut self) {
        self.words.delete();
        self.step = Step::Words;
    }
}
