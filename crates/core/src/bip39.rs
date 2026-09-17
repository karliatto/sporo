//! Turning entered words into a BIP-39 mnemonic.
//!
//! The entry screen collects every word but the last; this module resolves what
//! was typed against the official wordlist and derives the final word that makes
//! the phrase a valid mnemonic. Both lengths the device builds go through the
//! same code, told apart by a [`SeedLength`].
//!
//! ## Why the last word is not just a checksum
//!
//! A 12-word mnemonic encodes 132 bits — 128 bits of entropy followed by a 4-bit
//! checksum — split into twelve 11-bit words. Eleven words only account for
//! 11 x 11 = 121 of those bits, so the twelfth carries the remaining 7 bits of
//! entropy as well as the checksum:
//!
//! ```text
//!   word 1 .. word 11         word 12
//!  |<----- 121 bits ----->|<-7->|<-4->|
//!  |<------- entropy: 128 ----->| sum |
//! ```
//!
//! A 24-word mnemonic is the same shape at twice the size: 256 bits of entropy
//! and an 8-bit checksum, so twenty-three words account for 253 bits and the
//! twenty-fourth carries 3 bits of entropy on top of the checksum:
//!
//! ```text
//!   word 1 .. word 23         word 24
//!  |<----- 253 bits ----->|<3>|<-8->|
//!  |<------- entropy: 256 --->| sum |
//! ```
//!
//! Those bits have to come from somewhere: there are 2^7 = 128 equally valid
//! twelfth words for any given eleven (2^3 = 8 twenty-fourth words for any
//! twenty-three), and picking one by hand — or always taking the first — would
//! throw that entropy away.
//!
//! They come from the user, as coin flips entered on the keypad — see
//! [`crate::flips`]. The chip's hardware RNG could supply them instead, and
//! did, but a seed generator whose randomness comes out of an opaque block on
//! the die asks the user to trust the one thing this device exists not to trust.
//!
//! Note how far that reaches, though: these are a handful of bits, and the rest
//! are the words the user chose. A phrase whose entropy is a coin's all the way
//! down is a different thing — a flip per bit, and every word derived.

use heapless::String;
use sha2::{Digest, Sha256};

use crate::bip39_wordlist::{self, MAX_WORD_LEN};

/// The mnemonic lengths the device builds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeedLength {
    Words12,
    Words24,
}

impl SeedLength {
    /// Every length, so the arithmetic below can be checked for each of them.
    pub const ALL: [Self; 2] = [Self::Words12, Self::Words24];

    /// Words in the finished mnemonic.
    pub const fn total_words(self) -> usize {
        match self {
            Self::Words12 => 12,
            Self::Words24 => 24,
        }
    }

    /// Words the user supplies. The mnemonic's last word is derived from these
    /// rather than entered.
    pub const fn entered_words(self) -> usize {
        self.total_words() - 1
    }

    /// Entropy bits the mnemonic encodes.
    pub const fn entropy_bits(self) -> usize {
        match self {
            Self::Words12 => 128,
            Self::Words24 => 256,
        }
    }

    /// Checksum bits, `ENT / 32` per BIP-39.
    pub const fn checksum_bits(self) -> usize {
        self.entropy_bits() / 32
    }

    /// Bits of fresh entropy the final word carries on top of the checksum.
    pub const fn final_word_entropy_bits(self) -> usize {
        self.entropy_bits() - BITS_PER_WORD * self.entered_words()
    }
}

/// Words the user supplies for the longest [`SeedLength`]: what storage for
/// entered words is sized to.
pub const MAX_WORD_COUNT: usize = 23;

/// Words in the longest finished mnemonic.
pub const MAX_WORD_COUNT_TOTAL: usize = MAX_WORD_COUNT + 1;

/// One entered word, as [`complete`] receives it. Any case is accepted; the
/// entry screen happens to store upper-case.
pub type Word = String<MAX_WORD_LEN>;

/// Bits each word contributes: the wordlist has 2^11 entries.
const BITS_PER_WORD: usize = 11;

/// Bytes the longest mnemonic's bits — entropy and checksum — pack into.
const MAX_PACKED_BYTES: usize = (BITS_PER_WORD * MAX_WORD_COUNT_TOTAL).div_ceil(8);

