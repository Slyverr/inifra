# Template packs

Inifra loads bundled templates and templates from the user's configuration
directory. When a project path is selected, it also loads packs stored in that
project. Project packs let teams share and version their own conventions with
the repository.

## Built-in templates

The bundled catalog provides optional starters for common repository files:

- `readme` creates a README outline with project name, optional description,
  and optional repository URL inputs.
- `agents` creates concise root instructions for coding agents.
- `contributing` creates contributor guidance and shares the project name input.
- `changelog` creates an Unreleased section using Keep a Changelog's format.
- `editorconfig` sets portable text encoding and newline defaults.
- `docs-architecture` and `docs-development` create outlines for documenting
  verified architecture, setup, and development workflows.
- `github-issues` creates GitHub bug-report and feature-request forms;
  `github-pull-request` creates a pull request description template.
- `security` creates a vulnerability reporting policy and asks for the private
  reporting channel and supported versions.
- `license-mit` and `license-apache` write a `LICENSE` file. Choose one;
  both use the same output path. The MIT template asks for a copyright year and
  holder, while Apache-2.0 copies the standard license text.

Templates remain opt-in so projects only receive files that fit their needs.

## Location

Create one directory per pack under:

```text
<user config directory>/inifra/templates/<template-id>/
```

The user config directory follows the operating system convention. For example,
on Linux it is usually `~/.config`; `XDG_CONFIG_HOME` can change it. Set
`INIFRA_CONFIG_DIR` to an absolute path to override the config root on any
platform.

Store project-specific packs and shared input definitions in the repository:

```text
<project>/.inifra/
├── inputs.toml
└── templates/<template-id>/
```

Use `inifra list --path PROJECT` to inspect the effective catalog. `init`,
`generate`, and `validate --path PROJECT` include project packs for their
selected destination; `edit TEMPLATE_ID --path PROJECT` opens or creates a
project pack. Plain `inifra list` remains independent of the current
directory.

Use `inifra edit TEMPLATE_ID` to open an existing local pack in `$VISUAL` or
`$EDITOR`. Editing a built-in first copies its manifest and source files into
this directory, so changes remain local to your user config. In a terminal,
the same command can create a starter pack for an unknown ID.

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
those names directly. Pass values with repeatable `--set KEY=VALUE` options.
The user registry at `<user config directory>/inifra/inputs.toml` and the
optional project registry at `.inifra/inputs.toml` can add or override prompts,
required status, and defaults. Definitions merge in this order: built-in, user,
then project. Required values must be non-empty; optional values without
defaults render as empty strings. Unknown, duplicate, and undeclared `--set`
names are errors.

When a required value is unavailable and both input and error output are
terminals, Inifra prompts once for that shared input. In non-interactive runs,
provide required values with `--set`.

For README inputs and the contributor-guide project name, Inifra detects
metadata from a root `Cargo.toml` or `package.json` in the destination
directory. Explicit `--set` values take precedence over detected values, which
take precedence over configured defaults. Missing required values fail before
any files are written. License templates share the `copyright_year` and
`copyright_holder` inputs; the MIT template requires both.

## Discovery and overrides

`inifra list` shows bundled and user-local templates. Pass `--path PROJECT` to
include that project's packs. A project-local pack replaces a user-local pack
with the same ID, and a user-local pack replaces a bundled pack. Thus the
precedence is project-local, user-local, then built-in. IDs must be unique
within each template collection; the winning source is shown in the listing
and picker preview.

Pack directories may contain regular files and directories. Symlinks and
special files are rejected to keep template reads within the user's pack.
