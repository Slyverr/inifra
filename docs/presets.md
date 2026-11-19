# Presets

A preset saves a reusable template selection and input values. It references template IDs; it does not contain template source files or duplicate their input definitions.

## Reference examples

The examples in [`../presets/`](../presets/) define the intended manifest shape. They are fixtures for design and tests only: the application does not load them as built-in presets or embed them in its runtime catalog.

Each preset is stored as a directory named by its ID:

```text
presets/
├── documentation/
│   └── preset.toml
└── rust-cli/
    └── preset.toml
```

User-created presets belong in the platform's Inifra user-config directory:

```text
<user-config>/inifra/presets/<preset-id>/preset.toml
```

`INIFRA_CONFIG_DIR` can override the platform config root; see [personal catalogs](templates.md#personal-catalogs).

The directory name is authoritative; the manifest does not repeat an `id` field. There is no bundled preset catalog. Repository-shared presets are outside this format's current scope and need an explicit `.inifra/` design.

## Manifest format

```toml
schema_version = 1
description = "Core files for a small Rust command-line project"
templates = ["agents", "changelog", "contributing", "editorconfig", "license-mit", "readme"]

[values]
project_name = "example-cli"
project_description = "A small command-line application."
```

- `schema_version` is required and unsupported versions are rejected.
- `description` is a short label for preset listings.
- `templates` is a non-empty list of unique template IDs from the effective catalog.
- `[values]` stores user-provided strings keyed by input name. Prompt definitions stay with the template catalog.

Preset values use the normal input resolver. Explicit values supplied for a run override saved values. Missing required inputs follow the normal prompt or non-interactive error behavior. Applying a preset still reviews the planned outputs and requires the same approval as direct template selection.

Presets do not store a destination directory, overwrite approval, provider credentials, or executable actions. Do not store secrets in preset values.

Preset IDs, template references, and saved input names are validated when a preset is loaded for use. Renaming a user-owned template must keep affected preset references valid; preset updates must not be left partially written if a rename fails.

## Managing presets

Use `inifra preset save <id> --description "..." --template <template-id>` to save a selection. Repeat `--template` for additional packs and `--set name=value` for values to reuse. `inifra preset list` and `inifra preset inspect <id>` show saved data. Apply a preset with `inifra preset apply <id>`; it opens the same selection and review flow as `inifra init`, and `--set` values on the command override saved values.

Rename and remove presets with `inifra preset rename <old-id> <new-id>` and `inifra preset remove <id>`. If a manifest is malformed, `preset save --overwrite` can replace it and `preset remove` can remove it, provided that the preset directory contains only `preset.toml`. Unexpected files are preserved and block both operations.

The examples define the user-owned format, not shipped defaults. A later project-local preset format, if needed for team sharing, belongs in the separate project-configuration design rather than being inferred from this user-owned format.
