//! Turning entered words into a BIP-39 mnemonic.
//!
//! The entry screen collects [`WORD_COUNT`] words; this module resolves what was
//! typed against the official wordlist and derives the final word that makes the
//! phrase a valid mnemonic.
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
//! Those 7 bits have to come from somewhere, which is what the TRNG is for in
//! `main`: there are 2^7 = 128 equally valid twelfth words for any given eleven,
//! and picking one by hand — or always taking the first — would throw that
//! entropy away.

use sha2::{Digest, Sha256};

use crate::{
    bip39_wordlist,
    word_entry::{Word, WORD_COUNT},
};

/// Bits each word contributes: the wordlist has 2^11 entries.
const BITS_PER_WORD: usize = 11;

/// Words in the finished mnemonic: the [`WORD_COUNT`] entered plus the derived
/// one.
pub const WORD_COUNT_TOTAL: usize = WORD_COUNT + 1;

/// Entropy bits in a 12-word mnemonic.
const ENTROPY_BITS: usize = 128;

/// Checksum bits, `ENTROPY_BITS / 32` per BIP-39.
const CHECKSUM_BITS: usize = 4;

/// Bits of fresh entropy the final word carries on top of the checksum.
pub const FINAL_WORD_ENTROPY_BITS: usize = ENTROPY_BITS - BITS_PER_WORD * WORD_COUNT;

// Getting this arithmetic wrong would produce phrases no other wallet accepts,
// and only for some inputs, so it is checked at compile time rather than
// trusted. Changing WORD_COUNT to target a different mnemonic length trips
// these: the checksum grows to 8 bits for a 24-word phrase.
const _: () = assert!(BITS_PER_WORD * WORD_COUNT_TOTAL == ENTROPY_BITS + CHECKSUM_BITS);
const _: () = assert!(FINAL_WORD_ENTROPY_BITS + CHECKSUM_BITS == BITS_PER_WORD);
const _: () = assert!(bip39_wordlist::COUNT == 1 << BITS_PER_WORD);

/// A finished mnemonic: the entered words followed by the derived final one, all
/// in the canonical lowercase spelling.
pub type Mnemonic = [&'static str; WORD_COUNT_TOTAL];

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

/// Completes a mnemonic from the entered words plus fresh entropy.
///
/// Only the low [`FINAL_WORD_ENTROPY_BITS`] of `extra_entropy` are used; the
/// rest are ignored, so the caller can pass a whole random byte.
///
/// Returns `None` unless exactly [`WORD_COUNT`] words were given and each one
/// resolves — which the entry screen guarantees, since it resolves words before
/// accepting them.
pub fn complete(entered: &[Word], extra_entropy: u8) -> Option<Mnemonic> {
    if entered.len() != WORD_COUNT {
        return None;
    }

    let mut mnemonic: Mnemonic = [""; WORD_COUNT_TOTAL];

    // 121 bits from the entered words followed by 7 fresh ones lands exactly on
    // the 128 bits of entropy a 12-word mnemonic needs, so the whole thing fits
    // in a single u128 and needs no bit-buffer of its own.
    let mut entropy: u128 = 0;
    for (slot, word) in mnemonic.iter_mut().zip(entered) {
        let canonical = resolve(word)?;
        entropy = (entropy << BITS_PER_WORD) | u128::from(bip39_wordlist::index_of(canonical)?);
        *slot = canonical;
    }

    let extra = u16::from(extra_entropy) & ((1 << FINAL_WORD_ENTROPY_BITS) - 1);
    entropy = (entropy << FINAL_WORD_ENTROPY_BITS) | u128::from(extra);

    // BIP-39 checksum: the leading CHECKSUM_BITS of SHA-256 over the entropy.
    let digest = Sha256::digest(entropy.to_be_bytes());
    let checksum = u16::from(digest[0] >> (8 - CHECKSUM_BITS));

    mnemonic[WORD_COUNT] = bip39_wordlist::word_at((extra << CHECKSUM_BITS) | checksum)?;

    Some(mnemonic)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str) -> Word {
        let mut word = Word::new();
        for letter in text.chars() {
            word.push(letter).expect("test words fit MAX_WORD_LEN");
        }

        word
    }

    fn entered(phrase: &str) -> [Word; WORD_COUNT] {
        let mut words = core::array::from_fn(|_| Word::new());
        let mut count = 0;

        for (slot, text) in words.iter_mut().zip(phrase.split_whitespace()) {
            *slot = word(text);
            count += 1;
        }
        assert_eq!(count, WORD_COUNT, "a phrase is {WORD_COUNT} words");

        words
    }

    /// The 128-bit vectors from BIP-39 itself, split the way this module splits
    /// them: the first eleven words are entered, and the low 7 bits of the
    /// entropy are what `main` would draw from the TRNG.
    ///
    /// Getting this wrong yields phrases that look right and restore as a
    /// different wallet, or as none, so it is checked against the spec's own
    /// numbers rather than against this implementation's idea of them.
    const VECTORS: [(&str, u8, &str); 4] = [
        (
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon",
            0,
            "about",
        ),
        (
            "legal winner thank year wave sausage worth useful legal winner thank",
            127,
            "yellow",
        ),
        (
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage",
            0,
            "above",
        ),
        ("zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo", 127, "wrong"),
    ];

    #[test]
    fn completes_the_bip39_reference_vectors() {
        for (phrase, extra, expected) in VECTORS {
            let mnemonic = complete(&entered(phrase), extra).expect("vector words are in the list");

            assert_eq!(mnemonic[WORD_COUNT], expected, "for {phrase:?}");
            for (produced, given) in mnemonic.iter().zip(phrase.split_whitespace()) {
                assert_eq!(*produced, given);
            }
        }
    }

    #[test]
    fn completes_the_same_vectors_entered_in_upper_case() {
        // What the entry screen actually stores.
        for (phrase, extra, expected) in VECTORS {
            let upper: heapless::String<128> =
                phrase.chars().map(|c| c.to_ascii_uppercase()).collect();
            let mnemonic = complete(&entered(&upper), extra).expect("case is not significant");

            assert_eq!(mnemonic[WORD_COUNT], expected);
        }
    }

    #[test]
    fn only_the_low_bits_of_the_extra_byte_are_used() {
        let words = entered(VECTORS[0].0);

        // The high bit is above FINAL_WORD_ENTROPY_BITS and must be discarded
        // rather than shifted into the phrase.
        assert_eq!(
            complete(&words, 0b1000_0000).unwrap(),
            complete(&words, 0).unwrap()
        );
        assert_ne!(complete(&words, 1).unwrap(), complete(&words, 0).unwrap());
    }

    #[test]
    fn every_extra_value_yields_a_distinct_final_word() {
        // 7 bits of entropy is only worth carrying if all 128 values land
        // somewhere different.
        let words = entered(VECTORS[0].0);
        let mut seen = [""; 1 << FINAL_WORD_ENTROPY_BITS];

        for extra in 0..(1u8 << FINAL_WORD_ENTROPY_BITS) {
            seen[usize::from(extra)] = complete(&words, extra).unwrap()[WORD_COUNT];
        }

        for (index, word) in seen.iter().enumerate() {
            assert!(
                !seen[..index].contains(word),
                "{word:?} produced by two different extras"
            );
        }
    }

    #[test]
    fn a_short_phrase_does_not_complete() {
        assert_eq!(complete(&[word("ABANDON")], 0), None);
        assert_eq!(complete(&[], 0), None);
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
