//! What each key does, and which screen comes next.
//!
//! Sits between [`sporo_core`], which knows what a phrase *means*, and the
//! screens, which know what it *looks like*. Everything here is interaction:
//! where a cursor is, what a keypress changes, when a workflow moves on. Nothing
//! here draws or touches a pin, so a whole workflow can be walked through on the
//! host and checked, rather than only by pressing keys on the board.

// `std` only for the test harness; nothing outside `#[cfg(test)]` may use it.
#![cfg_attr(not(test), no_std)]

pub mod action;
pub mod app;
mod generate;
pub mod menu;
pub mod view;
pub mod word_entry;
mod workflow;
mod xor;
