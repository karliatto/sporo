//! The keypad legend along the bottom of a screen, composed from the keymap.
//!
//! Each screen describes its legend as which actions to offer and what to call
//! them; the key characters come from [`crate::keymap::key`]. Typing the legend
//! out as a literal is what this replaces: a literal restates the binding, and
//! nothing checks the restatement, so a rebinding compiled, passed, and left the
//! panel naming a key that did nothing.

use heapless::String;
use sporo_app::action::Action;

use crate::keymap;

/// Longest a legend can be. Every legend is checked against it by the tests that
/// compose them, and the longest today is 30 bytes.
pub(crate) const LEGEND_LEN: usize = 32;

/// One item in a legend: the keys that do a thing, and what to call the thing.
pub(crate) struct Hint {
    keys: &'static [Action],
    label: &'static str,
}

impl Hint {
    pub(crate) const fn new(keys: &'static [Action], label: &'static str) -> Self {
        Self { keys, label }
    }

    /// Text with no key in front of it — "phrase complete", say.
    pub(crate) const fn note(label: &'static str) -> Self {
        Self { keys: &[], label }
    }

    /// Whether this hint names `action`'s key.
    #[cfg(test)]
    pub(crate) fn offers(&self, action: Action) -> bool {
        self.keys.contains(&action)
    }
}

/// Lays `hints` out as one line: the keys of a hint joined by `/` and followed
/// by its label, and two spaces between one hint and the next.
///
/// Panics rather than truncating if the line outgrows [`LEGEND_LEN`]: a clipped
/// legend names a key that is not there, which is worse than no legend.
pub(crate) fn compose(hints: &[Hint]) -> String<LEGEND_LEN> {
    let mut line = String::new();

    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            push(&mut line, "  ");
        }

        for (position, action) in hint.keys.iter().enumerate() {
            if position > 0 {
                push_char(&mut line, '/');
            }
            push_char(&mut line, keymap::key(*action));
        }

        if !hint.keys.is_empty() {
            push_char(&mut line, ' ');
        }
        push(&mut line, hint.label);
    }

    line
}

fn push(line: &mut String<LEGEND_LEN>, text: &str) {
    line.push_str(text)
        .expect("every legend is composed in the tests, so LEGEND_LEN fits them all");
}

fn push_char(line: &mut String<LEGEND_LEN>, letter: char) {
    line.push(letter)
        .expect("every legend is composed in the tests, so LEGEND_LEN fits them all");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_one_hint_are_joined_by_a_slash() {
        let line = compose(&[Hint::new(&[Action::Left, Action::Right], "pick")]);

        assert_eq!(line, "4/6 pick");
    }

    #[test]
    fn hints_are_separated_by_two_spaces() {
        let line = compose(&[
            Hint::new(&[Action::Select], "add"),
            Hint::new(&[Action::Back], "del"),
        ]);

        assert_eq!(line, "5 add  * del");
    }

    #[test]
    fn a_note_has_no_key_in_front_of_it() {
        let line = compose(&[
            Hint::note("phrase complete"),
            Hint::new(&[Action::Back], "to edit"),
        ]);

        assert_eq!(line, "phrase complete  * to edit");
    }
}
