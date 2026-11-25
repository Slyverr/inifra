# Space marks profiles, d deletes the marked ones after a confirmation; marking all is a reset.
cat >> "$CONFIG_DIR/assistant.toml" <<TOML

[profiles.alpha]
provider = "ollama"
api = "ollama-chat"
model = "llama3.2"
credential_source = "environment"

[profiles.beta]
provider = "ollama"
api = "ollama-chat"
model = "llama3.2"
credential_source = "environment"
TOML
start assistant
wait_for "Create a profile"
send Home
send Down
send Space
send Down
send Space
wait_for "☑"
expect "Esc clear"
snap marked
send d
wait_for "Remove 2 profiles"
expect "alpha, beta"
send y Enter
wait_for "Removed profiles"
wait_for "Create a profile"
grep -q 'profiles.alpha' "$CONFIG_DIR/assistant.toml" && fail "alpha was not removed"
grep -q 'profiles.beta' "$CONFIG_DIR/assistant.toml" && fail "beta was not removed"
grep -q 'profiles.mock' "$CONFIG_DIR/assistant.toml" || fail "mock should remain"
snap after
