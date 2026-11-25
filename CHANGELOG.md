# Changelog

All notable changes to this project are documented here.

This format follows [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Choose a known provider or a custom endpoint during assistant setup, then pick the provider or API protocol from a short list.
- Switch saved profiles in `inifra ask` with `/profile`.
- Switch profile, model, and reasoning effort in `inifra ask` with the switcher, opened by Ctrl+O or `/model` without an ID, and save the selection as the default.
- Set reasoning effort with `/effort` and a per-profile default effort. Profiles saved with an effort cannot be read by 0.8.0 or earlier.
- Start a new conversation in `inifra ask` with `/clear`.
- Explain failed assistant requests with a summary, a suggested command, and details.

### Changed

- Show saved profiles on the `inifra assistant` home screen and separate creating a profile from editing one.
- Ask for a profile name in the profile form, validate it inline, and treat `Work` and `work` as the same name by saving it in lowercase.
- Start `inifra ask` replies with a `●` bullet and indent the rest, tint your messages, and show an animated "Thinking" indicator while the assistant works.
- Show slash-command output in `inifra ask` as a styled card instead of plain text, and grow the chat viewport downward while a reply streams.
- Show the "Type a message" placeholder in a muted color.
- Edit text in the profile form and the chat composer with one readline-style keymap, including Ctrl+W, Ctrl+←/→, Alt+B/F, Alt+Backspace, Ctrl+K, and Ctrl+U.
- Keep every profile form field in view: finished fields show a check and their value, the active field edits in place, and upcoming fields wait in gray.
- Ask for the API key in its own step, with pasting as the default, instead of asking for an environment variable name first.
- Drop leading and trailing spaces from submitted form values.
- Choose the default model from the provider's own list during setup, after the key is entered, with `/` to filter and a typed model ID as the fallback when the list is unavailable. Setup now makes one network request to fetch that list.
- Filter long choice lists with `/`.
- Lead the chat footer with the active profile.
- Label failed responses "request failed" and reserve "interrupted" for a user cancel.
- Return from input and review to the picker with Esc in `inifra init` instead of cancelling.
- Mark the focused row with `›` on inline screens and share one footer style.

### Fixed

- Enable the I/O driver in the assistant runtime so requests can reach the network.
- Report the cause of connection failures, such as a refused connection.

## [0.8.0] - 2026-11-21

### Added

- Provide a Nix flake with a pinned development shell and installable package.
- Build platform binaries and publish release archives on GitHub Releases after tag CI passes.
- Prepare the workspace crates for crates.io distribution, with a manual, CI-gated publishing workflow.

## [0.7.0] - 2026-11-20

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
