//! Provider-neutral template, project, and generation services for Inifra.
#![deny(missing_docs)]

pub mod apply;
pub mod catalog;
pub mod inputs;
pub mod output_rules;
pub mod plan;
pub mod presets;
pub mod render;
mod safe_fs;
pub mod sources;
pub mod user_templates;
pub mod workflow;
