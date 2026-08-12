//! State for the recovery-phrase entry screen.
//!
//! The keypad has no letter keys, so words are spelled out by walking a cursor
//! along the alphabet and confirming one letter at a time:
//!
//! | Key | Action                                             |
//! | --- | -------------------------------------------------- |
//! | `4` | move the cursor to the previous usable letter       |
//! | `6` | move the cursor to the next usable letter           |
//! | `5` | append the selected letter to the current word      |
//! | `*` | delete the last letter, or reopen the last word     |
//! | `#` | accept the current word and start the next one      |
//!
//! Only letters that still lead to a BIP-39 word are usable: `4` and `6` skip
//! the rest and `5` will not add them, so a spelling that matches nothing cannot
//! be typed in the first place. What `#` still has to reject is a prefix several
//! words share — see [`WordEntry::rejected`].
//!
//! Pure state — no display or GPIO — so the screen code can render it and the
//! main loop can drive it without either knowing about the other.

use heapless::{String, Vec};

use crate::bip39_wordlist::{self, LetterSet};

/// Letters the cursor walks, in order.
pub const ALPHABET_TEXT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The same letters as bytes: they are all ASCII, so byte indexing is character
/// indexing, which is what the cursor wants.
pub const ALPHABET: &[u8] = ALPHABET_TEXT.as_bytes();

// A [`LetterSet`] is indexed by position in the alphabet, and is built from the
// wordlist's lower-case letters, so the two orders have to agree.
const _: () = assert!(ALPHABET.len() == bip39_wordlist::LETTERS);

/// Words in a phrase.
pub const WORD_COUNT: usize = 11;

/// The longest word that can be entered, which is the longest one in the list.
pub use crate::bip39_wordlist::MAX_WORD_LEN;

pub type Word = String<MAX_WORD_LEN>;

pub const KEY_PREV: char = '4';
pub const KEY_NEXT: char = '6';
pub const KEY_ADD: char = '5';
pub const KEY_DELETE: char = '*';
pub const KEY_ACCEPT: char = '#';

pub struct WordEntry {
    accepted: Vec<Word, WORD_COUNT>,
    current: Word,
    cursor: usize,
    /// Letters that extend [`Self::current`] towards a real word. Derived from
    /// `current`, so every change to it goes through [`Self::refresh`].
    reachable: LetterSet,
    rejected: bool,
}

impl Default for WordEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl WordEntry {
    pub fn new() -> Self {
        let mut entry = Self {
            accepted: Vec::new(),
            current: String::new(),
            cursor: 0,
            reachable: LetterSet::EMPTY,
            rejected: false,
        };
        entry.refresh();

        entry
    }

    /// The letter `5` would append, or `None` when there is none to offer: the
    /// current spelling is already as long as any word that starts with it, or
    /// the phrase is finished.
    pub fn selected(&self) -> Option<char> {
        self.cursor().map(|cursor| ALPHABET[cursor] as char)
    }

    /// Position of the cursor within [`ALPHABET`], or `None` when no letter is
    /// selectable.
    pub fn cursor(&self) -> Option<usize> {
        self.reachable.contains(self.cursor).then_some(self.cursor)
    }

    /// Letters that can still be added. The rest are dead ends and are shown as
    /// unavailable rather than silently doing nothing.
    pub fn reachable(&self) -> LetterSet {
        self.reachable
    }

    /// Whether the last `#` was refused.
    ///
    /// Because unreachable letters cannot be typed, this only ever means the
    /// spelling so far is shared by several words — `AB`, say, which starts
    /// `able` and `about` among others. It is not that the word is unknown, so
    /// the way out is to keep spelling rather than to start over. Cleared by the
    /// next keypress.
    pub fn rejected(&self) -> bool {
        self.rejected
    }

    /// The word being spelled out, which is empty right after a word is
    /// accepted.
    pub fn current(&self) -> &str {
        &self.current
    }

