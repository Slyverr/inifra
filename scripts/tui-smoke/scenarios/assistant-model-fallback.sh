# When the provider's model list cannot be fetched, setup asks for a model ID instead.
start assistant configure
wait_for "Known provider"
send Down Enter
wait_for "Chat Completions"
send Down Enter
wait_for "Profile name"
type_text "offline"
send Enter
wait_for "API base URL"
type_text "http://127.0.0.1:1/v1"
send Enter
wait_for "Paste an API key"
send Enter
wait_for "└ Paste a key"
type_text "sk-offline"
send Enter
wait_for "could not fetch the model list" 20
expect "› Default model"
snap fallback
type_text "my-model"
send Enter
wait_for "● Default effort"
send Enter
wait_for "Saved profile offline" 10
grep -q 'model = "my-model"' "$CONFIG_DIR/assistant.toml" || fail "typed model not saved"
