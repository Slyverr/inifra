# Assistant Profiles

A profile is a saved connection: a provider, an API protocol, an endpoint, and a credential reference, plus a default model and an optional default reasoning effort. An `inifra ask` session can change the model and effort without editing the profile.

Run `inifra assistant` to open the inline profile manager. Its home screen lists saved profiles with their provider, protocol, and model, and marks the default. Actions are Create a profile, Edit a profile, Change default profile, Remove a profile, and Reset assistant data; actions that need saved profiles or data appear only when they exist. Press Esc to return to the shell.

Profile setup first asks whether to use a known provider or a custom endpoint. A known provider applies its preset protocol and endpoint (OpenAI defaults to Responses) and each row shows its host and whether it needs a key. A custom endpoint asks you to pick a supported API protocol and then enter the base URL. A custom endpoint must implement the chosen protocol's request and response schema; the protocol adapter already knows where to read the assistant's text.

The profile form then asks for a Profile name (a short name for the connection, such as `work` or `local`; letters, numbers, and hyphens, starting with a letter; it is saved in lowercase, so `Work` and `work` are the same name), checked when you press Enter, the Default model, an optional Default effort (`low`, `medium`, or `high`; leave it empty for the provider default), and, for a custom endpoint, its base URL. The form lists every field in order: finished fields show a check and their value, the active field is the input with its guidance underneath, and upcoming fields wait in gray. A separate API key step follows, with one question: how does Inifra get the key? Paste an API key (the default) stores it in the user-level `credentials.env` under the provider's usual variable name, such as `OPENAI_API_KEY`, or `<PROFILE>_API_KEY` for a custom endpoint. Use an environment variable reads the key from the process environment and shows whether the variable is set. Use a different variable lets you choose the variable name, and Keep the stored key appears when one is already saved. Servers that need no key, such as local Ollama, also offer No API key. After saving, Inifra reports the saved connection and the default profile. A profile stores the provider, API protocol, model ID, optional effort, optional base URL, and credential reference (the variable name). It never stores a key in `assistant.toml`.

For a custom API, enter its base URL; Inifra adds the protocol's API path while preserving path prefixes. For example, Chat Completions adds `/chat/completions` and does not duplicate the route if it is already present. Custom Chat Completions requests omit optional streaming usage settings because support varies between compatible servers. The picker supports arrow keys or `j`/`k` to move, `h`/`l` to switch sides, `/` to filter, and Enter to choose. Esc clears an active filter first, then returns one step. Esc backs out of nested screens; Ctrl+C quits everywhere, and `q` quits non-text screens while remaining ordinary text in fields, filters, and chat input.

## Editing keys

Every text input, in the profile form and in the `inifra ask` composer, uses one readline-style keymap. A terminal program receives key events, not your shell's own bindings, so these are built in:

| Keys | Action |
| --- | --- |
| ←/→ | Move one character |
| Ctrl+←/→, Alt+←/→, Alt+B/F | Move one word |
| Home or Ctrl+A, End or Ctrl+E | Start or end of the line |
| Backspace, Delete | Delete one character |
| Ctrl+W | Delete the whitespace-separated word before the cursor |
| Alt+Backspace, Ctrl+Backspace | Delete the word before the cursor |
| Alt+D, Ctrl+Delete | Delete the word after the cursor |
| Ctrl+U, Ctrl+K | Delete to the start or end of the line |

Some terminals send Ctrl+←/→ as another sequence; Alt+B and Alt+F always work.

## Commands

```sh
inifra assistant                 # Open the profile manager
inifra assistant profiles        # List profiles and the default
inifra assistant configure [ID]  # Create or edit a profile
inifra assistant use ID          # Change the default profile
inifra assistant remove ID       # Confirm and remove one profile
inifra assistant reset           # Confirm and remove all assistant data
```

