# Changelog

All notable changes to this project are documented here.

This format follows [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
