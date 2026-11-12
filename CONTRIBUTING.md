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

## Public interface and versioning

Inifra's public interface includes its CLI commands, options, prompts and
interactive picker behavior; template pack and shared-input formats; project
and user configuration formats; and the files and effects produced by those
features. Internal Rust modules and implementation details are not public
interfaces unless they become a documented library API.

Use Semantic Versioning (`MAJOR.MINOR.PATCH`). Before `1.0.0`, the public
interface is unstable: use MINOR for meaningful new functionality or
incompatible changes, and PATCH for backward-compatible bug fixes. Choose the
highest applicable increment for a release. Use `1.0.0` when the interface is
ready for compatibility guarantees. Prerelease identifiers such as
`0.8.0-alpha.1`, `0.8.0-beta.1`, and `0.8.0-rc.1` may be used when appropriate.

A commit does not imply a release. Group changes into deliberate releases;
there is no minimum number of commits or fixed release schedule. A PATCH can
group fixes, a MINOR can include features and fixes, and a MAJOR can include
breaking changes, features, and fixes. Critical fixes may be released sooner.
Keep internal changes unreleased when publication is unnecessary.

## Changelog and releases

Record notable user-facing changes in `CHANGELOG.md` under `Unreleased`,
grouped as `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`
as needed. Curate entries for users; do not copy the commit log. When preparing
a release, move applicable entries to a versioned section with its release
date and retain all historical entries.

Use `vX.Y.Z` for release tags. Every published version has one annotated,
signed tag on its exact validated release commit. Published tags and release
identifiers are immutable. Publish a GitHub Release for distributed versions,
with concise notes based on the changelog and artifacts when applicable.

Prepare a release as follows:

1. Complete the intended changes on `develop` and run the development checks.
2. Choose the highest applicable SemVer increment. Move noteworthy entries
   from `Unreleased` to `## [X.Y.Z] - YYYY-MM-DD`, add a fresh `Unreleased`
   section, update `Cargo.toml`, regenerate `Cargo.lock` with
   `cargo update --workspace`, and update release-specific metadata such as
   the README install tag when needed.
3. Release-preparation commits are optional. Make a non-empty
   `chore(release): prepare vX.Y.Z` commit when tracked release files need
   changes. Do not create an empty commit merely to mark a release. In Inifra,
   the package version, lockfile, and changelog normally require this commit.
4. Run the development checks and `cargo package --locked` on the exact
   candidate commit. Fast-forward `main` to that validated commit, push both
   branches, and wait for branch CI to pass.
5. Create and push a signed annotated tag on that exact commit:

   ```sh
   git tag -s vX.Y.Z -m "Inifra vX.Y.Z" main
   git push origin vX.Y.Z
   ```

6. Wait for tag CI to pass, then publish the GitHub Release with the matching
   changelog entry as its notes. Attach distributable artifacts when
   applicable.

CI runs on pushes, pull requests, and tags. On release tags it verifies that
the tag is annotated, points to the checked commit, and agrees with the
package, lockfile, and changelog versions. CI does not publish releases.
Version declarations, tags, artifacts, and published package versions must
agree.

## Branches and pull requests

Keep the established two-branch workflow: `develop` is the integration branch
and `main` is kept at validated release commits. Contributors should work in
short-lived branches, including branches in forks, and target pull requests to
`develop`. After a release candidate on `develop` passes checks, fast-forward
`main` to the same commit. Do not add permanent release branches or change the
branch and worktree layout without an explicit project decision.

## Commit conventions

Use Conventional Commits, with a type and an optional scope:

```text
feat(templates): add shared template inputs
fix(cli): handle an empty destination
refactor(output): isolate editor workflows
perf(templates): reduce repeated parsing
docs: explain template sources
test(cli): cover invalid options
build: update Rust dependencies
ci: validate release metadata
chore: maintain repository tooling
```

Use `!` in the subject or a `BREAKING CHANGE:` footer for incompatible public
interface changes. Other commit types do not imply a version increment by
themselves. Keep each commit logically cohesive; avoid unrelated work and
unnecessary administrative commits.

Add a body only when it conveys useful context that does not fit in the
subject. When present, use this order: one short introductory paragraph, a
bullet list of changes or details, optional short concluding paragraph(s),
then any trailers. Keep that structure consistent:

```text
fix(templates): reject empty output paths

Reject empty paths before planning generated files.

- Cover empty and whitespace-only paths.
- Keep the existing error message.

This validation runs before any output files are written.

Verified with `cargo test --locked`.
Refs: #123
```

Use `#123` to refer to a GitHub issue or pull request when useful. Use closing
keywords such as `Closes #123` when the commit or pull request should close an
issue. Put trailers last. Do not create a chain of generic fix commits when a
cohesive correction can be folded into its logical change.
