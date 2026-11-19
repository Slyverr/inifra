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

## Bundled packs

The bundled catalog groups its packs into four categories:

| Category | Packs |
| --- | --- |
| GitHub | `github-issues`, `github-pull-request` |
| Project docs | `architecture`, `development` |
| Project files | `agents`, `changelog`, `contributing`, `editorconfig`, `readme` |
| Project policies | `license-apache`, `license-mit`, `security` |

The catalog root defines those inputs in `inputs.toml`:

```toml
schema_version = 1

[inputs.project_name]
prompt = "Project name"
required = true
```

Shared input definitions may also have a string `default`. Define them globally in catalog-root `inputs.toml` or for one category in `<category-id>/inputs.toml`. A pack-local definition belongs under `[local_inputs.<name>]` in `template.toml` and is not also listed in `inputs`.

For a shared input name, the nearest definition wins: pack-local, then category, then catalog root. Definitions replace as a whole; their fields are not merged. Pass values with `--set project_name=Example`; omitted optional inputs render as empty strings. Unknown values and missing required values are errors. When selected packs use the same input with equal effective definitions, Inifra resolves it once; conflicting definitions fail before planning or writing.

## Personal catalogs

The default catalog combines the read-only bundled packs with packs under the user's Inifra config directory:

```text
<config-dir>/inifra/templates/<category-id>/<template-id>/
```

`<config-dir>` is the platform's standard configuration directory. Set `INIFRA_CONFIG_DIR` to choose a different Inifra config root; its `templates/` and `presets/` directories are read directly.

User packs with new IDs extend the bundled catalog. A user pack with an existing ID replaces that bundled pack. User root inputs and category inputs replace matching bundled definitions; new definitions extend them. A category display name replaces the bundled label only when explicitly set in the user's `categories.toml`. The combined catalog is validated before commands use it, so malformed user data produces an error instead of being silently ignored.

Passing `--catalog PATH` selects only that catalog and bypasses the bundled and user catalogs. This keeps explicit catalogs deterministic for scripts and tests.

### Moving personal packs from older versions

The current user catalog discovers packs at `<config-dir>/inifra/templates/<category-id>/<template-id>/`. Earlier versions placed packs directly under `templates/<template-id>/`; the current catalog does not discover or convert that layout. Back up the Inifra config directory before upgrading, then move each pack under a category directory.

For each moved pack, use `schema_version = 2`, remove the old `category` field from `template.toml`, and put any category label in `categories.toml`. Move shared inputs from the old config-root `inputs.toml` to `templates/inputs.toml`; category-specific inputs belong in `<category-id>/inputs.toml`. If a manifest uses legacy `[[files]]` source/output mappings, put each output source at its destination-relative path inside the pack. Files ending in `.j2` are rendered with that suffix removed; other regular files are copied.

Run `inifra validate` after moving the packs, then use `inifra inspect <id>` to check their outputs. Migration is manual; Inifra leaves existing user files untouched.

## Managing personal packs

Use `inifra edit <id>` to open a personal pack in `VISUAL` or `EDITOR`. A bundled pack is copied into the matching user category before editing, so the embedded copy stays unchanged. For a new ID, Inifra asks before creating it, then collects a category, description, and first source-file path; the default is `<id>.md.j2`. Editor arguments are parsed without invoking a shell.

Use `inifra rename <old-id> <new-id>` to rename a personal pack and update references from other personal packs and saved presets. `inifra remove <id>` refuses removal while a preset or another pack refers to it. Bundled packs must be copied through `edit` before they can be renamed or removed.

The top-level input name `references` is reserved for optional paths to other packs' outputs.

Every regular file in a pack is an output source except its root `template.toml`. Files ending in `.j2` are rendered with MiniJinja and lose that suffix; other files are copied as bytes. Nested directories and dotfiles are preserved. Symbolic links and unsupported file types are rejected. Output paths must be portable, and conflicting outputs are rejected before writing.

## Output paths and coordinated alternatives

Ordinary outputs use their source paths with only the final `.j2` suffix removed. Use `output_rules` only when a source needs a different destination or when packs intentionally share a destination on their own:

```toml
[[output_rules]]
source = "LICENSE.j2"
co_selected_output = { group = "licenses", path = "LICENSE-MIT" }
```

The rule above leaves the pack's output as `LICENSE` when used alone. If another selected pack also resolves to `LICENSE` and declares the same `licenses` group, both packs use their declared alternative paths. Every pack in that collision must declare the same group; otherwise generation reports the collision before writing. Alternative destinations are validated together too, so alternatives that still conflict are rejected.

An explicit default destination can be declared with `output = "docs/TERMS.md"`; it can be combined with `co_selected_output` when that default may collide. `inspect` shows the output used alone and any declared paired destination.

The bundled MIT and Apache license packs use this rule: each creates `LICENSE` alone, and selecting both creates `LICENSE-MIT` and `LICENSE-APACHE`.

In the inline `inifra init` picker, templates with the same default output and the same alternative group appear as one choice. Use left/right or `h`/`l` to switch variants; only the active variant is selected. The non-interactive `generate` command can still select both IDs explicitly, in which case the declared alternate paths are used.

## Optional pack references

A pack may name a source from another pack as an optional reference:

```toml
[references.contributing]
template = "contributing"
source = "CONTRIBUTING.md.j2"
```

The reference name is available as `references.contributing`. Gate its use in the source template so it is omitted when the target pack is not selected:

```jinja
{% if references.contributing %}
See [CONTRIBUTING.md]({{ references.contributing }}) for contribution guidance.
{% endif %}
```

References never prompt for or auto-select another pack. The target pack and source must exist, and self-references are rejected. When the target is selected, the value is its resolved output path, including `.j2` suffix removal and any output rule. Otherwise, the value is an empty string. Reference paths use `/` separators on every platform.

## Listing outputs

`inifra list` shows each pack's first file output followed by `+N other files` when there are more, plus a directory count when applicable. Use `inifra list --files` to expand every resolved file and declared directory path. `inspect` shows the complete outputs for one pack, including any co-selected destination.

Version control does not preserve intentionally empty directories, so declare those output paths in `directories`:

```toml
schema_version = 2
description = "Workspace structure"
directories = ["code", "drafts", "research", "worktrees"]
```

A pack may contain only directory declarations. Paths must be relative, safe, and unique; nested directory declarations and file/directory overlaps are rejected. If selected packs declare the same directory, Inifra plans it once. `inspect` lists directory outputs separately. Generation reports whether each will be created or already exists, and dry runs never create directories.

Use `inifra validate` to check the bundled catalog or `inifra --catalog PATH validate` to check an on-disk catalog. Catalogs are data: they do not execute hooks or scripts.
