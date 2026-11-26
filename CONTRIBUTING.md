# Contributing

Contributions are welcome. This guide is the single source of truth for development setup, conventions, checks, Git workflow, and releases.

The words **must** and **never** are requirements. **Should** is the default, to be departed from only with a stated reason.

## Getting Started

Install a Rust toolchain at or above the minimum supported Rust version (MSRV), with Cargo, Rustfmt, and Clippy:

```sh
rustup toolchain install stable --profile minimal --component clippy --component rustfmt
```

Nix users can enter the pinned environment with `nix develop`; see [Nix support](docs/nix.md). Git is required for development and for Git-backed template sources.

Run Inifra from the repository root:

```sh
cargo +stable run --locked -p inifra
```

The `rust-version` in the workspace `Cargo.toml` is the MSRV, and `unsafe_code = "forbid"` applies workspace-wide. Keep `Cargo.toml` and `Cargo.lock` consistent; always build and test with `--locked`.

## Repository Organization

Inifra is a Cargo workspace. Follow the existing layout.

| Path | Responsibility |
| --- | --- |
| `crates/inifra/` | CLI parsing, command dispatch, application wiring |
| `crates/inifra-core/` | Template, project, and generation services |
| `crates/inifra-assistant/` | Assistant workflows and provider integrations |
| `crates/inifra-tui/` | Ratatui presentation and terminal interaction |
| `crates/<crate>/tests/` | Public-behavior tests for that crate |
| `templates/`, `presets/` | Built-in template and preset catalogs |
| `scripts/` | Release validation, crate publishing, and the TUI smoke runner |
| `.github/workflows/` | Authoritative CI and release automation |
| `docs/` | Extended documentation |

[`docs/architecture.md`](docs/architecture.md) owns crate responsibilities and the allowed dependency direction; [`DESIGN.md`](DESIGN.md) owns interface and terminal-design conventions. Keep new modules inside their owning crate. Add a crate only when a distinct responsibility, dependency boundary, or reuse case justifies it.

## Development Conventions

- Follow existing Rust idioms, naming, and architecture. Do not introduce a competing convention.
- Keep changes focused. Do not mix refactors, formatting churn, or unrelated fixes into a feature or fix.
- Make errors explicit and compatibility changes intentional; document breaking changes.
- Justify every new dependency by maintenance, security, licensing, and compatibility. Prefer the standard library and existing dependencies.
- Discuss substantial interface or architecture changes, with alternatives and tradeoffs, in an issue or draft PR before implementing them.
- Never commit secrets, tokens, or local machine paths.

## Testing and Quality

Prefer tests in dedicated files under each crate's `tests/`, grouped by behavior or component. Use source-local unit tests only when private internals or Rust-specific constraints require them.

- Cover public behavior, edge cases, failures, and regressions. A bug fix includes a test that fails without it.
- Keep tests deterministic, isolated, and independent of the terminal, network, and the developer's configuration.
- Do not chase per-function or percentage coverage targets.

### Required checks

Run these before every push and report any you did not run. They mirror CI:

```sh
cargo +stable fmt --all -- --check
cargo +stable clippy --workspace --all-targets --locked -- -D warnings
cargo +stable test --workspace --locked
cargo +<msrv> check --workspace --locked
cargo +<msrv> test --workspace --locked
```

`<msrv>` is the toolchain CI pins in [`ci.yml`](.github/workflows/ci.yml), which matches `rust-version` in `Cargo.toml`. CI additionally runs the stable checks on Linux, macOS, and Windows, verifies the crates.io packages, and builds the Nix package; consult `ci.yml` for the exact configuration.

### Additional checks

| When you change | Also run |
| --- | --- |
| An interactive screen or slash command | The terminal smoke runner (below) |
| Crate manifests, packaged files, or `templates/` | `scripts/publish-crates.sh --dry-run` (add `--allow-dirty` only for local runs with uncommitted changes) |
| `flake.nix`, `nix/`, or `Cargo.lock` | `nix flake check --no-write-lock-file` |

The smoke runner drives the real binary in a private tmux server against a local mock provider, using temporary configuration and project directories, and writes captures to `target/tui-smoke/`. It needs `tmux` and `python3` (both in the Nix shell), runs only on Unix, and is not part of CI:

```sh
cargo build
scripts/tui-smoke/run.sh                  # every scenario in scripts/tui-smoke/scenarios/
scripts/tui-smoke/run.sh ask-chat         # one scenario
scripts/tui-smoke/run.sh --check-coverage # every slash command appears in a scenario
```

