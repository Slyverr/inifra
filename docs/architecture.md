# Architecture

Inifra is a Cargo workspace with a small set of crates separated by responsibility. The command-line package remains the executable users install; the other crates isolate reusable behavior, assistant integrations, and terminal presentation.

## Crates

| Crate | Owns | Must not own |
| --- | --- | --- |
| `inifra` | Command parsing, command dispatch, and application wiring. | Template rules, provider protocols, or terminal rendering internals. |
| `inifra-core` | Template/catalog models, project context, input resolution, rendering, generation plans, validation, and file application. | CLI parsing, terminal handling, or network/provider clients. |
| `inifra-assistant` | Assistant profiles, provider protocols, request/response handling, and constrained assistant workflows using core services. | Terminal rendering or direct file writes outside core's validated generation flow. |
| `inifra-tui` | Ratatui screens, terminal lifecycle, input/event handling, and shared interactive presentation. | Template or assistant policy, provider requests, and filesystem changes. |

## Intended dependency direction

```text
inifra (binary)
├── inifra-core
├── inifra-assistant ──> inifra-core
└── inifra-tui
```

Core stays independent of terminal and network concerns. The CLI coordinates application flows and maps data into TUI view models; the TUI does not depend on the assistant crate or core. The assistant may prepare or inspect core operations, while core remains responsible for validating and applying file changes. The packages are scaffolded before feature code, so Cargo dependencies will be added as implementations first need them instead of creating unused dependency edges.

## Keeping the boundary useful

- Keep template formats, sources, presets, and generation policy together in `inifra-core` until an independently useful package boundary or dependency need is demonstrated.
- Keep provider protocols and assistant orchestration in `inifra-assistant`; do not create one crate per provider unless dependency isolation or maintenance needs justify it.
- Keep TUI components and terminal lifecycle in `inifra-tui`; command parsing and workflow decisions remain outside it.
- Do not create separate crates for individual CLI commands or for modules that only share a directory. Split when ownership, dependencies, testing needs, or reuse clearly benefit.
- Put public behavior tests under the corresponding crate's `tests/`; keep private invariant tests close to the implementation when integration tests cannot exercise them cleanly.
