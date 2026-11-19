//! Terminal presentation and interaction for Inifra's interactive workflows.
#![deny(missing_docs)]

mod chat;
mod forms;
mod init;
mod markdown;
mod model;
pub mod presentation;
mod select;
mod terminal;
mod view;

pub use chat::{ChatAction, ChatConfig, ChatEvent, ChatExit, ChatUsage, run_chat};
pub use forms::{FormOutcome, TextField, run_confirmation, run_form, run_secret_input};
pub use init::{run_init, run_init_with_selection};
pub use model::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, TemplateChoice, TemplateView,
};
pub use select::run_select;
