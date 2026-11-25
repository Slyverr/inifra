# The composer understands readline-style word editing.
start ask
wait_for "Type a message"
type_text "alpha beta gamma"
send C-w
expect_not "gamma"
expect "alpha beta"
send C-Left
send C-w
expect_not "alpha"
send M-b
type_text "one two"
send M-BSpace
expect_not "two"
snap editing
