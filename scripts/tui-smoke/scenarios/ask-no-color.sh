# NO_COLOR removes colors but keeps every marker.
NO_COLOR=1
start ask
wait_for "Type a message"
send -l "hello"; send Enter
wait_for "● Inifra reads" 20
expect "▎ hello"
snap plain
if grep -q $'\033\\[[34]8;' "$OUT/$SCENARIO-plain.ansi"; then fail "color escapes with NO_COLOR"; fi
