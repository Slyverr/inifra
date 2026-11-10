# Template packs

Inifra loads templates bundled with the program and templates in the user's
configuration directory. Local packs let users add templates or replace a
built-in template without changing the project or the Inifra installation.

## Location

Create one directory per pack under:

```text
<user config directory>/inifra/templates/<template-id>/
```

The user config directory follows the operating system convention. For example,
on Linux it is usually `~/.config`; `XDG_CONFIG_HOME` can change it.

Each pack contains a `template.toml` manifest and its source files:

```text
notes/
├── template.toml
└── NOTES.md
```

## Manifest format

```toml
schema_version = 1
id = "notes"
description = "Project notes"
category = "Project guidance"
inputs = ["project_name"]

[[files]]
source = "NOTES.md"
output = "docs/NOTES.md"
```

`id` uses lowercase letters, digits, and hyphens. `description` must not be
empty. `category` is optional grouping metadata. Add one `[[files]]` entry for
each output. `source` is relative to the pack directory; `output` is relative
to the destination project directory. Both paths must be safe relative paths.
Outputs may not overlap or conflict.

Sources ending in `.j2` are rendered as MiniJinja text before writing. Undefined
values and invalid template syntax are errors, and the source's final newline
is preserved. Other source files are copied byte-for-byte. The manifest's
`output` is always the destination path, so a source named `README.md.j2` can
write to `README.md`.

Shared input prompts and defaults are defined in `templates/inputs.toml`. Each
pack lists the input names it uses in its manifest, and template text refers to
those names directly. Pass values with repeatable `--set KEY=VALUE` options. A
local registry at `<user config directory>/inifra/inputs.toml` can add or
override prompts, required status, and defaults. Required values must be
non-empty; optional values without defaults render as empty strings. Unknown,
duplicate, and undeclared `--set` names are errors.

When a required value is unavailable and both input and error output are
terminals, Inifra prompts once for that shared input. In non-interactive runs,
provide required values with `--set`.

For README inputs, Inifra detects project name, description, and repository URL
from a root `Cargo.toml` or `package.json` in the destination directory. Explicit
`--set` values take precedence over detected values, which take precedence over
configured defaults. Missing required values fail before any files are written.

## Discovery and overrides

`inifra list` shows both bundled and local templates, including category labels
when provided. Local template IDs must be unique. A local template with the
same ID as a bundled template replaces that bundled entry, so
`inifra generate <id>` uses the local manifest and files.

Pack directories may contain regular files and directories. Symlinks and
special files are rejected to keep template reads within the user's pack.
