//! State for the coin-flip screen: the entropy the final word carries.
//!
//! The eleven typed words fix 121 of a phrase's 128 entropy bits. The remaining
//! seven come from here — one coin flip each, entered on the keypad:
//!
//! | Key | Action                                             |
//! | --- | -------------------------------------------------- |
//! | `1` | record a heads                                      |
//! | `0` | record a tails                                      |
//! | `*` | take the last flip back, or leave once none are left |
//! | `#` | accept the flips, once every one of them is in      |
//!
//! The chip's hardware RNG used to supply these bits. It is a real entropy
//! source, but a seed generator whose randomness comes out of an opaque block on
//! the die asks the user to trust exactly what this device exists not to trust; a
//! coin on a table does not. See [`crate::bip39`] for how far that reaches — it
//! is seven bits of 128, and the other 121 are the words the user chose.
//!
//! Pure state — no display or GPIO — so the screen code can render it and the
//! main loop can drive it without either knowing about the other.

use crate::{
    bip39::FINAL_WORD_ENTROPY_BITS,
    word_entry::{KEY_ACCEPT, KEY_DELETE, KEY_HEADS, KEY_TAILS},
};

/// Coin flips a phrase needs: one per bit of entropy the final word carries on
/// top of the checksum.
pub const FLIP_COUNT: usize = FINAL_WORD_ENTROPY_BITS;

// The flips are packed into a single `u8`. A mnemonic length that pushed
// FINAL_WORD_ENTROPY_BITS past eight would start shifting the earliest flip out
// of the byte rather than failing to build.
const _: () = assert!(FLIP_COUNT <= u8::BITS as usize);

/// One recorded flip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flip {
    Heads,
    Tails,
}

/// What a keypress did, for the caller to act on.
///
/// Richer than the `bool` [`crate::word_entry::WordEntry::handle_key`] returns,
/// for the same reason [`crate::word_entry`]'s caller can get away with one and
/// this one cannot: `#` means "go on to the phrase", but only once every flip is
/// in, and returning a bare "something changed" would leave the firmware to
/// re-derive that rule for itself. [`Self::Ignored`] still serves the `bool`'s
/// purpose — it keeps the full-screen blit off keys that changed nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoinEvent {
    /// Nothing to do, and nothing to redraw.
    Ignored,
    /// A flip was recorded or taken back; the screen needs drawing again.
    Changed,
    /// Every flip is in and the user accepted them.
    Confirmed,
    /// `*` with nothing left to take back: the user backed out.
    Dismissed,
}

pub struct CoinEntry {
    /// The flips so far, right-aligned: the first flip is the highest of the
    /// [`Self::count`] bits in use. Grown by a shift, so it is already in the
    /// order [`crate::bip39::complete`] wants by the time the last one lands.
    bits: u8,
    count: usize,
}

impl Default for CoinEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl CoinEntry {
    pub const fn new() -> Self {
        Self { bits: 0, count: 0 }
    }

    /// Flips recorded so far, which is also the slot the next one lands in.
    pub const fn count(&self) -> usize {
        self.count
    }

    pub const fn is_complete(&self) -> bool {
        self.count == FLIP_COUNT
    }

    /// The flip recorded in `index`, or `None` for a slot not yet flipped —
    /// which is what lets the screen draw the row without knowing anything about
    /// how the bits are packed.
    pub const fn flip(&self, index: usize) -> Option<Flip> {
        if index >= self.count {
            return None;
        }

        // Slot `index` sits this many places above the bottom of what has been
        // entered so far, which holds mid-entry as well as once the row is full.
        if (self.bits >> (self.count - 1 - index)) & 1 == 1 {
            Some(Flip::Heads)
        } else {
            Some(Flip::Tails)
        }
    }

    /// The flips as the low [`FLIP_COUNT`] bits of a byte, or `None` until every
    /// one of them is in.
    ///
    /// Withheld rather than returned short: a partial byte reads as a complete
    /// one — three flips would come out as four tails followed by them — and
    /// would complete a phrase the user never authorised.
    pub const fn entropy(&self) -> Option<u8> {
        if self.is_complete() {
            Some(self.bits)
        } else {
            None
        }
    }

