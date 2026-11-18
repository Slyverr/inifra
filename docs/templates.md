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
```

The catalog root defines those inputs in `inputs.toml`:

```toml
schema_version = 1

[inputs.project_name]
prompt = "Project name"
required = true
```

Input definitions may also have a string `default`. Pass values with `--set project_name=Example`; omitted optional inputs render as empty strings. Unknown values and missing required values are errors.

Every regular file in a pack is an output source except its root `template.toml`. Files ending in `.j2` are rendered with MiniJinja and lose that suffix; other files are copied as bytes. Nested directories and dotfiles are preserved. Symbolic links and unsupported file types are rejected. Output paths must be portable, and conflicting outputs are rejected before writing.

Use `inifra validate` to check the bundled catalog or `inifra --catalog PATH validate` to check an on-disk catalog. Catalogs are data: they do not execute hooks or scripts.
