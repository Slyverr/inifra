# The manager lists saved profiles as rows, focused on the default, with the badge always visible.
start assistant
wait_for "Create a profile"
expect "mock"
expect "default"
expect "Reset assistant data"
expect_not "Edit a profile"
expect_not "Change default profile"
snap home
