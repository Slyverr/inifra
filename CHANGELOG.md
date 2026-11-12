# Changelog

All notable changes to Inifra are documented here.

The format follows [Keep a Changelog 2.0.0](https://keepachangelog.com/en/2.0.0/),
and this project uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Remove user-local or project-local template packs without deleting generated
  files.

## [0.6.2] - 2026-11-12

### Fixed

- Apply the same safe template-path rules on every operating system.

## [0.6.1] - 2026-11-12

### Fixed

- Reject control characters in template descriptions, categories, and file paths.

## [0.6.0] - 2026-11-12

### Added

- Filter templates by ID, description, category, or output path in the picker
  and with `inifra list --filter`.

## [0.5.0] - 2026-11-12

### Added

- Discover project-local packs and shared input definitions under `.inifra/` for a selected project path.
- Add pinned Git template sources with explicit syncing and offline generation.
- Report added, updated, and removed templates when updating a cached Git source.

## [0.4.0] - 2026-11-12

### Added

- Allow `INIFRA_CONFIG_DIR` to override the platform's user config directory.
- Add a contributor guide starter and MIT and Apache-2.0 license templates.
- Add agent, architecture, development, GitHub collaboration, and security policy starters.
- Install dynamic completions for supported shells without editing startup files.
- Add `inifra validate [TEMPLATE_ID]` to check bundled and user-local templates without writing files.

### Removed

- Remove the Rust `.gitignore` starter, which common project bootstrap tools already provide.

### Changed

- Add a category-grouped inline template picker with a live source preview,
  adaptive layout, category navigation, and alternatives for shared output paths.
- Make the built-in changelog and EditorConfig templates more adaptable to
  projects with different release and formatting conventions.
- Expand the README starter with common setup, usage, and support sections.

## [0.3.0] - 2026-11-10

### Added

- Colorize CLI help, output, and errors while respecting terminal color settings.
- Add an interactive template picker with rendered-file previews and write confirmation.
- Generate shell completions for Bash, Elvish, Fish, PowerShell, and Zsh; complete
  template IDs, preset names, and shared input names dynamically.
- Edit generated text files and user-local template packs with the configured editor.
- Verify builds and CLI behavior across platforms, and add an installable Nix package.

## [0.2.0] - 2026-11-10

### Added

- Discover user-local template packs and render strict `.j2` template sources.
- Resolve shared template inputs, detect Cargo and Node project metadata, and prompt for required values.
- Add a README starter template backed by shared project metadata.
- Add non-interactive initialization and save, list, and apply user presets.

## [0.1.0] - 2026-11-10

### Added

- Add bundled starter templates with listing, dry-run previews, and safe file generation.
