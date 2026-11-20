# Changelog

All notable changes to this project are documented here.

This format follows [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Filter templates live in the interactive picker with `/` while preserving the `a` group toggle.
- Quit the interactive initialization flow with `q` when the interface is not accepting text.
- Preview rendered text during a read-only `inifra generate --dry-run --show-content`.
- Add `--overwrite` and `--force` options for explicitly replacing or removing unmarked shell completion files.

### Changed

- Use a subdued blue-gray for inactive template choices in the interactive picker.
- Bring the initialization picker layout, preview hierarchy, and Markdown/Jinja color cues in line with the previous interface.
- Keep templates with coordinated output paths independently selectable and preview their resolved destinations.
- Disable incompatible template choices before selection and restore keybinding hints after transient validation errors.
- Distinguish template input names from their prompts in the preview pane.
- Group common path prefixes in `inifra list --files` and show paths directly under each template.
- Separate dry-run action summaries from content, omitting skipped files and diffing replacements.

## [0.6.0] - 2026-11-19

### Added

- Open an inline assistant session with `inifra ask`, slash-command catalog and project inspection, model listing and session switching, and explicit review before applying a validated generation plan.
- Stream assistant responses through OpenAI Responses, OpenAI-compatible Chat Completions, Anthropic Messages, Gemini Generate Content, and Ollama Chat APIs.
- Render streamed Markdown emphasis, inline code, links, headings, lists, quotes, and code blocks in the terminal view.

## [0.5.0] - 2026-11-19

### Added

- Load team template packs from `.inifra/templates/`, after bundled and personal catalog layers.
- Pin shared Git template catalogs to project-specific revisions and manage them with `inifra source`.
- Cache pinned catalogs for offline listing, inspection, validation, generation, and interactive initialization.
- Namespace source catalogs and support references between bundled, personal, Git, and project-local packs.

### Security

- Reject option-like SSH hosts in configured Git source URLs.

## [0.4.0] - 2026-11-19

### Added

- Load personal template packs from the platform config directory, overlaying bundled packs by template ID.
- Edit bundled templates through user-owned copies, and create, rename, or remove personal packs with reference checks.
- Save, inspect, apply, rename, and remove reusable template-selection presets.
- Create a valid starter source file when interactively scaffolding a new personal template.
- Generate and install shell completions for Bash, Elvish, Fish, PowerShell, and Zsh, including dynamic template, preset, and input suggestions.

### Changed

- Apply the established terminal palette and compact status formatting to CLI output.
- Keep preset recovery available when a saved manifest is malformed, while preserving unexpected files.

## [0.3.0] - 2026-11-19

### Added

- Add an inline interactive initialization flow for selecting templates, entering values, and reviewing planned files before approval.

## [0.2.0] - 2026-11-19

### Added

- Configure category labels and catalog/category shared inputs; define pack-local inputs in each pack manifest.
- Declare empty output directories, coordinate shared output paths, and reference other selected packs' outputs.
- List every template output with `inifra list --files`.
- Bundle 12 starter packs for project files, project documentation, GitHub, and project policies.

### Changed

- MIT and Apache license packs use distinct filenames when selected together while retaining `LICENSE` when used alone.

## [0.1.0] - 2026-11-18

### Added

- Discover and inspect bundled template packs, and validate catalog definitions.
- Generate project files with explicit input values and a dry-run preview. Existing files are preserved unless replacement is requested.

### Fixed

- Replacing a file no longer changes other hard links to its original contents.
- Reject symbolic links for shared input definitions.
- Reject Windows-reserved superscript device names in output paths.
