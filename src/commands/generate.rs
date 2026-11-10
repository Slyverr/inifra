use std::path::Path;

use anyhow::Result;

use super::setup;

pub(super) fn run(
    template_id: &str,
    destination: &Path,
    dry_run: bool,
    overwrite: bool,
    assignments: &[String],
) -> Result<()> {
    setup::run(
        &[template_id.to_owned()],
        destination,
        dry_run,
        overwrite,
        assignments,
    )
}
