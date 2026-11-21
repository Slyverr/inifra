# Architecture

Inifra is a Cargo workspace with a small set of crates separated by responsibility. The command-line package remains the executable users install; the other crates isolate reusable behavior, assistant integrations, and terminal presentation.

## Crates

| Crate | Owns | Must not own |
| --- | --- | --- |
| `inifra` | Command parsing, command dispatch, and application wiring. | Template rules, provider protocols, or terminal rendering internals. |
| `inifra-core` | Template/catalog models, project context, input resolution, rendering, generation plans, validation, and file application. | CLI parsing, terminal handling, or network/provider clients. |
| `inifra-assistant` | Assistant profile and credential models, provider protocols, model listing, and streaming request/response transport. | CLI parsing, terminal rendering, project filesystem access, or generation policy. |
| `inifra-tui` | Ratatui screens, terminal lifecycle, input/event handling, and the shared terminal palette and text-safety helpers used by CLI and interactive output. | Template or assistant policy, provider requests, and filesystem changes. |

## Intended dependency direction

```text
inifra (binary)
├── inifra-core
├── inifra-assistant
└── inifra-tui
```

Core stays independent of terminal and network concerns. The CLI coordinates application flows and maps data into TUI view models. The assistant and TUI crates are independently usable and do not depend on core or on each other. For assistant requests that need project data or generation, the CLI supplies bounded results and invokes core workflows; the provider crate itself has no project filesystem access. Core remains responsible for validating and applying file changes.

The terminal crate owns the shared semantic color palette, `NO_COLOR` handling, and escaping of control characters before user-controlled text is displayed. The CLI uses these helpers for redirected-safe colored output, errors, and status lines; interactive screens use the matching Ratatui styles.

For `init`, the CLI supplies catalog view data and a backend adapter to the TUI. The adapter resolves inputs and prepares a core `GenerationPlan`; the TUI retains that exact plan through review and requests its application only after explicit approval. The TUI owns the interaction and terminal lifecycle, while the adapter delegates validation and writes to `inifra-core`.

## Source layout

Each crate groups Rust modules by the work they own. The crate root declares the internal modules and keeps the public API small; files are split where the code has a clear responsibility, not simply to shorten file length.

| Crate | Internal groups |
| --- | --- |
| `inifra` | `cli` defines the Clap command tree and imports argument types from command modules. `app` loads runtime context and dispatches commands. `commands` owns list, inspect, validate, generate, and init. `config`, `templates`, `presets`, and `sources` own their respective CLI workflows. `assistant` and `completions` group those command families. `output` owns shared catalog and generation presentation; `inputs` parses shared `NAME=VALUE` assignments. |
| `inifra-core` | `templates` loads catalogs and project template data. `generation` owns input-aware workflows, output rules, plans, rendering, and application. `storage` owns presets, source registry data, and safe filesystem primitives. Existing crate-root module paths re-export these groups to avoid unnecessary public API churn. |
| `inifra-assistant` | `conversation` exposes chat and model-listing services. `provider` contains protocol adapters. `settings` owns profile and credential configuration. `transport` owns HTTP errors and streaming decoders. |
| `inifra-tui` | `model` defines UI-facing contracts. `init` owns the picker and review flow. `chat` owns the assistant conversation screen and Markdown rendering. `controls` provides reusable forms and selectors. `presentation` and `terminal` centralize terminal styling and lifecycle handling. |

The top-level CLI command enum stays in one place so `--help` and shell completion expose a single tree. Each command's arguments are defined with its handler, and `app` uses an exhaustive typed match to invoke that handler. This avoids a generic registration framework while keeping command behavior out of `main.rs`.

## Keeping the boundary useful

- Keep template formats, sources, presets, and generation policy together in `inifra-core` until an independently useful package boundary or dependency need is demonstrated.
- Keep provider protocols and assistant orchestration in `inifra-assistant`; do not create one crate per provider unless dependency isolation or maintenance needs justify it.
- Keep TUI components and terminal lifecycle in `inifra-tui`; command parsing and workflow decisions remain outside it.
- Do not create separate crates for individual CLI commands or for modules that only share a directory. Split when ownership, dependencies, testing needs, or reuse clearly benefit.
- Put public behavior tests under the corresponding crate's `tests/`; keep private invariant tests close to the implementation when integration tests cannot exercise them cleanly.
