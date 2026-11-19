//! Terminal presentation and interaction for Inifra's interactive workflows.
#![deny(missing_docs)]

mod init;
mod model;
pub mod presentation;
mod view;

pub use init::run_init;
pub use model::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, TemplateChoice, TemplateView,
};
