//! What a phrase *means*: the wordlist, the BIP-39 arithmetic, and how coin
//! flips pack into the final word.
//!
//! Nothing here knows about keys, cursors or screens — that is `sporo-app` and
//! `sporo-ui`, which depend on this and never the other way round. Kept apart
//! from the firmware so it can be built and tested for the host: the wordlist
//! arithmetic decides whether a phrase restores in another wallet, and that is
//! not a thing to verify only by reading it on a 135x240 panel.

// `std` only for the test harness; nothing outside `#[cfg(test)]` may use it.
#![cfg_attr(not(test), no_std)]

pub mod bip39;
pub mod bip39_wordlist;
pub mod flips;
