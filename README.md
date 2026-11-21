# Inifra

Inifra is a Rust command-line tool for bootstrapping projects with reusable, customizable file templates. It helps developers keep supporting project files and conventions consistent without recreating them by hand.

## Showcase

https://github.com/user-attachments/assets/229e62c2-5b1f-4e30-82f0-6faa53d9cd60

## Installation

Install the latest published version from crates.io with Cargo:

```sh
cargo install inifra --locked
```

To install from a source checkout, use Rust 1.100.0 or newer and Cargo:

```sh
cargo install --path crates/inifra --locked
```

Download prebuilt binaries from [GitHub Releases](https://github.com/Slyverr/inifra/releases). On Nix, install the package from the flake with `nix profile install github:Slyverr/inifra`. See [Nix support](docs/nix.md) for the development shell and local build commands.

## Quick Start

Choose templates interactively, or list and inspect the bundled catalog:

```sh
inifra init
inifra list
inifra inspect readme
inifra generate readme --set project_name=Example --target preview-output --dry-run --show-content
```

`inifra init` keeps template selection, input collection, and file review in one inline terminal flow. Press `/` in the picker to filter choices; `a` toggles matching entries in the current category; `q` quits outside text entry. Use `inifra generate` for scripted, non-interactive use. Add `--show-content` to `generate --dry-run` to see new file contents and diffs for replacements without writing anything; existing files that would be skipped are omitted from the content preview. Existing files are skipped by default; use `--overwrite` to replace them after review. Pass `--target PATH` to choose a destination. Use `inifra list --files` to expand every output path, and `inifra --help` for the available commands. An alternative on-disk catalog can be selected with `--catalog PATH`.

See [the template catalog guide](docs/templates.md) for pack layout and input definitions.
Personal template packs can extend bundled packs, and repositories can keep team packs in `.inifra/templates/` or pin shared Git catalogs with `inifra source`. Set `INIFRA_CONFIG_DIR` to choose a different personal config root; the [catalog guide](docs/templates.md) documents precedence, source pins, and cache behavior.
Use `inifra edit`, `rename`, and `remove` to manage personal packs; `inifra preset` saves and reapplies selections. See [presets](docs/presets.md) for details.
Use `inifra assistant` to configure assistant profiles and credentials, then `inifra ask` to inspect templates and prepare reviewed generation plans in an inline assistant session. See [assistant setup and data handling](docs/assistant.md).

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, checks, test organization, and contribution guidance.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.
