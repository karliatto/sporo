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

    let mut indices = [0u16; MAX_WORD_COUNT];
    for (slot, word) in indices.iter_mut().zip(entered) {
        *slot = bip39_wordlist::index_of(resolve(word)?)?;
    }

    assemble(length, &indices[..entered.len()], extra_entropy)
}

/// Reads a complete phrase of `length` and checks its checksum.
///
/// Returns `None` unless exactly [`SeedLength::total_words`] words were given,
/// every one resolves, and the final word is the one the words before it imply.
/// A phrase this accepts is one other wallets accept too, so it is what stands
/// between a mistyped word and a silently wrong answer.
pub fn parse(length: SeedLength, words: &[Word]) -> Option<Mnemonic> {
    if words.len() != length.total_words() {
        return None;
    }

    let (entered, last) = words.split_at(length.entered_words());
    let final_word = resolve(&last[0])?;

    // The final word is the entropy bits it carries followed by the checksum, so
    // dropping the checksum leaves the bits `complete` would have been handed.
    // Rebuilding from them and comparing checks the checksum without a second
    // implementation of it.
    let extra = bip39_wordlist::index_of(final_word)? >> length.checksum_bits();
    let mnemonic = complete(length, entered, extra as u8)?;

    (mnemonic.final_word() == final_word).then_some(mnemonic)
}

/// XORs two phrases of the same length into a third.
///
/// Every word but the last is the XOR of the two inputs' words at that
/// position; the last is derived from `extra_entropy` and the checksum, exactly
/// as [`complete`] derives it.
///
/// **The result is not reversible.** The final word's entropy bits come from
/// `extra_entropy` rather than from the inputs, so those bits of `a` and `b` are
/// not in the output and `xor(result, b)` does not give back `a`. This combines
/// two phrases into a new one; it is not a way to split a seed into shares.
///
/// Returns `None` if the lengths differ.
pub fn xor(a: &Mnemonic, b: &Mnemonic, extra_entropy: u8) -> Option<Mnemonic> {
    if a.length != b.length {
        return None;
    }

    let length = a.length;
    let entered = length.entered_words();

    // Each word is 11 aligned bits of the entropy, so XORing the indices is
    // XORing those bits — and the result is 11 bits too, always a word.
    let mut indices = [0u16; MAX_WORD_COUNT];
    for (slot, (word_a, word_b)) in indices
        .iter_mut()
        .zip(a.words[..entered].iter().zip(&b.words[..entered]))
    {
        *slot = bip39_wordlist::index_of(word_a)? ^ bip39_wordlist::index_of(word_b)?;
    }

    assemble(length, &indices[..entered], extra_entropy)
}

