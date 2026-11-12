# Inifra

Inifra is a Rust command-line tool for adding reusable project files. Browse
built-in or personal templates, preview their contents, and generate selected
files while keeping existing files unchanged by default.

## Install

Install the latest published release with Cargo. This requires Rust 1.88 or
newer and Git:

```sh
cargo install --git https://github.com/Slyverr/inifra --tag v0.5.0 --locked
```

## Get started

Use Rust 1.88 or newer, or run commands inside the Nix development shell.

```sh
cargo run --locked -- list
cargo run --locked -- validate
cargo run --locked -- init --path ./my-project
```

`init` opens the interactive template picker in a terminal. Use
`cargo run --locked -- --help` to see all commands and options. `validate`
checks bundled and personal templates without writing project files.
In the picker, press `/` to filter by template ID, description, category, or
output path. Press `Esc` to leave filter editing; press it again to clear the
filter.

## Templates and configuration

Templates can generate one or more files and can be customized in your user
config or shared with a project under `.inifra/`. See
[template packs](docs/templates.md) for discovery and precedence rules, and
[presets](docs/presets.md) for saving reusable selections.

Projects can also pin shared template packs from a Git repository. Run
`inifra source add NAME URL --revision REF` to add and fetch a source, then use
its templates as `NAME/TEMPLATE_ID`. Git is required for adding or syncing
sources; generation uses the cached pinned revision offline.

Inifra also supports dynamic completions for Bash, Elvish, Fish, PowerShell,
and Zsh. Run `inifra completions SHELL --install` to install them without
changing shell startup files.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
