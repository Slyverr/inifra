# Edit lists the parts of a profile with their values and changes only the one you pick.
start assistant
wait_for "Create a profile"
send Down Enter
wait_for "Edit"
send Enter
wait_for "Effort"
expect "Model"
expect "mock-1"
expect "Connection"
snap parts
send Enter
wait_for "Enter a model ID" 15
expect "mock-2"
send /
type_text "-2"
send Enter
wait_for "Saved profile mock" 10
wait_for "Effort"
grep -q 'model = "mock-2"' "$CONFIG_DIR/assistant.toml" || fail "model not changed"
grep -q 'base_url = "http://127.0.0.1:' "$CONFIG_DIR/assistant.toml" || fail "base url lost"
snap after
