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

[[files]]
source = "NOTES.md"
output = "docs/NOTES.md"
```

`id` uses lowercase letters, digits, and hyphens. `description` must not be
empty. `category` is optional grouping metadata. Add one `[[files]]` entry for
each output. `source` is relative to the pack directory; `output` is relative
to the destination project directory. Both paths must be safe relative paths.
Outputs may not overlap or conflict.

For now, source files are copied as-is. Variable rendering and shared inputs
will be introduced separately.

## Discovery and overrides

`inifra list` shows both bundled and local templates, including category labels
when provided. Local template IDs must be unique. A local template with the
same ID as a bundled template replaces that bundled entry, so
`inifra generate <id>` uses the local manifest and files.

Pack directories may contain regular files and directories. Symlinks and
special files are rejected to keep template reads within the user's pack.
