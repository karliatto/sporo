//! The official BIP-39 English wordlist, embedded from `assets/bip39-english.txt`.
//!
//! The asset is the upstream file byte for byte, so its provenance can be
//! checked directly rather than by reading a diff:
//!
//! ```text
//! $ sha256sum src/assets/bip39-english.txt
//! 2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda
//! ```
//!
//! which is <https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt>.
//! A single altered word would silently produce mnemonics no other wallet can
//! restore, so verify that hash rather than trusting the file to look right.
//!
//! Words are read out of the text as needed instead of being expanded into an
//! array of 2048 string slices. The slices would cost 16 KiB of pointers on top
//! of the text itself, to save a scan of 2048 short lines on a keypress — the
//! wrong trade on a device with one user and 4 MiB of flash.

/// Line-separated, in the order BIP-39 defines: line `n` encodes the value `n`.
const TEXT: &str = include_str!("assets/bip39-english.txt");

/// Words in the list. BIP-39 fixes this at 2^11, one per 11-bit group.
pub const COUNT: usize = 2048;

/// Letters the list is written in: plain ASCII `a`-`z`.
///
/// The entry screen's alphabet is the same letters in the same order, so a
/// position in one is a position in the other and [`LetterSet`] can be indexed
/// by either.
pub const LETTERS: usize = 26;

/// Length of the longest word, and so the capacity a buffer needs to hold any
/// of them. Asserted against the asset below rather than trusted.
pub const MAX_WORD_LEN: usize = 8;

/// Number of lines `str::lines` will yield, computed at compile time so a
/// truncated or re-encoded asset fails the build rather than shifting every
/// index and quietly generating wrong phrases.
const fn line_count(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut newlines = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\n' {
            newlines += 1;
        }
        index += 1;
    }

    // `lines` does not yield a trailing empty line, so a final newline does not
    // add one.
    if bytes.is_empty() || bytes[bytes.len() - 1] == b'\n' {
        newlines
    } else {
        newlines + 1
    }
}

/// Length of the longest line, used to check [`MAX_WORD_LEN`] against the asset
/// that has to fit in it.
const fn longest_line(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut longest = 0;
    let mut current = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\n' {
            if current > longest {
                longest = current;
            }
            current = 0;
        } else {
            current += 1;
        }
        index += 1;
    }

    if current > longest {
        longest = current;
    }

    longest
}

const fn contains_carriage_return(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\r' {
            return true;
        }
        index += 1;
    }

    false
}

const _: () = assert!(line_count(TEXT) == COUNT, "wordlist is not 2048 words");

// A checkout that converted the asset to CRLF would leave a trailing `\r` on
// every word, so nothing would resolve and no phrase could be entered.
const _: () = assert!(
    !contains_carriage_return(TEXT),
    "wordlist must use LF line endings"
);

// `Word` and `Needle` are both sized from MAX_WORD_LEN, so a longer word in the
// asset would be silently truncated on its way in or out.
const _: () = assert!(
    longest_line(TEXT) == MAX_WORD_LEN,
    "MAX_WORD_LEN does not match the longest word in the wordlist"
);

/// The words, in the order that defines their index.
pub fn words() -> impl Iterator<Item = &'static str> + Clone {
    TEXT.lines()
}

/// Input normalised for matching against the list: lowercase, since the entry
/// screen works in upper case and the wordlist is lower case.
///
/// Held by value in a fixed buffer rather than borrowed, so that lowercasing
/// happens once per lookup instead of once per word compared.
pub struct Needle {
    bytes: [u8; MAX_WORD_LEN],
    len: usize,
}