Add a scenario file whenever you add a screen or a slash command.

## Git Workflow

Use **GitHub Flow**: short-lived branches from `main`, no permanent `develop` branch, and no long-lived feature branches. `main` must always be releasable.

### Branches and worktrees

- Name branches `<type>/<topic>` in lowercase kebab-case, using a Conventional Commit type: `feat/chat-history`, `fix/profile-name`, `docs/contributing`.
- Start every branch from the current `main`, or from the relevant `release/vX.Y` branch for a patch to a supported release.
- Use one branch per change and one worktree per concurrent change (`git worktree add ../<name> -b <branch>`). Never let two people modify the same checkout at once.
- Delete a branch, its worktree, and its remote branch once its change is merged or abandoned.
- Never rewrite history that others may have based work on. Force-push only your own unmerged branch, with `--force-with-lease`; never force-push `main` or a `release/vX.Y` branch, except to undo a mistaken push, and then only by a maintainer.

### Conventional Commits

Follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):

```text
type(scope[/area]): short imperative description
```

- **Types:** `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`, `revert`.
- **Subject:** imperative mood, lowercase after the colon, no trailing period, at most 72 characters.
- **Scope** is optional and has **one or two segments**, never three (`core/parser/lexer` is invalid). Omit it for changes spanning unrelated components.
- **Crate scopes:** `inifra-cli` for `crates/inifra/`, otherwise the crate name (`inifra-core`, `inifra-assistant`, `inifra-tui`). Add one narrower area after `/` when it helps locate the change.
- **Breaking changes** use `!` after the scope and a `BREAKING CHANGE:` footer.

```text
feat(inifra-cli): add interactive initialization
fix(inifra-core/inputs): reject conflicting values
test(inifra-tui): cover cancellation
docs: clarify installation
chore(release): prepare vX.Y.Z
```

A body is optional. Add one when the title cannot explain the motivation or consequences: a short paragraph of why, then bullets for significant changes, then footers such as `Refs: #123`. Omit empty sections.

Keep commits cohesive: each one builds, passes the required checks, and does one thing. Tidy fixup and noise commits before review.

## Pull Requests and Reviews

Everything reaches `main` or a `release/vX.Y` branch through one of two paths.

**Pull request (the default).** Required for every contributor and for any change that is not small. Use a draft PR for work in progress. Fill in the PR template: purpose, changes, review focus, exact checks run with results, and compatibility impact. The PR title follows Conventional Commits. Required CI checks must pass and a maintainer must approve before merging.

**Direct fast-forward (maintainers only).** A maintainer may fast-forward `main` for a small, low-risk change (a typo, a comment, a CI or documentation tweak, a one-line fix) when the required checks pass locally and the history stays linear:

```sh
git switch main && git pull --ff-only
git merge --ff-only <branch>
git push origin main
```

If in doubt, open a PR.

### Merging

- Prefer **rebase merging** to keep meaningful commits in a linear history. GitHub may assign new hashes.
- Squash only when intermediate commits are not useful alone; the resulting commit must follow Conventional Commits and keep any `BREAKING CHANGE:` footer.
- Do not merge with failing or unrun required checks, unresolved review threads, or merge commits.

### Project and workspace context

Keep repository-facing material (documentation, commit messages, issues, and PR descriptions) grounded in checked-in project material, the change under review, or explicit requirements. Do not expose private workspace plans or notes, conversation details, local machine paths, or personal environment setup. Use external sources only when relevant, cite them for factual claims, and do not present them as project decisions.

## Versioning and Releases

Use [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) with these additions:

- **`0.Y.0`:** development milestones; document breaking changes in the changelog.
- **`0.Y.Z` (`Z > 0`):** compatible fixes to a released `0.Y` series, with no unfinished next-minor features.
- **`1.0.0`:** a defined stable public contract, resolved release blockers, and representative external testing.
- **`1.Y.Z` onward:** standard SemVer compatibility rules.

Finished work does not by itself authorize a release; a maintainer decides when to release.

### Release branches

A `release/vX.Y` branch exists to patch the **latest released minor series** without including unfinished work from `main`. This document names it only as `release/vX.Y`, so nothing here needs editing at release time.

- **Create:** after tagging a new minor release `vX.Y.0`, create `release/vX.Y` from that tag and push it:

  ```sh
  git branch release/vX.Y vX.Y.0 && git push -u origin release/vX.Y
  ```

