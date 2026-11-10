mod cli;
mod commands;
mod completions;
mod detect;
mod output;
mod presets;
mod templates;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
