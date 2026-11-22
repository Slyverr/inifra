# Contributing

Contributions are welcome. This guide covers development setup, project organization, coding conventions, testing, Git workflow, and releases.

## Getting Started

For a rustup-based setup, install Rust 1.100.0 or newer with Cargo, Rustfmt, and Clippy:

```sh
rustup toolchain install stable --profile minimal --component clippy --component rustfmt
```

Nix users can enter the pinned project environment with `nix develop`; see [Nix support](docs/nix.md).

Git is required for development and for using Git-backed template sources.

Run Inifra from the repository root:

```sh
cargo +stable run --locked -p inifra
```

The Rust version declared in the workspace manifest is the minimum supported Rust version (MSRV). Keep `Cargo.toml` and `Cargo.lock` consistent when changing dependencies.

## Repository Organization

Inifra is organized as a Cargo workspace. Follow the existing crate layout when adding code, tests, or documentation.

| Crate | Responsibility |
| --- | --- |
| `crates/inifra/` | CLI parsing, command dispatch, and application wiring |
| `crates/inifra-core/` | Template, project, and generation services |
| `crates/inifra-assistant/` | Assistant workflows and provider integrations |
| `crates/inifra-tui/` | Ratatui presentation and terminal interaction |
| `crates/<crate>/tests/` | Public-behavior tests for the corresponding crate |
| `.github/workflows/ci.yml` | Authoritative CI checks |
| `docs/` | Extended versioned documentation, when needed |

The crate responsibilities and allowed dependency direction are described in [`docs/architecture.md`](docs/architecture.md). Keep new modules inside their owning crate; add a crate only when a distinct responsibility, dependency boundary, or reuse case justifies it. Add tests and supporting files alongside the relevant crate.

## Development Conventions

Follow existing Rust patterns and repository tooling rather than introducing competing conventions.

- Follow Rust idioms, existing naming and architecture patterns, and repository tool configuration.
- Use Rustfmt, Clippy, and the configured lints; preserve the Rust 1.100 MSRV and `unsafe_code = "forbid"` setting.
- Keep changes focused, errors explicit, and compatibility intentional.
- Justify dependencies based on maintenance, security, licensing, and compatibility.
- Discuss substantial interface or architectural changes, including alternatives and tradeoffs, before implementation.

Use `docs/architecture.md` and `DESIGN.md` when maintained and relevant; these own detailed architecture and interface conventions.

## Testing and Quality

Prefer tests in dedicated files under each crate's `tests/`, grouped by behavior or component. Use source-local unit tests only when private internals or Rust-specific constraints make them necessary.

- Test public CLI behavior, edge cases, failures, and regressions.
- Keep tests deterministic, isolated, clearly named, and maintainable.
- Reuse fixtures where useful; avoid arbitrary per-function or percentage coverage targets.

Before requesting review, run the checks relevant to your change:

```sh
cargo +stable fmt --all -- --check
cargo +stable clippy --workspace --all-targets --locked -- -D warnings
cargo +stable test --workspace --locked
cargo +1.100.0 check --workspace --locked
cargo +1.100.0 test --workspace --locked
```

Run applicable checks and report anything not executed. The documented CI workflow runs stable Rust formatting, linting, and tests on Linux, macOS, and Windows, plus MSRV checks on Linux; consult [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for authoritative configuration.

To verify the crates.io package archives locally, run `scripts/publish-crates.sh --dry-run`. It stages the root `templates/` catalog into `inifra-core` for packaging, then removes the staged copy. Use `--allow-dirty` only for a local dry run with uncommitted changes.

## Git and Commits

Use **GitHub Flow**: short-lived feature or fix branches from `main`, PR-first integration, and no permanent `develop` branch. Roadmap chapters define milestones, **not long-lived chapter branches**. Git worktrees are optional and useful for independent concurrent changes.

Suggested branches: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, or the repository's existing scheme.

### Conventional Commits

Follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):

```text
type(scope[/area]): short imperative description
```

The scope is optional and may contain **one or two segments**, never three or more (`core/parser/lexer` is invalid). This is a project rule, not a requirement of the specification. Omit scope for changes spanning unrelated components.

For crate-owned work, use a scope that clearly names the crate: `inifra-cli` for `crates/inifra/`, or the crate name for `inifra-core`, `inifra-assistant`, and `inifra-tui`. Add one narrower area after `/` when it makes the change easier to locate.

```text
feat(inifra-cli): add interactive initialization
fix(inifra-core/inputs): reject conflicting values
test(inifra-tui): cover cancellation
docs: clarify installation
chore(release): prepare v0.13.0
```

Keep commits cohesive. A body is **optional**; add one when the title cannot explain motivation or consequences:

```text
type(scope[/area]): short imperative description

Briefly explain the purpose or motivation.

- Summarize a significant change.
- Summarize another relevant change.

Optional consequences or additional context.

Refs: #123
```

Omit unnecessary paragraphs and bullets. Use standard footers, including `BREAKING CHANGE: ...`, when applicable.

## Pull Requests and Reviews

Pull requests provide the review and integration path for contributors. Normal work targets `main`; fixes to a supported release line target its `release/vX.Y` branch (currently, `release/v0.8`).