// Getting this arithmetic wrong would produce phrases no other wallet accepts,
// and only for some inputs, so it is checked at compile time for every length
// rather than trusted.
const _: () = {
    let mut index = 0;
    while index < SeedLength::ALL.len() {
        let length = SeedLength::ALL[index];
        let checksum = length.checksum_bits();

        assert!(BITS_PER_WORD * length.total_words() == length.entropy_bits() + checksum);
        assert!(length.final_word_entropy_bits() + checksum == BITS_PER_WORD);
        // The flips hand the final word's entropy over as one byte.
        assert!(length.final_word_entropy_bits() <= u8::BITS as usize);
        // The checksum is taken from the digest's first byte alone.
        assert!(checksum <= u8::BITS as usize);
        // Entropy has to be whole bytes to be hashed.
        assert!(length.entropy_bits().is_multiple_of(8));
        assert!(length.entered_words() <= MAX_WORD_COUNT);

        index += 1;
    }
};
const _: () = assert!(bip39_wordlist::COUNT == 1 << BITS_PER_WORD);

/// A finished mnemonic: the entered words followed by the derived final one, all
/// in the canonical lowercase spelling.
///
/// Sized for the longest [`SeedLength`], so either fits without a heap; only
/// [`Self::words`] says how many of the slots are the phrase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mnemonic {
    words: [&'static str; MAX_WORD_COUNT_TOTAL],
    length: SeedLength,
}

impl Mnemonic {
    /// The phrase, first word first.
    pub fn words(&self) -> &[&'static str] {
        &self.words[..self.length.total_words()]
    }

    pub const fn length(&self) -> SeedLength {
        self.length
    }

    /// The word derived rather than entered.
    pub const fn final_word(&self) -> &'static str {
        self.words[self.length.entered_words()]
    }
}

/// Resolves what was typed to a wordlist entry.
///
/// Accepts a whole word, or any prefix only one word in the list starts with —
/// every 4-letter prefix is unique, which is what lets a word be accepted before
/// it has been fully spelled out. Input is matched case-insensitively, since the
/// entry screen works in upper case and the wordlist is lower case.
///
/// Returns `None` for a prefix several words share (`"AB"`) or none (`"ABZ"`).
pub fn resolve(input: &str) -> Option<&'static str> {
    let needle = bip39_wordlist::Needle::new(input)?;

    // The empty prefix matches every word, which is ambiguous rather than
    // unresolvable, but there is nothing to accept either way.
    if needle.is_empty() {
        return None;
    }

    let needle = needle.as_bytes();

    let mut prefixed: Option<&'static str> = None;
    let mut ambiguous = false;

    for word in bip39_wordlist::words() {
        let bytes = word.as_bytes();
        if !bytes.starts_with(needle) {
            continue;
        }

        // A whole word is never ambiguous, even when longer words extend it: 49
        // of the 2048 are prefixes of another ("add" also starts "addict" and
        // "address"), and treating those as ambiguous would make them
        // impossible to enter. Words are unique, so nothing later can beat it.
        if bytes.len() == needle.len() {
            return Some(word);
        }

        if prefixed.is_some() {
            ambiguous = true;
        } else {
            prefixed = Some(word);
        }
    }

    if ambiguous {
        None
    } else {
        prefixed
    }
}

/// Completes a mnemonic of `length` from the entered words plus fresh entropy.
///
/// Only the low [`SeedLength::final_word_entropy_bits`] of `extra_entropy` are
/// used; the rest are ignored, so the caller can pass a whole byte without
/// masking it first.
///
/// Returns `None` unless exactly [`SeedLength::entered_words`] words were given
/// and each one resolves — which the entry screen guarantees, since it resolves
/// words before accepting them.
pub fn complete(length: SeedLength, entered: &[Word], extra_entropy: u8) -> Option<Mnemonic> {
    if entered.len() != length.entered_words() {
        return None;
    }

    let mut mnemonic = Mnemonic {
        words: [""; MAX_WORD_COUNT_TOTAL],
        length,
    };

    // 256 bits do not fit an integer, so the entropy is packed into bytes, most
    // significant bit first — the order BIP-39 hashes it in.
    let mut packed = [0u8; MAX_PACKED_BYTES];
    let mut position = 0;

    for (slot, word) in mnemonic.words.iter_mut().zip(entered) {
        let canonical = resolve(word)?;
        push_bits(
            &mut packed,
            &mut position,
            bip39_wordlist::index_of(canonical)?,
            BITS_PER_WORD,
        );
        *slot = canonical;
    }

    let extra_bits = length.final_word_entropy_bits();
    let extra = u16::from(extra_entropy) & ((1 << extra_bits) - 1);
    push_bits(&mut packed, &mut position, extra, extra_bits);

    // BIP-39 checksum: the leading bits of SHA-256 over the entropy.
    let checksum_bits = length.checksum_bits();
    let digest = Sha256::digest(&packed[..length.entropy_bits() / 8]);
    let checksum = u16::from(digest[0] >> (8 - checksum_bits));

    mnemonic.words[length.entered_words()] =
        bip39_wordlist::word_at((extra << checksum_bits) | checksum)?;

    Some(mnemonic)
}

