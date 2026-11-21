import json
import os
import re
import subprocess
import sys
from pathlib import Path


tag = os.environ["RELEASE_TAG"]
tag_type = subprocess.check_output(["git", "cat-file", "-t", tag], text=True).strip()
if tag_type != "tag":
    sys.exit(f"release tag {tag!r} is not annotated")

commit = subprocess.check_output(["git", "rev-parse", f"{tag}^{{commit}}"], text=True).strip()
expected_sha = os.environ.get("RELEASE_SHA")
if expected_sha and commit != expected_sha:
    sys.exit(f"release tag {tag!r} targets {commit}, expected {expected_sha}")

manifest = json.loads(
    subprocess.check_output(
        ["cargo", "+stable", "read-manifest", "--manifest-path", "crates/inifra/Cargo.toml"],
        text=True,
    )
)
version = manifest["version"]
if tag != f"v{version}":
    sys.exit(f"release tag {tag!r} does not match Cargo version {version!r}")

changelog = Path("CHANGELOG.md").read_text(encoding="utf-8")
heading = re.compile(rf"^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$", re.MULTILINE)
if not heading.search(changelog):
    sys.exit(f"CHANGELOG.md has no dated section for {version}")

print(f"Validated {tag} at {commit} against Cargo.toml and CHANGELOG.md")
