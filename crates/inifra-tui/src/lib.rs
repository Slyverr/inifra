//! Terminal presentation and interaction for Inifra's interactive workflows.
#![deny(missing_docs)]

mod chat;
mod controls;
mod init;
mod line_edit;
mod model;
pub mod presentation;
mod terminal;

pub use chat::{
    ChatAction, ChatConfig, ChatEvent, ChatExit, ChatUsage, ModelList, SwitchRequest,
    SwitcherProfile, run_chat,
};
pub use controls::{
    Badge, FieldCheck, FormOutcome, Heading, InteractionOutcome, OutlineRow, Row, SelectEvent,
    SelectScreen, StepState, TextField, run_confirmation, run_form, run_secret_input, run_select,
    run_select_marking,
};
pub use init::{run_init, run_init_with_selection};
pub use model::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, SelectionPreview, TemplateChoice,
    TemplateInputView, TemplateSourceView, TemplateView,
};