- Use draft PRs for work in progress.
- Describe the purpose, important changes, linked issues, checks performed, and compatibility impact.
- Maintainers integrate only completed, reviewed changes with the required checks passing.
- Prefer **rebase merging** to retain meaningful individual commits in a linear history; tidy noisy commits before review. GitHub rebase merges may assign new commit hashes.
- Squash only when intermediate commits are not useful individually; the resulting commit must follow Conventional Commits.

### Project and workspace context

Keep repository-facing material—including documentation, commit messages, issues, and PR descriptions—grounded in checked-in project material, the change under review, or explicit user requirements. Do not expose private workspace plans or notes, conversation details, local machine paths, or personal environment setup as project context. Use external sources only when relevant to the change, cite them when making factual claims, and do not present them as project decisions.

## Versioning and Releases

Use [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html), with the project's additional release policy:

- **`v0.x.0`**: Development milestones, generally tied to roadmap chapters; document pre-1.0 breaking changes.
- **`v0.x.y` (`y > 0`)**: Compatible fixes and maintenance of a released `0.x` series, without unfinished next-minor features.
- **`v1.0.0`**: Defined stable public contract, resolved release blockers, and representative external testing. No fixed tester count.
- **`v1.x.y` onward**: Standard SemVer compatibility rules.

An approved roadmap guides scope, but a checked-off chapter does **not** automatically authorize a release.

### Maintenance Branches

Use a maintenance branch only when a supported release needs patches without including unfinished work from `main`. The current maintenance branch is `release/v0.8`, which starts at `v0.8.0`.

- Support only the latest published minor series by default; preserve tags for older versions. Maintain older series only when explicitly needed.
- If `main` has no unrelated unreleased features, an ordinary patch can be made and tagged on `main` after verification.
- If `main` contains work toward a newer release, create `release/vX.Y` from the **latest published tag in that series**, or reuse the existing branch. For example, create the current branch with `git switch -c release/v0.8 v0.8.0`. Patch PRs target `release/vX.Y`, not `main`, and exclude unfinished next-release features.
- **Port fixes, not releases.** `git cherry-pick` applies the change from a selected fix commit to another branch as a new commit (usually with a new SHA). Port between `main` and `release/vX.Y` in whichever direction is needed; prefer `git cherry-pick -x` for source traceability. Resolve conflicts and retest the destination branch.
- Keep release-specific version, lockfile, and changelog metadata out of ported fixes; do not merge the whole maintenance branch into `main`. A patch tag remains on its own release-line commit, even when absent from `git log main`. The maintenance branch may be deleted once support ends without losing its tags.

### Release Procedure

Release tags identify a verified revision on `main` or an active maintenance branch; both must meet the same quality requirements.

1. Verify the intended code and relevant roadmap acceptance criteria on `main` (major/minor) or the supported maintenance branch (patch). Ensure version metadata, `Cargo.lock` where applicable, documentation, and changelog match the intended release.
2. **Major/minor:** Include a final, nonempty `chore(release): prepare vX.Y.Z` commit in the PR that completes the release scope whenever practical. Use a separate release-preparation PR only when the implementation has already merged or the metadata change must be reviewed independently. No commit body is needed by default.
3. **Patch:** No required release-preparation commit. Include necessary version and changelog updates before tagging, keeping fix commits separate from release-only metadata when practical so the fix can be cleanly cherry-picked. If a separate metadata commit is necessary, use one without a ceremonial release-preparation step.
4. CI must validate PRs targeting both `main` and supported `release/vX.Y` branches. Confirm the **exact final commit** passes relevant checks before tagging. Only a maintainer or explicitly authorized automation creates the immutable `vX.Y.Z` tag on that commit.
5. Verify the tag target and signature **if signing is configured**. Tag-triggered CI tests the exact tagged revision, validates every Cargo package archive, and builds release binaries. A passing run creates a GitHub Release with platform archives and checksums.
6. After reviewing the GitHub Release, dispatch the **Publish crates.io** workflow from `main` with that tag. It verifies the tag, successful tag CI, and the GitHub Release, then publishes the workspace crates through Cargo. Configure the repository Actions secret `CARGO_REGISTRY_TOKEN` before using this workflow. Never place the token in source, workflow inputs, or logs.
7. Crates.io versions are immutable. If a workspace publish stops after publishing some crates, inspect crates.io and the workflow log before retrying; do not blindly rerun a partially completed publication.
8. If a defect is found after tagging, publish a new patch rather than moving an existing tag.

**Publication policy:** Every published version has an immutable `vX.Y.Z` Git tag. A successful tag workflow creates the matching GitHub Release and binary archives. Crates.io publication is a separate, manually dispatched workflow that runs only after tag CI and the GitHub Release are verified.

## Documentation and Support

Update documentation when behavior or project conventions change, and link to existing authoritative details instead of repeating them.

- Update `README.md` when installation, CLI usage, or supported behavior changes.
- Update relevant `docs/` documents when they exist; avoid repeating technical documentation here.
- Maintain `CHANGELOG.md` for notable user-facing changes, not every commit.
- Check existing issues and PRs to avoid overlapping work. Include reproduction steps and environment details in bug reports; use private vulnerability reporting when available.
