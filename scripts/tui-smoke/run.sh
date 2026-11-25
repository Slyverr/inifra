#!/usr/bin/env bash
# Drives the real inifra TUI in tmux against a local mock provider and checks what is on screen.
#
# Usage: scripts/tui-smoke/run.sh [--check-coverage] [SCENARIO...]
# Requires tmux and python3. Build first: cargo build. Override the binary with BIN=path.
# Captures are written to $OUT (default: target/tui-smoke) as .txt and .ansi files.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)
BIN=${BIN:-$root/target/debug/inifra}
OUT=${OUT:-$root/target/tui-smoke}
PORT=${PORT:-18765}
export BIN OUT

if [[ ${1:-} == --check-coverage ]]; then
  missing=0
  while read -r command; do
    if ! grep -rqF -- "$command" "$here/scenarios"; then
      echo "no smoke scenario exercises $command" >&2
      missing=1
    fi
  done < <(sed -n '/^const COMMANDS/,/^\];/p' "$root/crates/inifra/src/assistant/ask/commands.rs" \
    | grep -o '"/[a-z]*"' | tr -d '"')
  exit "$missing"
fi

for tool in tmux python3; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 2; }
done
[[ -x $BIN ]] || { echo "binary not found: $BIN (run cargo build)" >&2; exit 2; }

mkdir -p "$OUT"
rm -f "$OUT"/*
python3 "$here/mock_provider.py" "$PORT" &
mock=$!
trap 'kill $mock 2>/dev/null; tmux -L inifra-smoke kill-server 2>/dev/null || true' EXIT
sleep 0.5

# shellcheck source=lib.sh
source "$here/lib.sh"

if (( $# )); then
  scenarios=("$@")
else
  mapfile -t scenarios < <(cd "$here/scenarios" && ls *.sh | sed 's/\.sh$//')
fi

failed=0
for scenario in "${scenarios[@]}"; do
  export SCENARIO=$scenario SESSION=smoke-$scenario FAILED=0
  export CONFIG_DIR PROJECT_DIR
  CONFIG_DIR=$(mktemp -d) PROJECT_DIR=$(mktemp -d)
  printf '[package]\nname = "demo"\nversion = "0.1.0"\n' > "$PROJECT_DIR/Cargo.toml"
  printf '# Demo\n' > "$PROJECT_DIR/README.md"
  cat > "$CONFIG_DIR/assistant.toml" <<TOML
schema_version = 1
default_profile = "mock"

[profiles.mock]
provider = "custom-endpoint"
api = "open-ai-chat-completions"
model = "mock-1"
base_url = "http://127.0.0.1:$PORT/v1"
TOML
  echo "== $scenario"
  # shellcheck source=/dev/null
  ( source "$here/scenarios/$scenario.sh"; exit "$FAILED" ) || failed=1
  tmux -L inifra-smoke kill-session -t "$SESSION" 2>/dev/null || true
  rm -rf "$CONFIG_DIR" "$PROJECT_DIR"
done

if (( failed )); then echo "tui-smoke: FAILED (captures in $OUT)"; exit 1; fi
echo "tui-smoke: all scenarios passed (captures in $OUT)"