    /// Applies a keypress.
    pub fn handle_key(&mut self, key: char) -> CoinEvent {
        match key {
            KEY_HEADS if !self.is_complete() => self.record(Flip::Heads),
            KEY_TAILS if !self.is_complete() => self.record(Flip::Tails),
            // Backspace first and leave only once there is nothing left to take
            // back, the way `*` walks out of a word one letter at a time rather
            // than abandoning it in one press.
            KEY_DELETE => {
                if self.count == 0 {
                    return CoinEvent::Dismissed;
                }

                self.bits >>= 1;
                self.count -= 1;

                CoinEvent::Changed
            }
            KEY_ACCEPT if self.is_complete() => CoinEvent::Confirmed,
            _ => CoinEvent::Ignored,
        }
    }

    /// Appends a flip.
    ///
    /// Shifted in rather than or-ed at a fixed position, so the value stays
    /// right-aligned as it grows and the first flip ends up the highest bit —
    /// the same direction [`crate::bip39::complete`] packs the words in. Reading
    /// the row left to right then gives the binary number a person would write
    /// down.
    fn record(&mut self, flip: Flip) -> CoinEvent {
        self.bits = (self.bits << 1) | u8::from(matches!(flip, Flip::Heads));
        self.count += 1;

        CoinEvent::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{bip39, word_entry::WordEntry};

    /// Enters `pattern` one key per flip, the way a user would: `1` for heads
    /// and `0` for tails, left to right.
    fn entered(pattern: &str) -> CoinEntry {
        let mut coins = CoinEntry::new();
        for flip in pattern.chars() {
            coins.handle_key(flip);
        }

        coins
    }

    #[test]
    fn a_new_entry_has_no_flips_and_no_entropy() {
        let coins = CoinEntry::new();

        assert_eq!(coins.count(), 0);
        assert!(!coins.is_complete());
        assert_eq!(coins.entropy(), None);
        assert_eq!(coins.flip(0), None);
    }

    #[test]
    fn the_first_flip_is_the_most_significant_bit() {
        // Heads entered first weighs 64; the same flip entered last weighs 1.
        // Getting this backwards still produces a valid mnemonic — just one for
        // a different wallet — so no test of the final word alone would catch
        // it. That is why the two patterns here are not mirror images.
        assert_eq!(entered("1000000").entropy(), Some(0b100_0000));
        assert_eq!(entered("0000001").entropy(), Some(0b000_0001));
    }

    #[test]
    fn all_heads_sets_every_bit() {
        assert_eq!(entered("1111111").entropy(), Some(0b111_1111));
    }

    #[test]
    fn all_tails_sets_none() {
        assert_eq!(entered("0000000").entropy(), Some(0));
    }

    #[test]
    fn every_flip_reads_back_where_it_was_entered() {
        let pattern = "1101001";
        let coins = entered(pattern);

        for (index, key) in pattern.chars().enumerate() {
            let expected = if key == KEY_HEADS {
                Flip::Heads
            } else {
                Flip::Tails
            };

            assert_eq!(coins.flip(index), Some(expected), "at slot {index}");
        }

        assert_eq!(coins.flip(FLIP_COUNT), None);
    }

    #[test]
    fn a_slot_past_the_last_flip_is_empty() {
        // Mid-entry the row is part drawn, and the screen tells the two apart by
        // this `None` alone — there is no separate count on the drawing side.
        let coins = entered("101");

        assert_eq!(coins.flip(2), Some(Flip::Heads));
        assert_eq!(coins.flip(3), None);
    }

    #[test]
    fn entropy_is_withheld_until_every_flip_is_in() {
        let mut coins = CoinEntry::new();

        for _ in 0..FLIP_COUNT - 1 {
            coins.handle_key(KEY_TAILS);
            assert_eq!(coins.entropy(), None, "at {} flips", coins.count());
        }

        coins.handle_key(KEY_TAILS);
        assert_eq!(coins.entropy(), Some(0));
    }

    #[test]
    fn every_sequence_of_flips_gives_a_different_value() {
        // Pairs with `bip39::tests::every_extra_value_yields_a_distinct_final_word`:
        // that one proves the 128 values map onto 128 distinct twelfth words,
        // and this one proves the 128 flip sequences map onto the 128 values. A
        // collision here would quietly halve the entropy the user thinks they
        // provided.
        let mut seen = [false; 1 << FLIP_COUNT];

        for value in 0..(1u8 << FLIP_COUNT) {
            let mut coins = CoinEntry::new();
            for bit in (0..FLIP_COUNT).rev() {
                coins.handle_key(if (value >> bit) & 1 == 1 {
                    KEY_HEADS
                } else {
                    KEY_TAILS
                });
            }

            let entropy = coins.entropy().expect("every flip was entered");
            assert!(!seen[usize::from(entropy)], "{entropy} came up twice");
            seen[usize::from(entropy)] = true;
        }
    }

    #[test]
    fn undo_takes_the_last_flip_back() {
        let mut coins = entered("101");

        assert_eq!(coins.handle_key(KEY_DELETE), CoinEvent::Changed);
        assert_eq!(coins.count(), 2);
        assert_eq!(coins.flip(2), None);

        // The slot is genuinely free again, not merely hidden: flipping the
        // other way has to change the value.
        coins.handle_key(KEY_TAILS);
        assert_eq!(coins.flip(2), Some(Flip::Tails));
    }

    #[test]
    fn undo_on_an_empty_entry_backs_out() {
        let mut coins = CoinEntry::new();

        assert_eq!(coins.handle_key(KEY_DELETE), CoinEvent::Dismissed);
        assert_eq!(coins.count(), 0);
    }

    #[test]
    fn a_finished_entry_takes_no_more_flips() {
        let mut coins = entered("1111111");

        // An eighth flip would shift the first one out of the byte, changing a
        // phrase the user may already have written down.
        assert_eq!(coins.handle_key(KEY_HEADS), CoinEvent::Ignored);
        assert_eq!(coins.handle_key(KEY_TAILS), CoinEvent::Ignored);
        assert_eq!(coins.count(), FLIP_COUNT);
        assert_eq!(coins.entropy(), Some(0b111_1111));
    }

    #[test]
    fn accept_is_inert_until_every_flip_is_in() {
        let mut coins = CoinEntry::new();

        for _ in 0..FLIP_COUNT - 1 {
            assert_eq!(coins.handle_key(KEY_ACCEPT), CoinEvent::Ignored);
            coins.handle_key(KEY_HEADS);
        }

        assert_eq!(coins.handle_key(KEY_ACCEPT), CoinEvent::Ignored);
        coins.handle_key(KEY_HEADS);
        assert_eq!(coins.handle_key(KEY_ACCEPT), CoinEvent::Confirmed);
    }

    #[test]
    fn accepting_does_not_change_the_flips() {
        let mut coins = entered("1011010");
        let first = coins.entropy();

        // Coming back from the phrase to edit a word and accepting again lands
        // here a second time; the same flips have to give the same byte, or the
        // twelfth word changes under a user who already wrote it down.
        assert_eq!(coins.handle_key(KEY_ACCEPT), CoinEvent::Confirmed);
        assert_eq!(coins.handle_key(KEY_ACCEPT), CoinEvent::Confirmed);
        assert_eq!(coins.entropy(), first);
    }

    #[test]
    fn unbound_keys_are_ignored() {
        let mut coins = CoinEntry::new();

        for key in ['2', '3', '4', '5', '6', '7', '8', '9'] {
            assert_eq!(
                coins.handle_key(key),
                CoinEvent::Ignored,
                "{key:?} was bound"
            );
        }

        assert_eq!(coins.count(), 0);
    }

    #[test]
    fn the_flips_complete_the_bip39_reference_vectors() {
        // Both of these entropy values are bit-palindromes, so they say nothing
        // about the order the flips are packed in — that is
        // `the_first_flip_is_the_most_significant_bit`'s job. What they check is
        // the seam: that what this module hands `bip39::complete` is what the
        // spec's own vectors expect to receive.
        let vectors = [
            (
                "abandon abandon abandon abandon abandon abandon abandon abandon \
                 abandon abandon abandon",
                "0000000",
                "about",
            ),
            (
                "legal winner thank year wave sausage worth useful legal winner thank",
                "1111111",
                "yellow",
            ),
        ];

        for (phrase, flips, expected) in vectors {
            let mut entry = WordEntry::new();
            for word in phrase.split_whitespace() {
                for letter in word.chars() {
                    while entry.selected() != Some(letter.to_ascii_uppercase()) {
                        entry.handle_key(crate::word_entry::KEY_NEXT);
                    }
                    entry.handle_key(crate::word_entry::KEY_ADD);
                }
                entry.handle_key(KEY_ACCEPT);
            }

            let entropy = entered(flips).entropy().expect("every flip was entered");
            let mnemonic =
                bip39::complete(entry.accepted(), entropy).expect("vector words are in the list");

            assert_eq!(
                mnemonic[bip39::WORD_COUNT_TOTAL - 1],
                expected,
                "for {flips:?}"
            );
        }
    }
}
