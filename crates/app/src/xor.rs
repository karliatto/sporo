use sporo_core::{
    bip39::{self, Mnemonic, SeedLength},
    flips::{Flip, Flips},
};

use crate::{
    action::Action,
    view::{self, View},
    word_entry::WordEntry,
    workflow::Outcome,
};

/// The XOR workflow: two phrases the user already has, typed in full, combined
/// into a third.
///
/// Every word but the last is the XOR of the two inputs at that position. The
/// last cannot be, because it carries the checksum over everything before it, so
/// it is derived the way the generate workflow derives its own — from coin
/// flips, on the same screen, with the same counts.
///
/// **The result is not reversible**, and this tool is not seed splitting: the
/// final word's entropy comes from the flips rather than from the inputs, so
/// XORing the result back against one input does not return the other. See
/// [`bip39::xor`].
///
/// Both phrases are typed in full, final word and all, even though that word's
/// entropy is discarded. It is what makes the checksum checkable, and the
/// checksum is the only way the device can tell a mistyped phrase from a real
/// one — without it, one wrong word gives an answer just as plausible-looking as
/// the right one.
#[derive(Clone)]
pub(crate) struct Xor {
    /// Set by the first step, and what both entries and the flips are sized
    /// from. Meaningless until [`Step::Length`] is left, and nothing reads it
    /// before then.
    length: SeedLength,
    /// The two phrases as typed, kept for the life of the workflow rather than
    /// per step. Keeping both is what lets `Back` walk all the way out one word
    /// at a time: there is always an entry to reopen. Parsed on demand instead
    /// of stored twice, so a phrase lives in exactly one place.
    entries: [WordEntry; 2],
    /// Which of [`Self::entries`] is being typed, and so which label shows.
    typing: usize,
    /// Kept across visits to the coin screen for the reason the generate
    /// workflow keeps its own: going back to check a word and returning lands on
    /// the same flips, so the final word does not change under a user who has
    /// already written it down.
    flips: Flips,
    step: Step,
}

/// Large for the same reason `Generate`'s is — room for a 24-word phrase, with
/// no allocator to box it into. See the note there.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum Step {
    /// Before anything is typed: 12 or 24. The generate workflow takes this from
    /// the menu; this one cannot, because the menu has no room for two more
    /// entries on a 135-pixel panel.
    Length {
        selected: SeedLength,
    },
    /// Typing one of the two phrases. `invalid` marks a complete phrase whose
    /// final word does not match the rest; the words stay put so the user can
    /// back up to the one that is wrong.
    Words {
        invalid: bool,
    },
    Coin,
    /// Derived on the way in rather than on every redraw, and held only while it
    /// is on screen: leaving this step drops it.
    Phrase {
        mnemonic: Mnemonic,
        page: usize,
    },
}

/// Which phrase is being typed, shown beside the word count.
const LABELS: [&str; 2] = ["A", "B"];

/// Shown when a phrase is complete but its final word does not match the rest.
/// It names the phrase rather than a word, because any one of them could be the
/// one mistyped.
const INVALID_TEXT: &str = "phrase does not check out";

impl Xor {
    pub(crate) fn new() -> Self {
        let length = SeedLength::Words12;

        Self {
            length,
            entries: [WordEntry::full(length), WordEntry::full(length)],
            typing: 0,
            flips: Flips::new(length),
            step: Step::Length { selected: length },
        }
    }

    pub(crate) fn press(&mut self, action: Action) -> Outcome {
        match &self.step {
            Step::Length { .. } => self.press_length(action),
            Step::Words { .. } => self.press_words(action),
            Step::Coin => self.press_coin(action),
            Step::Phrase { .. } => self.press_phrase(action),
        }
    }

    fn press_length(&mut self, action: Action) -> Outcome {
        let Step::Length { selected } = &mut self.step else {
            return Outcome::Unchanged;
        };

        match action {
            Action::Back => Outcome::Exit,
            // Two entries, so either key is the other one: the wrap the menu
            // cursor has, with nothing in between.
            Action::Up | Action::Down => {
                *selected = match *selected {
                    SeedLength::Words12 => SeedLength::Words24,
                    SeedLength::Words24 => SeedLength::Words12,
                };

                Outcome::Redraw
            }
            Action::Select => {
                let length = *selected;
                self.start(length);

                Outcome::Redraw
            }
            _ => Outcome::Unchanged,
        }
    }

    /// Sizes both entries and the flips to `length` and opens the first phrase.
    ///
    /// Also what makes leaving the picker and coming back forget whatever was
    /// typed at the old length, which it must: a 12-word entry holds words a
    /// 24-word phrase has no room for.
    fn start(&mut self, length: SeedLength) {
        self.length = length;
        self.entries = [WordEntry::full(length), WordEntry::full(length)];
        self.typing = 0;
        self.flips = Flips::new(length);
        self.step = Step::Words { invalid: false };
    }

