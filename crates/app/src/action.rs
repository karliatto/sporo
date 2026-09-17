#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    Confirm,
    Heads,
    Tails,
}

impl Action {
    /// Every action, so a keymap can be checked for one that has no key.
    pub const ALL: [Self; 9] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Select,
        Self::Back,
        Self::Confirm,
        Self::Heads,
        Self::Tails,
    ];
}
