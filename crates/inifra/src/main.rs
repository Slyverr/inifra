mod app;
mod assistant;
mod cli;
mod cli_style;
mod commands;
mod completions;
mod config;
mod inputs;
mod output;
mod presets;
mod sources;
mod templates;

pub(crate) use inifra_tui::presentation;

fn main() {
    if let Err(error) = app::run() {
        app::report_error(error);
    }
}
