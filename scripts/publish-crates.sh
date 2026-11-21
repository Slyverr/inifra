#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/publish-crates.sh (--dry-run|--publish) [--allow-dirty]

Stages the root templates/ catalog into inifra-core for Cargo packaging, then
removes the staged copy. --publish uploads every workspace crate to crates.io.
EOF
}

if [[ $# -eq 0 ]]; then
  usage >&2
  exit 2
fi

mode=$1
shift
case "$mode" in
  --dry-run|--publish) ;;
  *) usage >&2; exit 2 ;;
esac

allow_dirty=0
if [[ ${1:-} == --allow-dirty ]]; then
  allow_dirty=1
  shift
fi

if [[ $allow_dirty -eq 1 && "$mode" != "--dry-run" ]]; then
  printf '%s\n' '--allow-dirty is only supported with --dry-run.' >&2
  exit 2
fi

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
core_templates="$repo_root/crates/inifra-core/templates"

if [[ "$mode" == "--publish" ]]; then
  release_tag=$(git -C "$repo_root" describe --exact-match --tags HEAD)
  RELEASE_TAG="$release_tag" python3 "$repo_root/scripts/validate-release-tag.py"
fi

if [[ -e "$core_templates" ]]; then
  printf 'Refusing to overwrite existing staged templates: %s\n' "$core_templates" >&2
  exit 1
fi

if [[ $allow_dirty -eq 0 ]]; then
  if ! git -C "$repo_root" diff --quiet || ! git -C "$repo_root" diff --cached --quiet; then
    printf 'Refusing to package a dirty worktree; use --allow-dirty for a local dry run.\n' >&2
    exit 1
  fi

  if [[ -n $(git -C "$repo_root" ls-files --others --exclude-standard) ]]; then
    printf 'Refusing to package untracked files; use --allow-dirty for a local dry run.\n' >&2
    exit 1
  fi
fi

cp -R "$repo_root/templates" "$core_templates"
cleanup() {
  rm -rf -- "$core_templates"
}
trap cleanup EXIT

cargo_args=(publish --workspace --locked --allow-dirty)
if [[ "$mode" == "--dry-run" ]]; then
  cargo_args+=(--dry-run)
fi
cargo "${cargo_args[@]}" "$@"