- **Retire:** at the same time, delete the previous series' branch (`git push origin --delete release/vX.(Y-1)` and the local copy). Branches do not outlive their series; the tags remain, so nothing is lost. Maintain an older series only when a maintainer explicitly asks, and then recreate its branch from its latest tag.
- **Patch:** if `main` holds no unreleased work, make and tag the patch on `main`. Otherwise make it on `release/vX.Y` through a PR that targets that branch, without unfinished next-release features.
- **Port fixes, not releases.** Move a fix between `main` and `release/vX.Y` with `git cherry-pick -x`, resolving conflicts and retesting on the destination. Keep version, lockfile, and changelog release metadata out of ported commits, and never merge a whole release branch into `main`. A patch tag lives on its own release-branch commit even though it is absent from `git log main`.

### Changelog

Maintain [`CHANGELOG.md`](CHANGELOG.md) in [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) format for notable user-facing changes, not every commit.

- Add entries under `## [Unreleased]` in the PR that makes the change.
- Write each entry as a user-visible outcome in the imperative, under `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`.
- At release, rename `Unreleased` to `## [X.Y.Z] - YYYY-MM-DD` and leave a new empty `## [Unreleased]` above it.
- The release workflow publishes that section as the GitHub Release notes, so it must read well on its own.

### Release procedure

Releases are cut from a verified commit on `main` or on the current `release/vX.Y` branch; both meet the same bar.

1. **Prepare.** Confirm the code, documentation, `Cargo.lock`, crate versions, and changelog match the intended version. The changelog must contain a dated `## [X.Y.Z] - YYYY-MM-DD` section; the tag workflow fails without one.
2. **Release commit.** For `X.Y.0`, end the completing PR with a nonempty `chore(release): prepare vX.Y.Z` commit, or use a separate release-preparation PR when the implementation already merged or the metadata needs independent review. For a patch, keep fix commits separate from release-only metadata so the fix cherry-picks cleanly. No commit body is needed.
3. **Verify.** CI must pass on the **exact final commit** before tagging; it runs on pushes and on PRs targeting `main` and `release/**`.
4. **Tag.** Only a maintainer, or automation they explicitly authorize, creates the tag. It must be **annotated** and point at the verified commit; CI rejects lightweight tags and mismatches with the crate version:

   ```sh
   git tag -a vX.Y.Z -m "Inifra vX.Y.Z" && git push origin vX.Y.Z
   ```

   Sign the tag and verify its target if signing is configured.
5. **Tag CI.** It validates the tag against `Cargo.toml` and `CHANGELOG.md`, tests the exact tagged revision, verifies every Cargo package archive, builds platform archives, and creates the GitHub Release with the changelog section as its notes, the archives, and `SHA256SUMS`.
6. **Branch.** For `X.Y.0`, create `release/vX.Y` and retire the previous one as described in [Release branches](#release-branches).
7. **Review the GitHub Release**, then dispatch the **Publish crates.io** workflow from `main` with the tag. It verifies the tag, a passing tag CI run, and the GitHub Release before publishing the workspace crates. The `CARGO_REGISTRY_TOKEN` Actions secret must be configured; never put the token in source, workflow inputs, or logs.
8. **Handle failures.** Crates.io versions are immutable. If a publish stops partway, inspect crates.io and the workflow log before retrying; never blindly rerun it. If a defect is found after tagging, publish a new patch; never move or delete a published tag.

Every published version has an immutable `vX.Y.Z` tag and a matching GitHub Release. Crates.io publication is a separate, manual step that runs only after tag CI and the GitHub Release are verified.

## Documentation and Support

Update documentation in the same change that alters behavior or conventions, and link to the authoritative document instead of repeating it.

- `README.md`: installation, CLI usage, supported behavior.
- `docs/`: detailed technical and user documentation.
- `DESIGN.md`: terminal and interface design.
- This file: development and release policy. Keep it free of version numbers, branch names, and dates that go stale.

Check existing issues and PRs to avoid overlapping work. Include reproduction steps and environment details in bug reports, and use private vulnerability reporting for security issues when available.

## Before You Finish

- [ ] The change is focused, and the diff contains no accidental edits.
- [ ] Required checks pass, plus any additional checks that apply; anything not run is stated.
- [ ] Tests cover the change, and bug fixes include a regression test.
- [ ] Documentation and `CHANGELOG.md` are updated for user-facing changes.
- [ ] Commits follow Conventional Commits.
- [ ] Compatibility impact is described, or stated as none.
