# Nix

The flake provides a development shell and an installable package for Linux and macOS. It pins the Nix inputs in `flake.lock` and uses Rust 1.100.0 for both development and package builds.

## Development

Enter the project environment from the repository root:

```sh
nix develop
```

The shell includes the pinned Rust toolchain, Git, and OpenSSH. Run the normal project checks inside it:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Build and install

Build the package from a checkout and run it directly:

```sh
nix build .#inifra
nix run .#inifra -- --help
```

Install the package from the repository's default branch:

```sh
nix profile install github:Slyverr/inifra
```

The package wraps the executable with Git and OpenSSH so Git catalog sources, including SSH sources, work when launched from the Nix profile.

## Supported systems

The flake defines package and shell outputs for `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, and `aarch64-darwin`. CI builds and smoke-tests the package on its Linux and macOS runners.