/// Appends the low `count` bits of `value` to `packed` at bit `position`, most
/// significant first, and moves `position` past them.
fn push_bits(packed: &mut [u8], position: &mut usize, value: u16, count: usize) {
    for bit in (0..count).rev() {
        if (value >> bit) & 1 == 1 {
            packed[*position / 8] |= 0x80 >> (*position % 8);
        }
        *position += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use SeedLength::{Words12, Words24};

    fn word(text: &str) -> Word {
        let mut word = Word::new();
        for letter in text.chars() {
            word.push(letter).expect("test words fit MAX_WORD_LEN");
        }

        word
    }

    fn entered(phrase: &str) -> heapless::Vec<Word, MAX_WORD_COUNT> {
        phrase.split_whitespace().map(word).collect()
    }

    /// The first `words` words of `pattern` repeated, joined by spaces.
    fn repeated(pattern: &str, words: usize) -> heapless::String<256> {
        let mut phrase = heapless::String::new();
        for text in pattern.split_whitespace().cycle().take(words) {
            if !phrase.is_empty() {
                phrase.push(' ').unwrap();
            }
            phrase.push_str(text).unwrap();
        }

        phrase
    }

    /// The reference vectors from BIP-39 itself, split the way this module
    /// splits them: every word but the last is entered, and the low bits of the
    /// entropy are what the coin screen supplies.
    ///
    /// Getting this wrong yields phrases that look right and restore as a
    /// different wallet, or as none, so it is checked against the spec's own
    /// numbers rather than against this implementation's idea of them. The
    /// spec's phrases repeat, which is what lets them be written as a pattern.
    const VECTORS: [(SeedLength, &str, u8, &str); 8] = [
        (Words12, "abandon", 0, "about"),
        (
            Words12,
            "legal winner thank year wave sausage worth useful",
            127,
            "yellow",
        ),
        (
            Words12,
            "letter advice cage absurd amount doctor acoustic avoid",
            0,
            "above",
        ),
        (Words12, "zoo", 127, "wrong"),
        (Words24, "abandon", 0, "art"),
        (
            Words24,
            "legal winner thank year wave sausage worth useful",
            7,
            "title",
        ),
        (
            Words24,
            "letter advice cage absurd amount doctor acoustic avoid",
            0,
            "bless",
        ),
        (Words24, "zoo", 7, "vote"),
    ];

    fn vectors() -> impl Iterator<Item = (SeedLength, heapless::String<256>, u8, &'static str)> {
        VECTORS
            .into_iter()
            .map(|(length, pattern, extra, expected)| {
                (
                    length,
                    repeated(pattern, length.entered_words()),
                    extra,
                    expected,
                )
            })
    }

    #[test]
    fn completes_the_bip39_reference_vectors() {
        for (length, phrase, extra, expected) in vectors() {
            let mnemonic =
                complete(length, &entered(&phrase), extra).expect("vector words are in the list");

            assert_eq!(mnemonic.words().len(), length.total_words());
            assert_eq!(mnemonic.final_word(), expected, "for {phrase:?}");
            for (produced, given) in mnemonic.words().iter().zip(phrase.split_whitespace()) {
                assert_eq!(*produced, given);
            }
        }
    }

    #[test]
    fn completes_the_same_vectors_entered_in_upper_case() {
        // What the entry screen actually stores.
        for (length, phrase, extra, expected) in vectors() {
            let upper: heapless::String<256> =
                phrase.chars().map(|c| c.to_ascii_uppercase()).collect();
            let mnemonic =
                complete(length, &entered(&upper), extra).expect("case is not significant");

            assert_eq!(mnemonic.final_word(), expected);
        }
    }

    #[test]
    fn only_the_low_bits_of_the_extra_byte_are_used() {
        for length in SeedLength::ALL {
            let words = entered(&repeated("abandon", length.entered_words()));

            // The bit just above the final word's entropy must be discarded
            // rather than shifted into the phrase.
            let above = 1u8 << length.final_word_entropy_bits();
            assert_eq!(
                complete(length, &words, above).unwrap(),
                complete(length, &words, 0).unwrap()
            );
            assert_ne!(
                complete(length, &words, 1).unwrap(),
                complete(length, &words, 0).unwrap()
            );
        }
    }

    #[test]
    fn every_extra_value_yields_a_distinct_final_word() {
        // The final word's entropy is only worth carrying if every value lands
        // somewhere different.
        for length in SeedLength::ALL {
            let words = entered(&repeated("abandon", length.entered_words()));
            let mut seen: heapless::Vec<&str, 128> = heapless::Vec::new();

            for extra in 0..(1u8 << length.final_word_entropy_bits()) {
                let word = complete(length, &words, extra).unwrap().final_word();
                assert!(
                    !seen.contains(&word),
                    "{word:?} produced by two different extras"
                );
                seen.push(word).unwrap();
            }
        }
    }

    #[test]
    fn the_flips_complete_the_bip39_reference_vectors() {
        use crate::flips::{Flip, Flips};

        // Every entropy value the vectors use is all ones or all zeros, so they
        // say nothing about the order flips are packed in — that is
        // `flips::tests::the_first_flip_is_the_most_significant_bit`'s job.
        // What this checks is the seam: that what `Flips` hands `complete` is
        // what the spec's own vectors expect to receive.
        for (length, phrase, extra, expected) in vectors() {
            let flip = if extra == 0 { Flip::Tails } else { Flip::Heads };
            let mut flips = Flips::new(length);
            while flips.record(flip) {}

            let entropy = flips.entropy().expect("every flip was recorded");
            let mnemonic =
                complete(length, &entered(&phrase), entropy).expect("vector words are in the list");

            assert_eq!(mnemonic.final_word(), expected, "for {phrase:?}");
        }
    }

    #[test]
    fn a_short_phrase_does_not_complete() {
        for length in SeedLength::ALL {
            assert_eq!(complete(length, &[word("ABANDON")], 0), None);
            assert_eq!(complete(length, &[], 0), None);
        }
    }

    #[test]
    fn a_phrase_of_the_other_length_does_not_complete() {
        let eleven = entered(&repeated("abandon", Words12.entered_words()));
        let twenty_three = entered(&repeated("abandon", Words24.entered_words()));

        assert_eq!(complete(Words24, &eleven, 0), None);
        assert_eq!(complete(Words12, &twenty_three, 0), None);
    }

    #[test]
    fn resolve_accepts_whole_words_in_either_case() {
        assert_eq!(resolve("abandon"), Some("abandon"));
        assert_eq!(resolve("ABANDON"), Some("abandon"));
        assert_eq!(resolve("AbAnDoN"), Some("abandon"));
        assert_eq!(resolve("ZOO"), Some("zoo"));
    }

    #[test]
    fn resolve_accepts_a_prefix_only_one_word_has() {
        assert_eq!(resolve("ABAN"), Some("abandon"));
        // Four letters is enough for every word in the list, but shorter
        // prefixes resolve too when they happen to be unique: only "aerobic"
        // starts "ae", whereas "zo" still leaves "zone" and "zoo".
        assert_eq!(resolve("AE"), Some("aerobic"));
        assert_eq!(resolve("ZO"), None);
    }

    #[test]
    fn resolve_refuses_a_prefix_several_words_share() {
        // The case the entry screen reports back to the user: reachable, but
        // not yet one word.
        assert_eq!(resolve("AB"), None);
        assert_eq!(resolve("A"), None);
    }

    #[test]
    fn resolve_prefers_a_whole_word_to_the_words_extending_it() {
        // 49 words start another word. Each has to resolve to itself, or it
        // could never be entered at all.
        let prefix_words: heapless::Vec<&str, 64> = bip39_wordlist::words()
            .filter(|word| {
                bip39_wordlist::words().any(|other| other != *word && other.starts_with(word))
            })
            .collect();

        assert_eq!(prefix_words.len(), 49);
        for word in prefix_words {
            assert_eq!(resolve(word), Some(word));
        }
    }

    #[test]
    fn resolve_refuses_what_is_not_in_the_list() {
        assert_eq!(resolve(""), None);
        assert_eq!(resolve("ABZ"), None);
        assert_eq!(resolve("QQQQ"), None);
        assert_eq!(resolve("ABANDONING"), None);
        assert_eq!(resolve("ABAND0N"), None);
    }

    #[test]
    fn every_word_resolves_to_itself() {
        for word in bip39_wordlist::words() {
            assert_eq!(resolve(word), Some(word));
        }
    }
}
