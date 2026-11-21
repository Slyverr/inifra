//! Provider-neutral template, project, and generation services for Inifra.
#![deny(missing_docs)]

mod generation;
mod storage;
mod templates;

pub use generation::{apply, output_rules, plan, render, workflow};
pub use storage::{presets, source_registry};
pub use templates::{catalog, inputs, sources, user_templates};

pub(crate) use storage::safe_fs;
