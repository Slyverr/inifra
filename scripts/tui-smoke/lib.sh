# Helpers sourced by run.sh and by every scenario. Scenarios call these functions only.
# All tmux calls use a private socket so the runner can never touch the user's own sessions.
tm() { tmux -L inifra-smoke "$@"; }

# start CMD...  launch the command in a fresh tmux session sized COLS x ROWS.
start() {
  tm new-session -d -s "$SESSION" -x "${COLS:-100}" -y "${ROWS:-30}" -c "$PROJECT_DIR" \
    -e "INIFRA_CONFIG_DIR=$CONFIG_DIR" -e "NO_COLOR=${NO_COLOR:-}" \
    "$BIN $* ; sleep 600"
}

screen() { tm capture-pane -t "$SESSION" -p | sed -e 's/[[:space:]]*$//'; }

# send KEYS...  send tmux key names or literal text.
send() { tm send-keys -t "$SESSION" "$@"; }

# type TEXT  type literal text, then wait a beat so the UI redraws.
type_text() { tm send-keys -t "$SESSION" -l -- "$1"; sleep 0.2; }

# wait_for TEXT [SECONDS]  poll the screen until TEXT appears.
wait_for() {
  local text=$1 limit=${2:-15} waited=0
  while (( waited < limit * 10 )); do
    if screen | grep -qF -- "$text"; then return 0; fi
    sleep 0.1
    waited=$((waited + 1))
  done
  fail "timed out after ${limit}s waiting for: $text"
  return 1
}

# expect TEXT / expect_not TEXT  assert on the current screen.
expect() { screen | grep -qF -- "$1" || fail "expected on screen: $1"; }
expect_not() { ! screen | grep -qF -- "$1" || fail "unexpected on screen: $1"; }

# expect_width MAX  assert no line is wider than MAX columns.
expect_width() {
  local widest
  widest=$(screen | awk '{ if (length($0) > max) max = length($0) } END { print max + 0 }')
  (( widest <= $1 )) || fail "a line is $widest columns wide, limit $1"
}

# snap NAME  save the screen as text and with ANSI colors.
snap() {
  screen > "$OUT/$SCENARIO-$1.txt"
  tm capture-pane -t "$SESSION" -p -e > "$OUT/$SCENARIO-$1.ansi"
}

fail() {
  echo "  FAIL: $*" >&2
  FAILED=1
  snap "failure-$(date +%s%N)"
}
