//! What the panel should be showing, as the application sees it.

use sporo_core::{
    bip39::{Mnemonic, SeedLength},
    flips::Flips,
};

use crate::{menu::MenuItem, word_entry::WordEntry};

/// Words of a finished phrase shown at once. A 12-word phrase is one page; a
/// 24-word one is two, turned with `Left` and `Right`.
pub const PHRASE_PAGE_SIZE: usize = 12;

/// Pages `mnemonic` takes at [`PHRASE_PAGE_SIZE`] words apiece.
pub fn phrase_pages(mnemonic: &Mnemonic) -> usize {
    mnemonic.words().len().div_ceil(PHRASE_PAGE_SIZE)
}

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
    Menu {
        selected: MenuItem,
    },
    About {
        version: &'static str,
    },
    /// Choosing how long the phrases a tool is about to read are.
    SeedLengthPick {
        selected: SeedLength,
    },
    Words {
        entry: &'a WordEntry,
        /// Which phrase is being typed, for a tool that reads more than one.
        /// `None` when there is only one and saying so would be noise.
        label: Option<&'static str>,
        /// Why the phrase as it stands was refused, shown in place of the
        /// legend. `None` when nothing is wrong.
        notice: Option<&'static str>,
    },
    Coin(&'a Flips),
    /// `page` counts from 0, and is always below [`phrase_pages`].
    Phrase {
        mnemonic: &'a Mnemonic,
        page: usize,
    },
}
