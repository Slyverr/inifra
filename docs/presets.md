# Presets

Presets save a reusable template selection and shared input values in the
user's config directory under `inifra/presets/<name>.toml`.

Save, list, and apply a preset:

```sh
inifra preset save rust-cli \
  --description "Rust command-line project" \
  --template readme \
  --template gitignore-rust \
  --set project_name=rust-cli

inifra preset list
inifra init --preset rust-cli --path ./rust-cli
```

An explicit `--set KEY=VALUE` passed to `init` overrides the saved value. If a
required value is absent from both the preset and the project, Inifra prompts
on an interactive terminal. Use `--overwrite` with `preset save` to replace an
existing preset.

Preset files use this versioned TOML format:

```toml
schema_version = 1
name = "rust-cli"
description = "Rust command-line project"
templates = ["readme", "gitignore-rust"]

[values]
project_name = "rust-cli"
```
