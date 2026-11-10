# Contributing

Keep Inifra understandable and useful to people preparing project files from
templates. Make each change a small, buildable step, and include the tests and
documentation that belong to it.

## Development

The minimum supported Rust version is 1.88. Use Rust 1.88 or newer for
development, and check the minimum version during release validation. Before
submitting Rust changes, run the relevant checks:

```sh
cargo fmt -- --check
cargo check --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

## Design changes

For substantial features or architecture changes, clarify the use case and
constraints, compare plausible approaches, and explain the chosen tradeoffs
before implementation. Discuss unresolved choices that could materially affect
CLI behavior, template formats, filesystem safety, architecture, or workflow
before proceeding. Consider the experience of contributors, template authors,
and people using Inifra. Prefer the simplest design that meets the need without
imposing unnecessary limits.

## Test organization

Put tests for behavior visible through the CLI in the top-level `tests/`
directory by default. Cargo compiles these as integration-test crates, so they
verify how users invoke the program.

Use source-local `#[cfg(test)]` unit tests when a test needs private functions,
private types, or internal invariants that cannot reasonably be checked
through the CLI. Do not make an item public or `pub(crate)` only to reach it
from an integration test. Keep tests focused on one behavior; several
assertions are fine when they verify different parts of that same behavior.
Add a regression test when fixing a bug.

Use mocks, property-testing tools, snapshots, and coverage tools when they
solve a concrete problem. They are optional techniques, not required
dependencies or blanket coverage targets. Keep documentation accurate to
implemented and verified capabilities.

Benchmark only when a change has a concrete performance goal or a measured
performance concern. Use representative workloads and compare against a
baseline; do not add benchmark infrastructure speculatively.

## Changelog and releases

Record notable user-facing changes in `CHANGELOG.md` under `Unreleased`,
grouped as `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`
as needed. Curate entries for users; do not copy the commit log.

For a release, move entries to `## [X.Y.Z] - YYYY-MM-DD`, add a fresh
`Unreleased` section, and keep the Cargo package version, changelog, and Git
tag aligned. Regenerate `Cargo.lock` with Cargo, then run the development
checks above and `cargo package --locked`. Use `chore(release): prepare vX.Y.Z`
for the release-preparation commit.

## Commit conventions

Use `type(scope): message`, for example:

```text
feat(templates): add shared template inputs
chore(deps): add TOML manifest parsing
```

Choose a type such as `feat`, `fix`, `refactor`, `test`, `docs`, `chore`,
`perf`, or `ci`. Use the affected command or area as the scope, with at most
two path segments (`A/B`). Omit an unnecessary scope (`docs: explain setup`).
Use `chore(deps): message` for dependency changes.

Add a brief body only when the subject cannot express useful context. A body
may have a paragraph, related bullets, a closing note, and a trailer; include
only what helps and put trailers last:

```text
fix(templates): reject empty output paths

Reject empty paths before planning generated files.

- Cover empty and whitespace-only paths.
- Keep the existing error message.

Verified with `cargo test --locked`.
Refs: #123
```

Make each commit one coherent change; fold fixes into it instead of adding a
chain of generic `fix` commits.
