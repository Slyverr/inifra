# The setup lists fit a 50-column terminal.
COLS=50 ROWS=20
start assistant configure
wait_for "Known provider"
send Enter
wait_for "OpenAI"
expect_width 50
snap narrow
