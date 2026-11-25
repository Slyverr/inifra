# The profile form keeps every field in place, edits words like a shell, and then asks for a key.
start assistant configure
wait_for "Known provider"
send Enter
wait_for "OpenAI"
send Enter
wait_for "Profile name"
type_text "Work"
send Enter
wait_for "step 2 of 3"
expect "✓ Profile name"
expect "› Default model"
expect "· Default effort"
snap stepper
type_text "gpt-5 extra"
send C-w
expect_not "extra"
expect "gpt-5"
send Enter
wait_for "step 3 of 3"
expect "✓ Default model"
send Enter
wait_for "Paste an API key"
expect "Use an environment variable"
expect "Use a different variable"
snap key
send Enter
wait_for "API key"
type_text "sk-smoke-123"
send Enter
wait_for "Saved profile work" 10
snap saved
cp "$CONFIG_DIR/assistant.toml" "$OUT/$SCENARIO-assistant.toml"; cp "$CONFIG_DIR/credentials.env" "$OUT/$SCENARIO-credentials.env" 2>/dev/null
grep -q 'model = "gpt-5"' "$CONFIG_DIR/assistant.toml" || fail "model not saved"
grep -q 'api_key_env = "OPENAI_API_KEY"' "$CONFIG_DIR/assistant.toml" || fail "key variable not saved"
grep -q 'sk-smoke-123' "$CONFIG_DIR/credentials.env" || fail "key not stored"
! grep -q 'sk-smoke-123' "$CONFIG_DIR/assistant.toml" || fail "key leaked into assistant.toml"
