use anyhow::{Context, Result};
use clap_complete::{Shell, generate};

use crate::cli;

pub(super) fn run(shell: Shell) -> Result<()> {
    let mut command = cli::command();
    generate(shell, &mut command, "inifra", &mut std::io::stdout());
    std::io::Write::flush(&mut std::io::stdout()).context("could not write completion script")
}