    fn press_words(&mut self, action: Action) -> Outcome {
        let Step::Words { invalid } = &mut self.step else {
            return Outcome::Unchanged;
        };

        // A refused phrase is complete: there is no letter to add and no word to
        // accept, so the only way on is back to the last word, which is where
        // the mistake has to be corrected.
        if *invalid {
            if action != Action::Back {
                return Outcome::Unchanged;
            }

            *invalid = false;
            self.entries[self.typing].delete();

            return Outcome::Redraw;
        }

        if action == Action::Back && self.entries[self.typing].is_empty() {
            // One step at a time, as `Back` is everywhere else: out of the
            // second phrase into the last word of the first, and out of the
            // first back to the length.
            if self.typing == 0 {
                self.step = Step::Length {
                    selected: self.length,
                };
            } else {
                self.typing = 0;
                self.entries[0].delete();
            }

            return Outcome::Redraw;
        }

        let changed = self.entries[self.typing].press(action);

        if changed && self.entries[self.typing].is_complete() {
            // The final word is read for its checksum alone — its entropy bits
            // are replaced by the flips — so this is the one chance to catch a
            // phrase that was typed wrong.
            if bip39::parse(self.length, self.entries[self.typing].accepted()).is_none() {
                self.step = Step::Words { invalid: true };
            } else if self.typing == 0 {
                self.typing = 1;
            } else {
                self.step = Step::Coin;
            }
        }

        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    /// Identical to the generate workflow's coin step, deliberately: the final
    /// word is derived the same way, so it is entered the same way.
    fn press_coin(&mut self, action: Action) -> Outcome {
        let changed = match action {
            Action::Heads => self.flips.record(Flip::Heads),
            Action::Tails => self.flips.record(Flip::Tails),
            // Backspace first, and step back only once there is nothing left to
            // take back.
            Action::Back => {
                if !self.flips.undo() {
                    self.reopen_last_word();
                }

                true
            }
            Action::Confirm => match self.flips.entropy() {
                Some(entropy) => {
                    let mnemonic = bip39::xor(&self.parsed(0), &self.parsed(1), entropy)
                        .expect("both phrases were read at the same length");
                    self.step = Step::Phrase { mnemonic, page: 0 };

                    true
                }
                None => false,
            },
            _ => false,
        };

        if changed {
            Outcome::Redraw
        } else {
            Outcome::Unchanged
        }
    }

    fn press_phrase(&mut self, action: Action) -> Outcome {
        let Step::Phrase { mnemonic, page } = &mut self.step else {
            return Outcome::Unchanged;
        };

        let pages = view::phrase_pages(mnemonic);

        match action {
            Action::Back => {
                self.reopen_last_word();

                Outcome::Redraw
            }
            // Wraps, as the menu cursor does: with two pages, either key is the
            // other page.
            Action::Left | Action::Right if pages > 1 => {
                *page = if action == Action::Right {
                    (*page + 1) % pages
                } else {
                    (*page + pages - 1) % pages
                };

                Outcome::Redraw
            }
            _ => Outcome::Unchanged,
        }
    }

    /// One of the two phrases, as typed.
    ///
    /// Re-parsed rather than kept: both entries were parsed successfully to get
    /// past the word screen, and neither can change without going back through
    /// it, so this cannot fail. Doing it this way keeps each phrase in one place
    /// rather than two.
    fn parsed(&self, index: usize) -> Mnemonic {
        bip39::parse(self.length, self.entries[index].accepted())
            .expect("a phrase is only left once it has parsed")
    }

    /// Steps back from past the phrases to the last word of the second, open for
    /// editing — the move `Generate::reopen_last_word` makes.
    ///
    /// The first phrase is kept and the second is reopened at its last word
    /// rather than cleared: the user came back to change something, and losing
    /// two phrases they typed correctly is not that.
    fn reopen_last_word(&mut self) {
        self.typing = 1;
        self.entries[1].delete();
        self.step = Step::Words { invalid: false };
    }

    pub(crate) fn view(&self) -> View<'_> {
        match &self.step {
            Step::Length { selected } => View::SeedLengthPick {
                selected: *selected,
            },
            Step::Words { invalid } => View::Words {
                entry: &self.entries[self.typing],
                label: Some(LABELS[self.typing]),
                notice: invalid.then_some(INVALID_TEXT),
            },
            Step::Coin => View::Coin(&self.flips),
            Step::Phrase { mnemonic, page } => View::Phrase {
                mnemonic,
                page: *page,
            },
        }
    }
}
