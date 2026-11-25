# Esc during a slow response cancels it.
start ask
wait_for "Type a message"
send -l "SLOW cancel me"; send Enter
wait_for "Esc cancel"
send Escape
wait_for "interrupted" 10
wait_for "Type a message" 10
snap cancelled
