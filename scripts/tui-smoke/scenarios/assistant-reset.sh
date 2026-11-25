# Marking every profile with a, then d, resets all assistant data.
cat >> "$CONFIG_DIR/assistant.toml" <<TOML

[profiles.alpha]
provider = "ollama"
api = "ollama-chat"
model = "llama3.2"
credential_source = "environment"
TOML
echo "SOME_KEY=secret" > "$CONFIG_DIR/credentials.env"
start assistant
wait_for "Create a profile"
send a
wait_for "☑"
send d
wait_for "Remove all profiles and saved credentials?"
snap confirm
send y Enter
wait_for "Assistant data removed"
[[ ! -e "$CONFIG_DIR/assistant.toml" ]] || fail "assistant.toml was not removed"
[[ ! -e "$CONFIG_DIR/credentials.env" ]] || fail "credentials.env was not removed"
