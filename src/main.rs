mod cli;
mod commands;
mod completions;
mod detect;
mod output;
mod paths;
mod presets;
mod templates;
mod theme;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!(
            "{} {error:#}",
            theme::paint_stderr("error:", theme::Tone::Error, true)
        );
        std::process::exit(1);
    }
}
