//! Coin flips, packed into the entropy the final word carries.
//!
//! The entered words fix all but a few of a phrase's entropy bits: 121 of 128
//! for a 12-word phrase, 253 of 256 for a 24-word one. The remaining seven, or
//! three, come from here — one coin flip each.
//!
//! The chip's hardware RNG used to supply these bits. It is a real entropy
//! source, but a seed generator whose randomness comes out of an opaque block on
//! the die asks the user to trust exactly what this device exists not to trust; a
//! coin on a table does not. See [`crate::bip39`] for how far that reaches — it
//! is seven bits of 128 (or three of 256), and the rest are the words the user
//! chose.
//!
//! This is the packing alone: how a sequence of flips becomes bits, and in which
//! order. It belongs with the BIP-39 arithmetic rather than with the screen that
//! collects the flips, because getting it backwards produces a valid mnemonic for
//! a different wallet — and nothing downstream could tell.

use crate::bip39::SeedLength;

/// The most coin flips any [`SeedLength`] needs — the 12-word phrase's seven.
pub const MAX_FLIP_COUNT: usize = 7;

// The flips are packed into a single `u8`. A mnemonic length that pushed its
// final word's entropy past eight bits would start shifting the earliest flip
// out of the byte rather than failing to build.
const _: () = {
    assert!(MAX_FLIP_COUNT <= u8::BITS as usize);

    let mut index = 0;
    while index < SeedLength::ALL.len() {
        assert!(SeedLength::ALL[index].final_word_entropy_bits() <= MAX_FLIP_COUNT);
        index += 1;
    }
};

/// One recorded flip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flip {
    Heads,
    Tails,
}

#[derive(Clone)]
pub struct Flips {
    /// The flips so far, right-aligned: the first flip is the highest of the
    /// [`Self::count`] bits in use. Grown by a shift, so it is already in the
    /// order [`crate::bip39::complete`] wants by the time the last one lands.
    bits: u8,
    count: usize,
    /// Flips the phrase needs: one per bit of entropy its final word carries on
    /// top of the checksum.
    required: usize,
}

impl Flips {
    pub const fn new(length: SeedLength) -> Self {
        Self {
            bits: 0,
            count: 0,
            required: length.final_word_entropy_bits(),
        }
    }

    /// Flips the phrase needs in all, which is also the number of slots in the
    /// row.
    pub const fn required(&self) -> usize {
        self.required
    }

    /// Flips recorded so far, which is also the slot the next one lands in.
    pub const fn count(&self) -> usize {
        self.count
    }

