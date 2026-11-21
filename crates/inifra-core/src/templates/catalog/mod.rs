//! Discover, load, and validate categorized template packs.

mod loader;
mod model;
mod overlay;
mod resolve;

pub use model::{Catalog, Category, Pack, PackReference};

use include_dir::{Dir, include_dir};
use std::{error::Error, fmt};

static BUNDLED: Dir<'_> = include_dir!("$OUT_DIR/bundled_templates");

/// Extracts the compile-time bundled catalog into an existing private directory.
///
/// # Errors
/// Returns an error when catalog files cannot be written.
pub fn extract_bundled(destination: &std::path::Path) -> Result<(), std::io::Error> {
    BUNDLED.extract(destination)
}

/// A catalog discovery or validation error.
#[derive(Debug)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for CatalogError {}

pub(crate) fn valid_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn valid_template_id(id: &str) -> bool {
    match id.split_once('/') {
        Some((namespace, local_id)) => {
            valid_identifier(namespace) && valid_identifier(local_id) && !local_id.contains('/')
        }
        None => valid_identifier(id),
    }
}
