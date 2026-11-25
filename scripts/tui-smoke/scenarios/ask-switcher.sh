# Ctrl+O opens the profile, model, and effort switcher; Esc closes it.
start ask
wait_for "Type a message"
send C-o
wait_for "Profiles"
expect "mock"
snap switcher
send Escape
wait_for "Type a message"
