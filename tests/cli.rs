use std::process::{Command, Output};

use tempfile::tempdir;

#[path = "cli/basics.rs"]
mod basics;
#[path = "cli/catalog.rs"]
mod catalog;
#[path = "cli/completions.rs"]
mod completions;
#[path = "cli/edit.rs"]
mod edit;
#[path = "cli/generation.rs"]
mod generation;
#[path = "cli/init.rs"]
mod init;
#[path = "cli/presets.rs"]
mod presets;
#[path = "cli/project_templates.rs"]
mod project_templates;
#[path = "cli/remove.rs"]
mod remove;
#[path = "cli/rendering.rs"]
mod rendering;

fn run_cli(arguments: &[&str]) -> Output {
    let config = tempdir().expect("create isolated config directory");
    run_cli_with_config(arguments, config.path())
}

fn run_cli_in(arguments: &[&str], directory: &std::path::Path) -> Output {
    let config = tempdir().expect("create isolated config directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(arguments).current_dir(directory);
    set_config_directory(&mut command, config.path());
    command.output().expect("run Inifra")
}

fn run_cli_with_config(arguments: &[&str], config: &std::path::Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command.args(arguments);
    set_config_directory(&mut command, config);
    command.output().expect("run Inifra")
}

fn run_cli_with_forced_colors(arguments: &[&str], no_color: bool) -> Output {
    let config = tempdir().expect("create isolated config directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args(arguments)
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR");
    if no_color {
        command.env("NO_COLOR", "1");
    }
    set_config_directory(&mut command, config.path());
    command.output().expect("run Inifra")
}

fn set_config_directory(command: &mut Command, config: &std::path::Path) {
    command.env("INIFRA_CONFIG_DIR", config);
}

fn write_local_pack(
    config: &std::path::Path,
    id: &str,
    description: &str,
    output: &str,
    body: &str,
) {
    let pack = templates_root(config).join(id);
    std::fs::create_dir_all(&pack).expect("create local template pack");
    std::fs::write(
        pack.join("template.toml"),
        format!(
            "schema_version = 1\nid = {id:?}\ndescription = {description:?}\n\n[[files]]\nsource = \"source.txt\"\noutput = {output:?}\n"
        ),
    )
    .expect("write manifest");
    std::fs::write(pack.join("source.txt"), body).expect("write template source");
}

fn templates_root(config: &std::path::Path) -> std::path::PathBuf {
    config.join("inifra/templates")
}

fn write_project_pack(
    project: &std::path::Path,
    id: &str,
    description: &str,
    output: &str,
    body: &str,
) {
    let pack = project.join(".inifra/templates").join(id);
    std::fs::create_dir_all(&pack).expect("create project template pack");
    std::fs::write(
        pack.join("template.toml"),
        format!(
            "schema_version = 1\nid = {id:?}\ndescription = {description:?}\n\n[[files]]\nsource = \"source.txt\"\noutput = {output:?}\n"
        ),
    )
    .expect("write project template manifest");
    std::fs::write(pack.join("source.txt"), body).expect("write project template source");
}

fn write_project_inputs(project: &std::path::Path, name: &str, default: &str) {
    let config = project.join(".inifra");
    std::fs::create_dir_all(&config).expect("create project Inifra config");
    std::fs::write(
        config.join("inputs.toml"),
        format!(
            "schema_version = 1\n\n[inputs.{name}]\nprompt = {name:?}\ndefault = {default:?}\n"
        ),
    )
    .expect("write project input definitions");
}
