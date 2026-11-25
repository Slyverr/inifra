# A streamed reply: waiting indicator, bullet, hanging indent, footer usage.
start ask
wait_for "Type a message"
expect "mock · Custom API · mock-1"
send -l "SLOW how does inifra work?"; send Enter
wait_for "Thinking"
expect "▎ SLOW how does inifra work?"
expect "Esc cancel"
snap waiting
wait_for "● Inifra reads the catalog" 20
wait_for "Nothing is written until you approve the plan." 20
expect "Fill the inputs"
wait_for "40 out" 5
wait_for "Ctrl+O switch" 5
snap answered