/// Builds a mnemonic from the wordlist indices of every word but the last, plus
/// the entropy bits that last word carries.
///
/// The checksum is computed here and nowhere else, so every phrase the device
/// produces — generated or XORed — is checksummed by the same code.
fn assemble(length: SeedLength, indices: &[u16], extra_entropy: u8) -> Option<Mnemonic> {
    if indices.len() != length.entered_words() {
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

    for (slot, &index) in mnemonic.words.iter_mut().zip(indices) {
        push_bits(&mut packed, &mut position, index, BITS_PER_WORD);
        *slot = bip39_wordlist::word_at(index)?;
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

    /// Every word of a finished mnemonic, as [`parse`] takes them.
    fn spelled(mnemonic: &Mnemonic) -> heapless::Vec<Word, MAX_WORD_COUNT_TOTAL> {
        mnemonic.words().iter().copied().map(word).collect()
    }

    /// The vector phrases, completed, to XOR and re-parse.
    fn completed() -> impl Iterator<Item = (SeedLength, Mnemonic)> {
        vectors().map(|(length, phrase, extra, _)| {
            (
                length,
                complete(length, &entered(&phrase), extra).expect("vector words are in the list"),
            )
        })
    }

    #[test]
    fn parse_accepts_the_bip39_reference_vectors() {
        for (length, mnemonic) in completed() {
            assert_eq!(
                parse(length, &spelled(&mnemonic)),
                Some(mnemonic),
                "a phrase this module built was refused when read back"
            );
        }
    }

    /// The whole reason the final word is worth typing: its entropy bits are
    /// replaced by the coin flips, so the checksum is all it is read for.
    #[test]
    fn parse_refuses_a_phrase_whose_final_word_is_wrong() {
        for (length, mnemonic) in completed() {
            let mut words = spelled(&mnemonic);
            let last = words.len() - 1;

            for replacement in ["abandon", "zoo", "legal"] {
                if replacement == mnemonic.final_word() {
                    continue;
                }
                words[last] = word(replacement);

                assert_eq!(
                    parse(length, &words),
                    None,
                    "{replacement:?} passed as the final word of a {length:?} phrase"
                );
            }
        }
    }

    #[test]
    fn parse_refuses_a_phrase_of_the_wrong_length() {
        for (length, mnemonic) in completed() {
            let words = spelled(&mnemonic);

            assert_eq!(parse(length, &words[..words.len() - 1]), None);
            assert_eq!(
                parse(
                    match length {
                        Words12 => Words24,
                        Words24 => Words12,
                    },
                    &words
                ),
                None
            );
        }
    }

    #[test]
    fn xor_refuses_phrases_of_different_lengths() {
        let short = complete(Words12, &entered(&repeated("abandon", 11)), 0).unwrap();
        let long = complete(Words24, &entered(&repeated("abandon", 23)), 0).unwrap();

        assert_eq!(xor(&short, &long, 0), None);
        assert_eq!(xor(&long, &short, 0), None);
    }

    /// `x ^ x == 0`, and index 0 is `abandon`, so the entered words all collapse
    /// to it. The final word does not, because it is derived rather than XORed.
    #[test]
    fn xoring_a_phrase_with_itself_clears_every_entered_word() {
        for (length, mnemonic) in completed() {
            let result = xor(&mnemonic, &mnemonic, 0).expect("the lengths match");

            for word in &result.words()[..length.entered_words()] {
                assert_eq!(*word, "abandon");
            }
        }
    }

    #[test]
    fn xor_combines_the_entered_words_pairwise() {
        let a = complete(Words24, &entered(&repeated("zoo", 23)), 7).unwrap();
        let b = complete(
            Words24,
            &entered(&repeated(
                "letter advice cage absurd amount doctor acoustic avoid",
                23,
            )),
            0,
        )
        .unwrap();

        let result = xor(&a, &b, 0).expect("the lengths match");

        for index in 0..Words24.entered_words() {
            let index_of = |word| bip39_wordlist::index_of(word).unwrap();

            assert_eq!(
                index_of(result.words()[index]),
                index_of(a.words()[index]) ^ index_of(b.words()[index]),
            );
        }
    }

    /// Whatever goes in, what comes out is a phrase another wallet will accept.
    ///
    /// Every pair of vectors of a length, against the extremes of the entropy
    /// the final word can carry. `resolve` scans the wordlist per word, so
    /// sweeping all 256 byte values here would cost a minute to re-cover what
    /// [`every_extra_value_yields_a_distinct_final_word`] already covers.
    #[test]
    fn every_xor_result_is_a_phrase_that_parses() {
        for (length, a) in completed() {
            let mask = (1u8 << length.final_word_entropy_bits()) - 1;

            for (other, b) in completed() {
                if other != length {
                    continue;
                }

                for extra in [0, 1, mask / 2, mask] {
                    let result = xor(&a, &b, extra).expect("the lengths match");

                    assert_eq!(
                        parse(length, &spelled(&result)),
                        Some(result),
                        "a XORed phrase failed its own checksum"
                    );
                }
            }
        }
    }

    /// The other half of the sweep above, kept to one pair so every value of the
    /// final word's entropy is still exercised through [`xor`].
    #[test]
    fn xor_accepts_every_value_the_flips_can_produce() {
        for (length, a) in completed() {
            for extra in 0..1u16 << length.final_word_entropy_bits() {
                let result = xor(&a, &a, extra as u8).expect("the lengths match");

                assert_eq!(parse(length, &spelled(&result)), Some(result));
            }
        }
    }

    /// The property the tool deliberately gives up, pinned so nobody restores it
    /// by accident and nobody documents it as seed splitting: the final word's
    /// entropy comes from the flips, so XORing back does not return the input.
    #[test]
    fn xor_is_not_reversible_through_the_final_word() {
        let a = complete(Words12, &entered(&repeated("zoo", 11)), 127).unwrap();
        let b = complete(
            Words12,
            &entered(&repeated(
                "legal winner thank year wave sausage worth useful",
                11,
            )),
            42,
        )
        .unwrap();

        let result = xor(&a, &b, 0).expect("the lengths match");
        let back = xor(&result, &b, 0).expect("the lengths match");

        // Every word but the last comes back, which is what makes the loss
        // specific rather than general.
        assert_eq!(
            back.words()[..Words12.entered_words()],
            a.words()[..Words12.entered_words()]
        );
        assert_ne!(back.final_word(), a.final_word());
        assert_ne!(back, a);
    }
}
