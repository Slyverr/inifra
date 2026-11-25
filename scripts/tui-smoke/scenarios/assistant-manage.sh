# A profile opens its own menu: edit one part, make another profile the default.
cat >> "$CONFIG_DIR/assistant.toml" <<TOML

[profiles.second]
provider = "ollama"
api = "ollama-chat"
model = "llama3.2"
credential_source = "environment"
TOML
start assistant
wait_for "Create a profile"
expect "second"
expect "Ollama · Ollama Chat"
snap list
send Enter
# Focus starts on the default profile ("mock"), so its menu has no Make default.
wait_for "Connection"
expect "Model"
expect "mock-1"
expect_not "Make default"
snap profile
send Escape
wait_for "Create a profile"
send Down Enter
wait_for "Make default"
expect "Edit"
expect "Remove"
snap second
send Down Enter
sleep 1
send Escape
wait_for "Create a profile"
grep -q 'default_profile = "second"' "$CONFIG_DIR/assistant.toml" || fail "default profile not changed"
