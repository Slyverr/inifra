# Inifra

Inifra is a Rust CLI for adding reusable starter files to a project. It lists
bundled and user-local templates and generates their files in a chosen project
directory, preserving existing files unless replacement is explicitly
requested.

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
cargo run --locked -- generate gitignore-rust --dry-run
cargo run --locked -- generate gitignore-rust
```

Set up a project with multiple templates using `init`:

```sh
cargo run --locked -- init --template readme --template editorconfig \
  --path ./my-project \
  --set project_name=MyProject
```

Run `inifra init` without `--template` or `--preset` in a terminal to choose
templates interactively, preview their rendered files, and confirm the write.

Templates that need values accept repeatable `--set KEY=VALUE` options, for
example `cargo run --locked -- generate readme --set project_name=Inifra`.

Use `--path DIR` to choose a project directory and `--overwrite` to replace
existing files. The default is to leave existing files unchanged.

Inifra also discovers user-local template packs from the platform config
directory under `inifra/templates`. See [Template packs](docs/templates.md)
for the manifest format and override behavior. See [Presets](docs/presets.md)
to save and reuse template selections.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