    pub const fn is_complete(&self) -> bool {
        self.count == self.required
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

    /// The flips as the low [`Self::required`] bits of a byte, or `None` until every
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

    /// Appends a flip. Returns whether it was taken: once every flip is in,
    /// another would shift the first one out of the byte, changing a phrase the
    /// user may already have written down, so it is refused.
    ///
    /// Shifted in rather than or-ed at a fixed position, so the value stays
    /// right-aligned as it grows and the first flip ends up the highest bit —
    /// the same direction [`crate::bip39::complete`] packs the words in. Reading
    /// the row left to right then gives the binary number a person would write
    /// down.
    pub fn record(&mut self, flip: Flip) -> bool {
        if self.is_complete() {
            return false;
        }

        self.bits = (self.bits << 1) | u8::from(matches!(flip, Flip::Heads));
        self.count += 1;

        true
    }

    /// Takes the last flip back. Returns whether there was one to take.
    pub fn undo(&mut self) -> bool {
        if self.count == 0 {
            return false;
        }

        self.bits >>= 1;
        self.count -= 1;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records `pattern` left to right, reading each character as a binary digit:
    /// `1` for heads and `0` for tails. These are bits, not keys — which key
    /// records which flip is the application's business, not this module's.
    ///
    /// The phrase length is the one whose row `pattern` fills, so a pattern of
    /// seven is a 12-word phrase and three a 24-word one.
    fn entered(pattern: &str) -> Flips {
        let length = SeedLength::ALL
            .into_iter()
            .find(|length| length.final_word_entropy_bits() == pattern.len())
            .expect("a pattern fills some phrase length's row");

        let mut flips = Flips::new(length);
        for digit in pattern.chars() {
            flips.record(if digit == '1' {
                Flip::Heads
            } else {
                Flip::Tails
            });
        }

        flips
    }

    #[test]
    fn a_new_entry_has_no_flips_and_no_entropy() {
        for length in SeedLength::ALL {
            let flips = Flips::new(length);

            assert_eq!(flips.count(), 0);
            assert!(!flips.is_complete());
            assert_eq!(flips.entropy(), None);
            assert_eq!(flips.flip(0), None);
        }
    }

    #[test]
    fn each_length_needs_as_many_flips_as_its_final_word_has_entropy_bits() {
        assert_eq!(Flips::new(SeedLength::Words12).required(), 7);
        assert_eq!(Flips::new(SeedLength::Words24).required(), 3);
    }

    #[test]
    fn the_first_flip_is_the_most_significant_bit() {
        // Heads entered first weighs 64; the same flip entered last weighs 1.
        // Getting this backwards still produces a valid mnemonic — just one for
        // a different wallet — so no test of the final word alone would catch
        // it. That is why the two patterns here are not mirror images.
        assert_eq!(entered("1000000").entropy(), Some(0b100_0000));
        assert_eq!(entered("0000001").entropy(), Some(0b000_0001));

        // A 24-word phrase packs its three the same way.
        assert_eq!(entered("100").entropy(), Some(0b100));
        assert_eq!(entered("001").entropy(), Some(0b001));
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
        let flips = entered(pattern);

        for (index, digit) in pattern.chars().enumerate() {
            let expected = if digit == '1' {
                Flip::Heads
            } else {
                Flip::Tails
            };

            assert_eq!(flips.flip(index), Some(expected), "at slot {index}");
        }

        assert_eq!(flips.flip(flips.required()), None);
    }

    #[test]
    fn a_slot_past_the_last_flip_is_empty() {
        // Mid-entry the row is part drawn, and the screen tells the two apart by
        // this `None` alone — there is no separate count on the drawing side.
        let flips = entered("101");

        assert_eq!(flips.flip(2), Some(Flip::Heads));
        assert_eq!(flips.flip(3), None);
    }

    #[test]
    fn entropy_is_withheld_until_every_flip_is_in() {
        for length in SeedLength::ALL {
            let mut flips = Flips::new(length);

            for _ in 0..flips.required() - 1 {
                flips.record(Flip::Tails);
                assert_eq!(flips.entropy(), None, "at {} flips", flips.count());
            }

            flips.record(Flip::Tails);
            assert_eq!(flips.entropy(), Some(0));
        }
    }

    #[test]
    fn every_sequence_of_flips_gives_a_different_value() {
        // Pairs with `bip39::tests::every_extra_value_yields_a_distinct_final_word`:
        // that one proves the 128 values map onto 128 distinct twelfth words,
        // and this one proves the 128 flip sequences map onto the 128 values. A
        // collision here would quietly halve the entropy the user thinks they
        // provided.
        for length in SeedLength::ALL {
            let required = length.final_word_entropy_bits();
            let mut seen = [false; 1 << MAX_FLIP_COUNT];

            for value in 0..(1u8 << required) {
                let mut flips = Flips::new(length);
                for bit in (0..required).rev() {
                    flips.record(if (value >> bit) & 1 == 1 {
                        Flip::Heads
                    } else {
                        Flip::Tails
                    });
                }

                let entropy = flips.entropy().expect("every flip was entered");
                assert!(!seen[usize::from(entropy)], "{entropy} came up twice");
                seen[usize::from(entropy)] = true;
            }
        }
    }

    #[test]
    fn undo_takes_the_last_flip_back() {
        let mut flips = entered("101");

        assert!(flips.undo());
        assert_eq!(flips.count(), 2);
        assert_eq!(flips.flip(2), None);

        // The slot is genuinely free again, not merely hidden: flipping the
        // other way has to change the value.
        flips.record(Flip::Tails);
        assert_eq!(flips.flip(2), Some(Flip::Tails));
    }

    #[test]
    fn undo_with_nothing_recorded_does_nothing() {
        let mut flips = Flips::new(SeedLength::Words12);

        assert!(!flips.undo());
        assert_eq!(flips.count(), 0);
    }

    #[test]
    fn a_finished_entry_takes_no_more_flips() {
        let mut flips = entered("1111111");

        // An eighth flip would shift the first one out of the byte, changing a
        // phrase the user may already have written down.
        assert!(!flips.record(Flip::Heads));
        assert!(!flips.record(Flip::Tails));
        assert_eq!(flips.count(), 7);
        assert_eq!(flips.entropy(), Some(0b111_1111));

        // A 24-word row is full at three.
        let mut flips = entered("101");
        assert!(flips.is_complete());
        assert!(!flips.record(Flip::Heads));
        assert_eq!(flips.entropy(), Some(0b101));
    }
}