impl Needle {
    /// `None` for input that cannot match any word however it continues: longer
    /// than the longest word, or holding something other than a letter.
    ///
    /// Empty input is a valid needle — every word starts with it.
    pub fn new(input: &str) -> Option<Self> {
        if input.len() > MAX_WORD_LEN {
            return None;
        }

        let mut bytes = [0u8; MAX_WORD_LEN];
        for (slot, byte) in bytes.iter_mut().zip(input.bytes()) {
            if !byte.is_ascii_alphabetic() {
                return None;
            }
            *slot = byte.to_ascii_lowercase();
        }

        Some(Self {
            bytes,
            len: input.len(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A set of letters, held as one bit per position in the alphabet.
///
/// Small enough to copy and to keep as derived state on the entry screen, which
/// is the point: the alphabet strip is redrawn from it on every keypress.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LetterSet(u32);

impl LetterSet {
    pub const EMPTY: Self = Self(0);

    pub const fn contains(self, index: usize) -> bool {
        index < LETTERS && self.0 & (1 << index) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many letters are in the set.
    pub const fn count(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The lowest letter in the set, which is where the cursor goes when a word
    /// starts or changes length.
    pub fn first(self) -> Option<usize> {
        (!self.is_empty()).then(|| self.0.trailing_zeros() as usize)
    }

    /// The next letter after `index`, wrapping past `Z`. `index` itself need not
    /// be in the set, and is returned when it is the only one.
    pub fn next_after(self, index: usize) -> Option<usize> {
        self.step_from(index, |current| (current + 1) % LETTERS)
    }

    /// The previous letter before `index`, wrapping past `A`.
    pub fn prev_before(self, index: usize) -> Option<usize> {
        self.step_from(index, |current| (current + LETTERS - 1) % LETTERS)
    }

    fn insert(&mut self, index: usize) {
        if index < LETTERS {
            self.0 |= 1 << index;
        }
    }

    /// Walks the whole alphabet from `index` in the direction `step` takes it,
    /// stopping at the first letter in the set.
    fn step_from(self, index: usize, step: impl Fn(usize) -> usize) -> Option<usize> {
        let mut current = index % LETTERS;

        for _ in 0..LETTERS {
            current = step(current);
            if self.contains(current) {
                return Some(current);
            }
        }

        None
    }
}

/// The letters that can follow `prefix` and still reach a word.
///
/// This is what lets the entry screen dim the rest: a letter outside this set
/// spells something no word in the list starts with, so offering it would only
/// lead the user into a dead end they have to back out of one `*` at a time.
///
/// An empty prefix returns 25 letters, not 26 — no BIP-39 English word starts
/// with `x`. A whole word with nothing longer extending it returns the empty
/// set, and so does a prefix that matches nothing.
pub fn next_letters(prefix: &str) -> LetterSet {
    let Some(needle) = Needle::new(prefix) else {
        return LetterSet::EMPTY;
    };
    let needle = needle.as_bytes();

    let mut letters = LetterSet::EMPTY;

    for word in words() {
        let bytes = word.as_bytes();

        // Equal length is a match but extends nothing, so it contributes no
        // letter; `starts_with` alone would index past the end.
        if bytes.len() > needle.len() && bytes.starts_with(needle) {
            letters.insert(usize::from(bytes[needle.len()] - b'a'));
        }
    }

    letters
}

/// The word encoding `index`, or `None` if it is past the end of the list.
pub fn word_at(index: u16) -> Option<&'static str> {
    words().nth(usize::from(index))
}

/// The value `word` encodes, matched exactly.
pub fn index_of(word: &str) -> Option<u16> {
    words()
        .position(|candidate| candidate == word)
        .map(|index| index as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Position of an ASCII letter in the alphabet, for readable expectations.
    fn letter(character: char) -> usize {
        usize::from(character as u8 - b'a')
    }

    fn set(letters: &str) -> LetterSet {
        let mut result = LetterSet::EMPTY;
        for character in letters.chars() {
            result.insert(letter(character));
        }

        result
    }

    #[test]
    fn asset_holds_exactly_the_words_the_constants_promise() {
        assert_eq!(words().count(), COUNT);
        assert_eq!(words().map(str::len).max(), Some(MAX_WORD_LEN));
        assert!(words().all(|word| word.bytes().all(|b| b.is_ascii_lowercase())));
    }

    #[test]
    fn asset_is_sorted() {
        // Not relied on for lookup, but a list out of order would mean the file
        // is not the upstream one, and indices decide what a phrase means.
        let mut previous = "";
        for word in words() {
            assert!(previous < word, "{previous:?} then {word:?}");
            previous = word;
        }
    }

    #[test]
    fn index_and_word_round_trip_across_the_whole_list() {
        for (index, word) in words().enumerate() {
            let index = index as u16;
            assert_eq!(word_at(index), Some(word));
            assert_eq!(index_of(word), Some(index));
        }
    }

    #[test]
    fn index_past_the_end_has_no_word() {
        assert_eq!(word_at(COUNT as u16), None);
        assert_eq!(index_of("notaword"), None);
    }

    #[test]
    fn nothing_starts_with_x() {
        let initials = next_letters("");

        assert_eq!(initials.count(), LETTERS - 1);
        assert!(!initials.contains(letter('x')));
        assert!(initials.contains(letter('a')));
        assert!(initials.contains(letter('z')));
    }

    #[test]
    fn next_letters_narrows_to_a_single_continuation() {
        // "aband" only continues into "abandon".
        assert_eq!(next_letters("aband"), set("o"));
        assert_eq!(next_letters("ab"), set("ailosu"));
    }

    #[test]
    fn next_letters_is_empty_at_a_word_nothing_extends() {
        assert_eq!(next_letters("abandon"), LetterSet::EMPTY);
        assert_eq!(next_letters("zoo"), LetterSet::EMPTY);
    }

    #[test]
    fn next_letters_still_offers_extensions_of_a_whole_word() {
        // "add" is a word *and* the start of "addict" and "address", so the
        // letters that continue it must survive.
        assert_eq!(next_letters("add"), set("ir"));
    }

    #[test]
    fn next_letters_ignores_case() {
        assert_eq!(next_letters("ABAND"), next_letters("aband"));
        assert_eq!(next_letters("AbAnD"), set("o"));
    }

    #[test]
    fn next_letters_rejects_what_cannot_be_a_word() {
        assert_eq!(next_letters("abz"), LetterSet::EMPTY);
        assert_eq!(next_letters("toolongforalist"), LetterSet::EMPTY);
        assert_eq!(next_letters("ab3"), LetterSet::EMPTY);
        assert_eq!(next_letters("ábc"), LetterSet::EMPTY);
    }

    #[test]
    fn needle_normalises_and_rejects() {
        assert_eq!(Needle::new("AbC").unwrap().as_bytes(), b"abc");
        assert!(Needle::new("").unwrap().is_empty());
        assert!(Needle::new("waaaytoolong").is_none());
        assert!(Needle::new("ab-c").is_none());
    }

    #[test]
    fn letter_set_walks_only_its_members() {
        let letters = set("aco");

        assert_eq!(letters.first(), Some(letter('a')));
        assert_eq!(letters.next_after(letter('a')), Some(letter('c')));
        assert_eq!(letters.next_after(letter('c')), Some(letter('o')));
        // Past the last member it wraps to the first.
        assert_eq!(letters.next_after(letter('o')), Some(letter('a')));
        assert_eq!(letters.prev_before(letter('a')), Some(letter('o')));

        // The starting point need not be a member: `b` sits between `a` and `c`.
        assert_eq!(letters.next_after(letter('b')), Some(letter('c')));
        assert_eq!(letters.prev_before(letter('b')), Some(letter('a')));
    }

    #[test]
    fn letter_set_with_one_member_comes_back_to_it() {
        let only = set("q");

        assert_eq!(only.next_after(letter('q')), Some(letter('q')));
        assert_eq!(only.prev_before(letter('q')), Some(letter('q')));
        assert_eq!(only.next_after(letter('a')), Some(letter('q')));
    }

    #[test]
    fn empty_letter_set_goes_nowhere() {
        assert_eq!(LetterSet::EMPTY.first(), None);
        assert_eq!(LetterSet::EMPTY.next_after(0), None);
        assert_eq!(LetterSet::EMPTY.prev_before(0), None);
        assert!(LetterSet::EMPTY.is_empty());
        assert!(!LetterSet::EMPTY.contains(0));
    }

    #[test]
    fn letter_set_ignores_positions_off_the_alphabet() {
        let mut letters = LetterSet::EMPTY;
        letters.insert(LETTERS);

        assert!(letters.is_empty());
        assert!(!set("a").contains(LETTERS));
    }
}
