#!/usr/bin/env bash
set -euo pipefail

tag="${1:?usage: check-release-metadata.sh TAG COMMIT}"
commit="${2:?usage: check-release-metadata.sh TAG COMMIT}"

if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: release tag must use vMAJOR.MINOR.PATCH: $tag" >&2
  exit 1
fi

version="${tag#v}"
tag_ref="refs/tags/$tag"
tag_type="$(git cat-file -t "$tag_ref" 2>/dev/null || true)"
if [[ "$tag_type" != "tag" ]]; then
  echo "error: release tag must be annotated: $tag" >&2
  exit 1
fi

tag_commit="$(git rev-parse "$tag_ref^{commit}")"
if [[ "$tag_commit" != "$commit" ]]; then
  echo "error: release tag $tag does not point to commit $commit" >&2
  exit 1
fi

manifest="$(git show "$commit:Cargo.toml")"
lockfile="$(git show "$commit:Cargo.lock")"
changelog="$(git show "$commit:CHANGELOG.md")"

manifest_version="$(awk '
  /^\[package\]$/ { in_package = 1; next }
  /^\[/ { in_package = 0 }
  in_package && /^version = / { gsub(/"/, "", $3); print $3; exit }
' <<< "$manifest")"
lock_version="$(awk '
  /^\[\[package\]\]$/ { is_inifra = 0 }
  /^name = "inifra"$/ { is_inifra = 1; next }
  is_inifra && /^version = / { gsub(/"/, "", $3); print $3; exit }
' <<< "$lockfile")"

if [[ "$manifest_version" != "$version" ]]; then
  echo "error: tag $tag does not match Cargo.toml version $manifest_version" >&2
  exit 1
fi

if [[ "$lock_version" != "$version" ]]; then
  echo "error: tag $tag does not match Cargo.lock version $lock_version" >&2
  exit 1
fi

if ! grep -Fq "## [$version] - " <<< "$changelog"; then
  echo "error: CHANGELOG.md has no release heading for $version" >&2
  exit 1
fi

echo "release metadata is consistent for $tag"
