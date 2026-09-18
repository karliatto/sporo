//! What every workflow has in common.
//!
//! A workflow is a tool the menu opens: it owns its state, it is handed one
//! [`Action`](crate::action::Action) at a time, and it says what that did. There
//! is no trait, because the three methods differ in their arguments — `new`
//! takes whatever the tool needs to start — and a trait would buy nothing the
//! `Screen` enum in `app.rs` does not already give. What a workflow does share
//! is the answer it hands back, which is here so both mean the same thing by it.

/// What an action did to a workflow, for the application to act on.
pub(crate) enum Outcome {
    /// Nothing changed; the screen does not need drawing again.
    Unchanged,
    Redraw,
    /// `Back` with nothing left to take back: the user left the workflow.
    Exit,
}
