//! Reusable inline controls shared by interactive workflows.

pub(crate) mod chrome;
mod forms;
mod select;

/// Result of an interactive selection screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InteractionOutcome<T> {
    /// The user confirmed the current selection.
    Selected(T),
    /// The user pressed Esc to return to the previous screen.
    Back,
    /// The user requested to quit the interactive command.
    Quit,
}

pub use chrome::{Badge, Heading, OutlineRow, Row, StepState};
pub use forms::{FieldCheck, FormOutcome, TextField, run_confirmation, run_form, run_secret_input};
pub use select::{SelectEvent, SelectScreen, run_select, run_select_marking};
