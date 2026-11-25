# At 50 columns nothing overflows and the footer still fits.
COLS=50 ROWS=24
start ask
wait_for "Type a message"
send -l "/help"; send Enter
wait_for "Show this help"
expect_width 50
send -l "hello"; send Enter
wait_for "● Inifra reads" 20
expect_width 50
snap narrow
