//! What the panel should be showing, as the application sees it.

use sporo_core::{bip39::Mnemonic, flips::Flips};

use crate::{menu::MenuItem, word_entry::WordEntry};

/// One screen's worth of state, borrowed from the [`crate::app::App`] that owns
/// it. The screens draw from this and nothing else.
///
/// Borrowed rather than copied: the phrase is the one thing on this device that
/// must not be left lying around in extra places.
///
/// Deliberately not `Debug`, and nor is anything it borrows that holds part of a
/// phrase. A `println!("{:?}", view)` added while chasing a bug would otherwise
/// put words or flips on the serial line; this way it does not compile.
#[derive(Clone, Copy)]
pub enum View<'a> {
    Home,
    Menu { selected: MenuItem },
    About { version: &'static str },
    Words(&'a WordEntry),
    Coin(&'a Flips),
    Phrase(&'a Mnemonic),
}
