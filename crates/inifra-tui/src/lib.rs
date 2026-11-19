//! Terminal presentation and interaction for Inifra's interactive workflows.
#![deny(missing_docs)]

mod forms;
mod init;
mod model;
pub mod presentation;
mod select;
mod terminal;
mod view;

pub use forms::{FormOutcome, TextField, run_confirmation, run_form};
pub use init::{run_init, run_init_with_selection};
pub use model::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, TemplateChoice, TemplateView,
};
pub use select::run_select;
