//! Reusable inline controls shared by interactive workflows.

mod forms;
mod select;

pub use forms::{FormOutcome, TextField, run_confirmation, run_form, run_secret_input};
pub use select::run_select;
