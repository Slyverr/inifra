# A provider error is labeled "request failed", not "interrupted".
start ask
wait_for "Type a message"
send -l "FAIL please"; send Enter
wait_for "request failed" 20
expect_not "interrupted"
snap failed
