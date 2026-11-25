# Setup end to end against the mock provider: fields stay in view, words edit like a shell, the
# key is pasted, and the model is picked from the provider's own list.
start assistant configure
wait_for "Known provider"
send Down Enter
wait_for "Chat Completions"
send Down Enter
wait_for "Profile name"
type_text "Work"
send Enter
wait_for "step 2 of 2"
expect "✓ Profile name"
expect "› API base URL"
snap stepper
type_text "http://127.0.0.1:$PORT/v1 extra"
send C-w
expect_not "extra"
send Enter
wait_for "Paste an API key"
expect "Use an environment variable"
expect "Use a different variable"
snap key
send Enter
wait_for "API key"
type_text "sk-smoke-123"
send Enter
wait_for "Enter a model ID" 15
expect "mock-2"
expect "/ filter"
snap models
send /
type_text "-2"
send Enter
wait_for "Default effort"
send Enter
wait_for "Saved profile work" 10
snap saved
cp "$CONFIG_DIR/assistant.toml" "$OUT/$SCENARIO-assistant.toml"
grep -q 'model = "mock-2"' "$CONFIG_DIR/assistant.toml" || fail "model not saved"
grep -q "base_url = \"http://127.0.0.1:$PORT/v1\"" "$CONFIG_DIR/assistant.toml" || fail "base url not saved"
grep -q 'api_key_env = "WORK_API_KEY"' "$CONFIG_DIR/assistant.toml" || fail "key variable not saved"
grep -q 'sk-smoke-123' "$CONFIG_DIR/credentials.env" || fail "key not stored"
! grep -q 'sk-smoke-123' "$CONFIG_DIR/assistant.toml" || fail "key leaked into assistant.toml"
