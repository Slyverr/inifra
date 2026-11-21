//! Terminal presentation and interaction for Inifra's interactive workflows.
#![deny(missing_docs)]

mod chat;
mod controls;
mod init;
mod model;
pub mod presentation;
mod terminal;

pub use chat::{ChatAction, ChatConfig, ChatEvent, ChatExit, ChatUsage, run_chat};
pub use controls::{
    FormOutcome, TextField, run_confirmation, run_form, run_secret_input, run_select,
};
pub use init::{run_init, run_init_with_selection};
pub use model::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, SelectionPreview, TemplateChoice,
    TemplateInputView, TemplateSourceView, TemplateView,
};
