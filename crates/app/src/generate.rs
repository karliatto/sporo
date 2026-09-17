use sporo_core::{
    bip39::{self, Mnemonic},
    flips::{Flip, Flips},
};

use crate::{action::Action, view::View, word_entry::WordEntry};

#[derive(Clone)]
pub(crate) struct Generate {
    words: WordEntry,
    /// Kept for the life of the phrase, not per visit to the coin screen.
    /// Going back from the phrase to check a word and accepting it again lands
    /// on the same seven flips, so the final word does not change under a user
    /// who has already written it down — and they can see that it did not.
    flips: Flips,
    step: Step,
}

#[derive(Clone)]
enum Step {
    Words,
    Coin,
    /// Derived on the way in rather than on every redraw, and held only while
    /// it is on screen: leaving this step drops it.
    Phrase(Mnemonic),
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
    pub(crate) fn new() -> Self {
        Self {
            words: WordEntry::new(),
            flips: Flips::new(),
            step: Step::Words,
        }
    }

    pub(crate) fn press(&mut self, action: Action) -> Outcome {
        let changed = match self.step {
            Step::Words => {
                if action == Action::Back && self.words.is_empty() {
                    return Outcome::Exit;
                }

                let changed = self.words.press(action);

                // The eleventh word is the last the keypad can spell. The seven
                // bits the twelfth carries come off the coin screen, which opens
                // here.
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
                        let mnemonic = bip39::complete(self.words.accepted(), entropy)
                            .expect("every accepted word was resolved against the wordlist");
                        self.step = Step::Phrase(mnemonic);

                        true
                    }
                    None => false,
                },
                _ => false,
            },
            // Nothing to pick here; `Back` reopens the last word for editing,
            // exactly as it does from the coin screen.
            Step::Phrase(_) => {
                if action == Action::Back {
                    self.reopen_last_word();
                }

                action == Action::Back
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
            Step::Phrase(mnemonic) => View::Phrase(mnemonic),
        }
    }

    /// Steps back from past the words to the last of them, open for editing.
    ///
    /// Opening it, rather than showing the word screen at 11/11 with no letter
    /// on offer, is what keeps the step back from being a dead end.
    fn reopen_last_word(&mut self) {
        self.words.delete();
        self.step = Step::Words;
    }
}