    /// Words accepted so far, oldest first.
    pub fn accepted(&self) -> &[Word] {
        &self.accepted
    }

    /// 1-based number of the word being entered, for display. Stays at
    /// [`WORD_COUNT`] once the phrase is complete rather than running past it.
    pub fn word_number(&self) -> usize {
        (self.accepted.len() + 1).min(WORD_COUNT)
    }

    pub fn is_complete(&self) -> bool {
        self.accepted.len() == WORD_COUNT
    }

    /// Applies a keypress. Returns whether anything changed, so the caller can
    /// skip the (full-screen, and therefore slow) redraw when it hasn't —
    /// pressing `5` against a dead-end letter, say.
    pub fn handle_key(&mut self, key: char) -> bool {
        // Any press clears a standing rejection, so the message never outlives
        // the word it was about. Counts as a change even when the key does
        // nothing else, because the screen still has to repaint without it.
        let was_rejected = core::mem::take(&mut self.rejected);

        let changed = match key {
            KEY_PREV => self.move_cursor(LetterSet::prev_before),
            KEY_NEXT => self.move_cursor(LetterSet::next_after),
            KEY_ADD if self.reachable.contains(self.cursor) => {
                self.current
                    .push(ALPHABET[self.cursor] as char)
                    // A reachable letter is one some longer word continues with,
                    // and every word fits, so there is always room for it.
                    .expect("a reachable letter always leaves the word within MAX_WORD_LEN");
                self.refresh();

                true
            }
            KEY_DELETE => self.delete(),
            KEY_ACCEPT if !self.is_complete() && !self.current.is_empty() => {
                // Only real BIP-39 words can be accepted: the phrase is built by
                // packing each word's index in the wordlist, so anything not in
                // the list has no index to pack. A unique prefix is enough and
                // is completed here, so the accepted word is always the whole
                // one even when only four letters were typed.
                let Some(word) = crate::bip39::resolve(&self.current) else {
                    self.rejected = true;

                    return true;
                };

                let mut accepted = Word::new();
                for letter in word.chars() {
                    accepted
                        .push(letter.to_ascii_uppercase())
                        .expect("wordlist words fit MAX_WORD_LEN");
                }

                self.current.clear();
                self.accepted
                    .push(accepted)
                    .expect("checked against WORD_COUNT above");
                self.refresh();

                true
            }
            _ => false,
        };

        changed || was_rejected
    }

    /// Backspace that keeps going past the start of a word: with nothing left
    /// to delete it reopens the previous word, which is the only way to correct
    /// one that has already been accepted.
    fn delete(&mut self) -> bool {
        if self.current.pop().is_none() {
            match self.accepted.pop() {
                Some(word) => self.current = word,
                None => return false,
            }
        }

        self.refresh();

        true
    }

    /// Moves to the next letter `step` finds in [`Self::reachable`], skipping
    /// the dead ends in between.
    ///
    /// Reports no change when the cursor stays put — there is one reachable
    /// letter, or none — which spares the caller a redraw that would look
    /// identical.
    fn move_cursor(&mut self, step: fn(LetterSet, usize) -> Option<usize>) -> bool {
        match step(self.reachable, self.cursor) {
            Some(next) if next != self.cursor => {
                self.cursor = next;

                true
            }
            _ => false,
        }
    }

