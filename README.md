# Inifra

Inifra is a Rust command-line tool for bootstrapping projects with reusable, customizable file templates. It helps developers keep supporting project files and conventions consistent without recreating them by hand.

## Installation

To install Inifra from a source checkout, use Rust 1.100.0 or newer and Cargo:

```sh
cargo install --path crates/inifra --locked
```

## Quick Start

List bundled templates, inspect one, and preview its output:

```sh
inifra list
inifra inspect readme
inifra generate readme --set project_name=Example --dry-run
```

Remove `--dry-run` to write the files. Existing files are skipped by default; use `--overwrite` to replace them. Pass `--target PATH` to choose a destination, and use `inifra --help` for the available commands. An alternative on-disk catalog can be selected with `--catalog PATH`.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, checks, test organization, and contribution guidance.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.
