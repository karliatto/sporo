//! Which printed key asks for which [`Action`].
//!
//! The one place a key character means something. The application only ever
//! sees actions, and every legend on the panel is composed from [`key`], so a
//! rebinding here changes what the keys do and what the screens say they do in
//! the same edit — there is no second copy of the binding to fall out of step.

use sporo_app::action::Action;

/// The characters printed on the keypad, row-major from the top left — the order
/// the firmware's matrix scan indexes them in.
pub const LAYOUT: [[char; 3]; 4] = [
    ['1', '2', '3'],
    ['4', '5', '6'],
    ['7', '8', '9'],
    ['*', '0', '#'],
];

/// The key that asks for `action`.
///
/// An exhaustive `match` rather than a table, so an action added without a key
/// fails to build instead of reaching a screen as a blank in its legend.
///
/// The keys form a d-pad around `5`, which is why the names are directions:
/// `2`/`8` move up and down, `4`/`6` left and right, `5` selects. `1` and `0`
/// sit well apart from each other, so a fumbled press is unlikely to record the
/// wrong flip.
pub const fn key(action: Action) -> char {
    match action {
        Action::Up => '2',
        Action::Down => '8',
        Action::Left => '4',
        Action::Right => '6',
        Action::Select => '5',
        Action::Back => '*',
        Action::Confirm => '#',
        Action::Heads => '1',
        Action::Tails => '0',
    }
}

/// The action a pressed key asks for, or `None` for a key bound to nothing.
///
/// Derived from [`key`] rather than written out a second time, so the two
/// directions cannot disagree.
pub fn action(key_char: char) -> Option<Action> {
    Action::ALL
        .into_iter()
        .find(|action| key(*action) == key_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_the_keypad(key_char: char) -> bool {
        LAYOUT.iter().flatten().any(|printed| *printed == key_char)
    }

    #[test]
    fn every_action_has_its_own_key() {
        // Two actions on one key would make `action` return whichever comes
        // first in `Action::ALL`, silently leaving the other unreachable.
        for (index, first) in Action::ALL.iter().enumerate() {
            for second in &Action::ALL[index + 1..] {
                assert_ne!(
                    key(*first),
                    key(*second),
                    "{first:?} and {second:?} share a key"
                );
            }
        }
    }

    #[test]
    fn every_bound_key_is_on_the_keypad() {
        for action in Action::ALL {
            assert!(
                on_the_keypad(key(action)),
                "{action:?} is bound to {:?}, which is not printed on the keypad",
                key(action),
            );
        }
    }

    #[test]
    fn a_key_maps_back_to_the_action_that_owns_it() {
        for action in Action::ALL {
            assert_eq!(super::action(key(action)), Some(action));
        }
    }

    #[test]
    fn three_keys_are_left_unbound() {
        // These are why the application takes `Option<Action>`: they still
        // count as "press any key" on the home screen, and do nothing anywhere
        // else.
        let unbound: heapless::Vec<char, 12> = LAYOUT
            .iter()
            .flatten()
            .copied()
            .filter(|printed| action(*printed).is_none())
            .collect();

        assert_eq!(unbound, ['3', '7', '9']);
    }
}
