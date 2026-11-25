# Create is the first row and has the focus; a blank row separates it from the profiles; no Reset row.
start assistant
wait_for "Create a profile"
expect "mock"
expect "default"
expect "Space mark"
expect_not "Reset assistant data"
expect_not "Edit a profile"
expect_not "Change default profile"
snap home
