# Inifra

Inifra is a Rust CLI for adding reusable starter files to a project. It lists
bundled and user-local templates and generates their files in a chosen project
directory, preserving existing files unless replacement is explicitly
requested.

Human-readable output uses terminal colors when supported. Set `NO_COLOR` to
disable styling.

## Requirements

- Rust 1.88 or newer, or Nix with flakes enabled

## Build and run

```sh
cargo check --locked
cargo run --locked -- --help
```

With Nix, run a command in the development shell without entering it first:

```sh
nix develop -c cargo check --locked
```

Install the current checkout locally with:

```sh
cargo install --path . --locked
```

## Usage

List available templates:

```sh
cargo run --locked -- list
```

Preview a template, then generate it in the current directory:

```sh
cargo run --locked -- generate editorconfig --dry-run
cargo run --locked -- generate editorconfig
```

Set up a project with multiple templates using `init`:

```sh
cargo run --locked -- init --template readme --template editorconfig \
  --path ./my-project \
  --set project_name=MyProject
```

Run `inifra init` without `--template` or `--preset` in a terminal to choose
templates interactively. The picker groups templates by category, previews the
focused template, and lets you review rendered files before confirming writes.

Templates that need values accept repeatable `--set KEY=VALUE` options, for
example `cargo run --locked -- generate readme --set project_name=Inifra`.
Pass `--edit` to open generated text files in `$VISUAL` or `$EDITOR` before
writing them. Files are edited in a temporary directory; binary files and
existing files that would be skipped are left out.

Use `--path DIR` to choose a project directory and `--overwrite` to replace
existing files. The default is to leave existing files unchanged.

Inifra also discovers user-local template packs from the platform config
directory under `inifra/templates`. See [Template packs](docs/templates.md)
for the manifest format and override behavior. See [Presets](docs/presets.md)
to save and reuse template selections.

Run `inifra edit TEMPLATE_ID` to open a local template pack in `$VISUAL` or
`$EDITOR`. A built-in template is copied to the user config directory before
it is opened; an unknown ID can start a new template pack from an interactive
terminal.

Generate dynamic completions for Bash, Elvish, Fish, PowerShell, or Zsh with
`inifra completions SHELL`. The generated completion scripts use the installed
`inifra` command to complete available commands, options, paths, template IDs,
preset names, and shared input names. You can write the script to Inifra's
managed config location with `inifra completions SHELL --install` (the older
`--apply` spelling remains available). This never edits shell startup files.
Fish loads its managed file automatically. For other shells, source the
reported file from that shell's completion setup. Re-run `--install` after
upgrading Inifra to refresh the shell integration script.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
