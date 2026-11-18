# Template catalogs

Inifra discovers packs beneath category directories. Each pack has a globally unique ID given by its directory name and a `template.toml` manifest:

```text
templates/
├── inputs.toml
└── project-files/
    └── readme/
        ├── template.toml
        └── README.md.j2
```

Category and pack IDs use lowercase ASCII letters, digits, and hyphens. The manifest describes the pack and names its shared inputs:

```toml
schema_version = 2
description = "Project overview"
inputs = ["project_name"]

[local_inputs.project_description]
prompt = "Short project description"
required = false
```

Category IDs come from their directory names. An optional catalog-root `categories.toml` maps those IDs to display names:

```toml
schema_version = 1

[categories.project-files]
name = "Project files"
```

Without an entry, Inifra derives a readable label from the category ID. Names must be non-empty and contain no control characters.

The catalog root defines those inputs in `inputs.toml`:

```toml
schema_version = 1

[inputs.project_name]
prompt = "Project name"
required = true
```

Shared input definitions may also have a string `default`. Define them globally in catalog-root `inputs.toml` or for one category in `<category-id>/inputs.toml`. A pack-local definition belongs under `[local_inputs.<name>]` in `template.toml` and is not also listed in `inputs`.

For a shared input name, the nearest definition wins: pack-local, then category, then catalog root. Definitions replace as a whole; their fields are not merged. Pass values with `--set project_name=Example`; omitted optional inputs render as empty strings. Unknown values and missing required values are errors. When selected packs use the same input with equal effective definitions, Inifra resolves it once; conflicting definitions fail before planning or writing.

Every regular file in a pack is an output source except its root `template.toml`. Files ending in `.j2` are rendered with MiniJinja and lose that suffix; other files are copied as bytes. Nested directories and dotfiles are preserved. Symbolic links and unsupported file types are rejected. Output paths must be portable, and conflicting outputs are rejected before writing.

Version control does not preserve intentionally empty directories, so declare those output paths in `directories`:

```toml
schema_version = 2
description = "Workspace structure"
directories = ["code", "drafts", "research", "worktrees"]
```

A pack may contain only directory declarations. Paths must be relative, safe, and unique; nested directory declarations and file/directory overlaps are rejected. If selected packs declare the same directory, Inifra plans it once. `inspect` lists directory outputs separately. Generation reports whether each will be created or already exists, and dry runs never create directories.

Use `inifra validate` to check the bundled catalog or `inifra --catalog PATH validate` to check an on-disk catalog. Catalogs are data: they do not execute hooks or scripts.