    /// Recomputes what can follow the current spelling, and puts the cursor on
    /// the first of those letters.
    ///
    /// The cursor has to move: the letter it was on is usually a dead end under
    /// the new spelling. Landing on the lowest one keeps it somewhere
    /// predictable, which for an empty word is `A`.
    fn refresh(&mut self) {
        self.reachable = if self.is_complete() {
            LetterSet::EMPTY
        } else {
            bip39_wordlist::next_letters(&self.current)
        };

        self.cursor = self.reachable.first().unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks the cursor onto `letter` with `6` presses and adds it, the way a
    /// user would, rather than reaching into the state.
    ///
    /// Asserts the letter is reachable first: that is the property worth
    /// holding, since an unreachable letter is one no amount of pressing `5`
    /// would ever add.
    fn add_letter(entry: &mut WordEntry, letter: char) {
        let target = ALPHABET
            .iter()
            .position(|candidate| *candidate == letter.to_ascii_uppercase() as u8)
            .expect("wordlist letters are a-z");

        assert!(
            entry.reachable().contains(target),
            "{letter:?} is unreachable after {:?}",
            entry.current()
        );

        for _ in 0..ALPHABET.len() {
            if entry.cursor() == Some(target) {
                break;
            }
            entry.handle_key(KEY_NEXT);
        }

        assert_eq!(
            entry.cursor(),
            Some(target),
            "cursor never reached {letter:?}"
        );
        assert!(entry.handle_key(KEY_ADD), "{letter:?} was not added");
    }

    fn spell(entry: &mut WordEntry, word: &str) {
        for letter in word.chars() {
            add_letter(entry, letter);
        }
    }

    fn enter(entry: &mut WordEntry, word: &str) {
        spell(entry, word);
        assert!(entry.handle_key(KEY_ACCEPT));
        assert!(!entry.rejected(), "{word:?} was refused");
    }

    #[test]
    fn a_new_entry_starts_on_a_with_every_initial_offered() {
        let entry = WordEntry::new();

        assert_eq!(entry.current(), "");
        assert_eq!(entry.cursor(), Some(0));
        assert_eq!(entry.selected(), Some('A'));
        assert_eq!(entry.word_number(), 1);
        assert_eq!(entry.reachable().count(), 25);
    }

    #[test]
    fn a_word_is_stored_in_upper_case() {
        let mut entry = WordEntry::new();
        enter(&mut entry, "abandon");

        assert_eq!(entry.accepted(), ["ABANDON"]);
        assert_eq!(entry.current(), "");
        assert_eq!(entry.word_number(), 2);
    }

    #[test]
    fn a_unique_prefix_is_completed_on_accept() {
        let mut entry = WordEntry::new();
        enter(&mut entry, "aban");

        assert_eq!(entry.accepted(), ["ABANDON"]);
    }

    #[test]
    fn the_cursor_skips_letters_that_spell_nothing() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "ab");