Removing one profile leaves saved credential entries in place because profiles may share them. Reset removes only `assistant.toml` and `credentials.env` from the Inifra configuration directory; it does not remove template packs, presets, or other files.

## Credentials

Profiles can read a key from the process environment or from the user-level `credentials.env`. When both are configured for a profile, a nonempty environment value takes precedence. The configuration flow masks keys while they are entered. Credential values are stored literally and never evaluated as shell code; on Unix, the file is created with owner-only permissions.

The default configuration directory follows the platform's user configuration location. Set `INIFRA_CONFIG_DIR` to use a different directory. Do not commit `credentials.env` or share it with a project catalog.

## Ask

After configuring a profile, run an inline assistant session in the current project:

```sh
inifra ask
inifra ask --profile local
```

The footer shows the active profile, provider, model, effort when set, and token usage when the API reports it. While a request runs it also shows `Esc cancel`. `/profile` lists saved connections and `/profile ID` switches to one; switching clears conversation and queued local context without changing the saved default. `/models` asks the active API for model IDs when it supports listing; pressing Tab after `/model ` fetches IDs on demand and completes matching models. Use `/model ID` to switch models for the current session, and `/effort [low|medium|high|default]` to show or set the reasoning effort. `/help` lists the available commands:

- `/templates [query]` searches the loaded catalog.
- `/inspect ID` shows one template's outputs and inputs.
- `/project` reports recognized manifests and guidance files in the current directory.
- `/plan ID... [--set NAME=VALUE] [--overwrite]` prepares a generation plan using Inifra Core's validation.
- `/clear` starts a new conversation: it clears the visible conversation, queued local results, and any unapproved plan, and keeps the profile, model, and effort.
- `/apply` opens a No-default confirmation. Files and directories are changed only after an explicit Yes, and the core revalidates the plan before applying it.

`/model` without an ID, or Ctrl+O while idle, opens the switcher: profiles on the left and the highlighted profile's model IDs on the right. Use ↑/↓ or `j`/`k` to move, Tab or `h`/`l` to switch sides, `e` to cycle the effort, Enter to use the selection for this session, `s` to also save it as the default profile with its model and effort in `assistant.toml`, and Esc to close. Switching profile or model clears the conversation and queued local context; changing only the effort does not.

The assistant cannot browse the filesystem or apply changes on its own. Slash commands run locally. Their visible results are held for context and sent to the configured provider only with a later message you submit; the interface discloses this before sending. Submitted messages and prior conversation history are sent to the configured provider. Do not include secrets or other sensitive data unless you intend to share them with that provider.

Provider APIs may charge for requests. Inifra displays token usage only when the API reports it; it does not estimate monetary cost. Model listing may be unavailable for a compatible endpoint. Errors include available endpoint and response details while redacting configured credentials. Message, response, conversation, and local-result sizes are bounded by the application.

### Reasoning effort

Effort is `low`, `medium`, or `high`. Set a default in the profile, or change it for a session with `/effort` or the switcher; `/effort default` returns to the provider default. When no effort is set, nothing is sent. Each protocol maps it differently:

- Responses sends `reasoning.effort`.
- Chat Completions sends `reasoning_effort`.
- Anthropic Messages enables extended thinking with a token budget (2,048, 8,192, or 24,576 tokens) and raises `max_tokens` so the answer still fits.
- Gemini Generate Content sends `thinkingConfig.thinkingBudget` with the same budgets.
- Ollama Chat sends `think: true`; the level itself is not sent.

Models without reasoning support may reject the setting. If a request fails for that reason, switch with `/effort default`. A profile saved with an effort cannot be read by Inifra 0.8.0 or earlier; remove the `effort` line from `assistant.toml` before downgrading.

### Errors

A failed request shows a one-line summary, a suggested command when one applies, and a muted `details:` line with the provider's message and request ID. A response labeled "request failed" ended with an error; "interrupted" means you cancelled it. Connection failures report their cause, such as a refused connection.
