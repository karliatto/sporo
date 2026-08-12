//! State for the recovery-phrase entry screen.
//!
//! The keypad has no letter keys, so words are spelled out by walking a cursor
//! along the alphabet and confirming one letter at a time:
//!
//! | Key | Action                                          |
//! | --- | ----------------------------------------------- |
//! | `4` | move the cursor to the previous letter (wraps)   |
//! | `6` | move the cursor to the next letter (wraps)       |
//! | `5` | append the selected letter to the current word   |
//! | `*` | delete the last letter, or reopen the last word  |
//! | `#` | accept the current word and start the next one   |
//!
//! Pure state — no display or GPIO — so the screen code can render it and the
//! main loop can drive it without either knowing about the other.

use heapless::{String, Vec};

/// Letters the cursor walks, in order.
pub const ALPHABET_TEXT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The same letters as bytes: they are all ASCII, so byte indexing is character
/// indexing, which is what the cursor wants.
pub const ALPHABET: &[u8] = ALPHABET_TEXT.as_bytes();

/// Words in a phrase.
pub const WORD_COUNT: usize = 11;

/// No BIP39 English word is longer than this, and the first four letters
/// already identify a word uniquely.
pub const MAX_WORD_LEN: usize = 8;

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
}

impl WordEntry {
    pub const fn new() -> Self {
        Self {
            accepted: Vec::new(),
            current: String::new(),
            cursor: 0,
        }
    }

    /// The letter `5` would append.
    pub fn selected(&self) -> char {
        ALPHABET[self.cursor] as char
    }

    /// Position of the cursor within [`ALPHABET`].
    pub fn cursor(&self) -> usize {
        self.cursor
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

    /// Whether `5` would add anything: a word already at capacity takes no more
    /// letters, and neither does a complete phrase.
    pub fn accepts_letter(&self) -> bool {
        !self.is_complete() && self.current.len() < MAX_WORD_LEN
    }

    /// Applies a keypress. Returns whether anything changed, so the caller can
    /// skip the (full-screen, and therefore slow) redraw when it hasn't —
    /// pressing `5` against a full word, say.
    pub fn handle_key(&mut self, key: char) -> bool {
        match key {
            KEY_PREV => {
                self.cursor = if self.cursor == 0 {
                    ALPHABET.len() - 1
                } else {
                    self.cursor - 1
                };

                true
            }
            KEY_NEXT => {
                self.cursor = (self.cursor + 1) % ALPHABET.len();

                true
            }
            KEY_ADD if self.accepts_letter() => {
                self.current
                    .push(self.selected())
                    .expect("checked against MAX_WORD_LEN above");

                true
            }
            KEY_DELETE => self.delete(),
            KEY_ACCEPT if !self.is_complete() && !self.current.is_empty() => {
                let word = core::mem::take(&mut self.current);
                self.accepted
                    .push(word)
                    .expect("checked against WORD_COUNT above");
                // Every word starts from A, so the cursor is always somewhere
                // predictable when one begins.
                self.cursor = 0;

                true
            }
            _ => false,
        }
    }

    /// Backspace that keeps going past the start of a word: with nothing left
    /// to delete it reopens the previous word, which is the only way to correct
    /// one that has already been accepted.
    fn delete(&mut self) -> bool {
        if self.current.pop().is_some() {
            return true;
        }

        match self.accepted.pop() {
            Some(word) => {
                self.current = word;

                true
            }
            None => false,
        }
    }
}