        // "ab" continues only into a, i, l, o, s, u.
        assert_eq!(entry.cursor(), Some(0));
        entry.handle_key(KEY_NEXT);
        assert_eq!(entry.selected(), Some('I'));
        entry.handle_key(KEY_PREV);
        assert_eq!(entry.selected(), Some('A'));
        // Wrapping backwards from the first lands on the last, not on Z.
        entry.handle_key(KEY_PREV);
        assert_eq!(entry.selected(), Some('U'));
    }

    #[test]
    fn a_dead_end_letter_cannot_be_added() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "aband");

        // Only "abandon" continues, so O is the one letter on offer.
        assert_eq!(entry.reachable().count(), 1);
        assert_eq!(entry.selected(), Some('O'));

        // The cursor cannot be moved off it, so `5` can only ever add O.
        assert!(!entry.handle_key(KEY_NEXT));
        assert!(!entry.handle_key(KEY_PREV));
        assert_eq!(entry.selected(), Some('O'));
    }

    #[test]
    fn a_finished_word_offers_no_letter() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "abandon");

        assert!(entry.reachable().is_empty());
        assert_eq!(entry.cursor(), None);
        assert_eq!(entry.selected(), None);
        // With nothing to select, the cursor keys do nothing at all.
        assert!(!entry.handle_key(KEY_NEXT));
        assert!(!entry.handle_key(KEY_ADD));
    }

    #[test]
    fn a_word_that_extends_another_still_offers_its_extensions() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "add");

        // "add" is a word, but "addict" and "address" continue it.
        assert_eq!(entry.reachable().count(), 2);
        assert_eq!(entry.selected(), Some('I'));

        // Accepting here takes the word, not one of its extensions.
        assert!(entry.handle_key(KEY_ACCEPT));
        assert_eq!(entry.accepted(), ["ADD"]);
    }

    #[test]
    fn an_ambiguous_prefix_is_refused_and_kept() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "ab");

        assert!(entry.handle_key(KEY_ACCEPT));
        assert!(entry.rejected());
        // Refusing costs the user nothing: the word is still there to finish.
        assert_eq!(entry.current(), "AB");
        assert!(entry.accepted().is_empty());
    }

    #[test]
    fn the_next_keypress_clears_a_refusal() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "ab");
        entry.handle_key(KEY_ACCEPT);
        assert!(entry.rejected());

        // Even a key that changes nothing else is a change, because the message
        // has to come off the screen.
        assert!(entry.handle_key('1'));
        assert!(!entry.rejected());
        assert!(!entry.handle_key('1'));
    }

    #[test]
    fn a_refused_word_can_be_finished_and_accepted() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "ab");
        entry.handle_key(KEY_ACCEPT);

        add_letter(&mut entry, 'l');
        assert!(!entry.rejected());
        assert!(entry.handle_key(KEY_ACCEPT));
        assert_eq!(entry.accepted(), ["ABLE"]);
    }

    #[test]
    fn delete_backs_out_of_a_word_letter_by_letter() {
        let mut entry = WordEntry::new();
        spell(&mut entry, "aband");

        assert!(entry.handle_key(KEY_DELETE));
        assert_eq!(entry.current(), "ABAN");
        // The offered letters follow the word back.
        assert_eq!(entry.reachable().count(), 1);
        assert_eq!(entry.selected(), Some('D'));
    }

    #[test]
    fn delete_on_an_empty_word_reopens_the_previous_one() {
        let mut entry = WordEntry::new();
        enter(&mut entry, "abandon");
        assert_eq!(entry.word_number(), 2);

        assert!(entry.handle_key(KEY_DELETE));
        assert_eq!(entry.current(), "ABANDON");
        assert!(entry.accepted().is_empty());
        assert_eq!(entry.word_number(), 1);
    }

    #[test]
    fn delete_at_the_very_start_does_nothing() {
        let mut entry = WordEntry::new();

        assert!(!entry.handle_key(KEY_DELETE));
        assert_eq!(entry.current(), "");
    }

    #[test]
    fn a_finished_phrase_takes_no_more_words() {
        let mut entry = WordEntry::new();
        for _ in 0..WORD_COUNT {
            enter(&mut entry, "abandon");
        }

        assert!(entry.is_complete());
        assert_eq!(entry.accepted().len(), WORD_COUNT);
        assert_eq!(entry.word_number(), WORD_COUNT);

        // Nothing is on offer, so no key can grow the phrase past its length.
        assert!(entry.reachable().is_empty());
        assert!(!entry.handle_key(KEY_ADD));
        assert!(!entry.handle_key(KEY_ACCEPT));
        assert_eq!(entry.accepted().len(), WORD_COUNT);

        // `*` is the way back in, and reopens the last word.
        assert!(entry.handle_key(KEY_DELETE));
        assert!(!entry.is_complete());
        assert_eq!(entry.current(), "ABANDON");
    }

    #[test]
    fn every_word_in_the_list_can_be_spelled_and_accepted() {
        // The guarantee that makes dimming safe: skipping dead-end letters must
        // never put a real word out of reach. Checked for all 2048 rather than
        // for a sample, since it is the whole basis for hiding letters at all.
        for word in bip39_wordlist::words() {
            let mut entry = WordEntry::new();
            enter(&mut entry, word);

            let expected: heapless::String<MAX_WORD_LEN> =
                word.chars().map(|c| c.to_ascii_uppercase()).collect();
            assert_eq!(entry.accepted(), [expected]);
        }
    }
}
