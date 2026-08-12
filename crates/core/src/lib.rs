//! Everything the entry screen needs that a chip does not.
//!
//! Kept apart from the firmware so it can be built and tested for the host: the
//! wordlist arithmetic decides whether a phrase restores in another wallet, and
//! that is not a thing to verify only by reading it on a 135x240 panel.

// `std` only for the test harness; nothing outside `#[cfg(test)]` may use it.
#![cfg_attr(not(test), no_std)]

pub mod bip39;
pub mod bip39_wordlist;
pub mod word_entry;
