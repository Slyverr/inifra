# The setup flow: known provider vs custom endpoint, then the matching list.
start assistant configure
wait_for "Known provider"
expect "Custom endpoint"
snap first
send Enter
wait_for "OpenAI"
expect "NVIDIA NIM"
expect "no key"
snap providers
send Escape
wait_for "Known provider"
send Down Enter
wait_for "Chat Completions"
expect "you set the URL"
snap apis
send Escape
wait_for "Known provider"
