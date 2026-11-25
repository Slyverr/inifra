# Local slash commands render as cards, and /clear starts over.
start ask
wait_for "Type a message"
send -l "/help"; send Enter
wait_for "◆ Local"
expect "/clear"
expect "/effort"
snap help
send -l "/project"; send Enter
wait_for "Known files in the current project directory"
expect "Cargo.toml"
snap project
send -l "/profile"; send Enter
wait_for "mock"
snap profile
send -l "/templates"; send Enter
sleep 1
snap templates
send -l "/inspect no-such-template"; send Enter
wait_for "no-such-template" 10
snap inspect
send -l "/models"; send Enter
wait_for "mock-2" 10
snap models
send -l "/model mock-2"; send Enter
sleep 1
send -l "/effort high"; send Enter
sleep 1
snap effort
send -l "/plan no-such-template"; send Enter
sleep 1
send -l "/apply"; send Enter
sleep 1
snap apply
send -l "/clear"; send Enter
wait_for "Started a new conversation"
expect_not "◆ Local"
snap cleared
