# Agent Instructions

This file defines **agent-specific operating rules** for this repository. Shared development policies live in [CONTRIBUTING.md](CONTRIBUTING.md); project purpose and usage live in [README.md](README.md). Do not duplicate their contents here.

## Start a Task

1. Understand the request; consult the relevant sections of `README.md` and `CONTRIBUTING.md`.
2. Read applicable nested `AGENTS.md` files and task-specific documentation (`docs/`, `DESIGN.md`, etc.) **when present and relevant**.
3. Inspect the code, tests, current Git status, and existing changes before choosing an approach.
4. Check existing branches and worktrees before starting parallel or unfinished work.

Load only the context needed for the task. Existing code and configuration take precedence over stale descriptive notes when determining actual behavior.

For user-facing changes to CLI output, terminal interfaces, or graphical interfaces, follow [`DESIGN.md`](DESIGN.md). It defines a design and validation process, not a fixed palette or a mandate to redesign unrelated screens.

## Execution and Coordination

- Follow the repository's conventions, tests, Git workflow, and integration policy in `CONTRIBUTING.md`.
- Work in the assigned branch or worktree. For independent concurrent tasks, use separate worktrees; never let agents modify the same checkout concurrently.
- Prefer the smallest maintainable change that satisfies the request. Avoid unrelated refactors or new dependencies without justification.
- Preserve unrelated modifications. Do not discard changes, force-push, delete branches/worktrees, or merge into protected branches without authorization.
- For substantial or risky changes, outline an implementation approach before editing. Use a durable plan only when multi-session continuity or technical complexity warrants it.
- Update authoritative documentation when APIs, user-visible behavior, design, or development procedures change.

## Verification and Handoff

1. Review the diff and check for accidental changes.
2. Run relevant tests, formatting, linting, and build checks as documented; state what was **not** run.
3. Follow the repository's review and PR policy. Do not equate finished implementation with merged or released work.
4. Report changes, affected files/branch, actual verification results, remaining issues, and next steps when unfinished.

Never invent results, project decisions, or completed work. Do not place secrets in code, logs, or documentation.
