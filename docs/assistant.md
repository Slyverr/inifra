# Assistant Profiles

Run `inifra assistant` to open the inline profile manager. It lets you create or edit profiles, switch the default, remove a profile, or reset assistant data. Press Esc to return to the shell.

Profile setup starts with two independent choices, before asking for a profile ID. The left pane lists ready-to-use providers; choosing one applies its preset protocol and endpoint, then asks for the model and credential details. OpenAI defaults to Responses. The right pane lists supported API protocols; choosing one configures a custom endpoint and asks for its base URL, model, and optional API-key environment variable. A custom endpoint must implement the chosen protocol's request and response schema; the protocol adapter already knows where to read the assistant's text. Both routes use the same connection-template model, profile form, and protocol adapters; provider presets supply defaults rather than a second setup implementation. A profile stores the provider, API protocol, model ID, optional base URL, and credential reference. It never stores a key in `assistant.toml`.

For a custom API, enter its base URL; Inifra adds the protocol's API path while preserving path prefixes. For example, Chat Completions adds `/chat/completions` and does not duplicate the route if it is already present. Custom Chat Completions requests omit optional streaming usage settings because support varies between compatible servers. The picker supports arrow keys or `j`/`k` to move, `h`/`l` to switch sides, `/` to filter, and Enter to choose. Esc clears an active filter first, then returns one step. Esc backs out of nested screens; Ctrl+C quits everywhere, and `q` quits non-text screens while remaining ordinary text in fields, filters, and chat input.

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

The session shows the active provider and model. `/profile` lists saved connections and `/profile ID` switches to one; switching clears conversation and queued local context without changing the saved default. `/models` asks the active API for model IDs when it supports listing; pressing Tab after `/model ` fetches IDs on demand and completes matching models. Use `/model ID` to switch models for the current session. `/help` lists the available commands:

- `/templates [query]` searches the loaded catalog.
- `/inspect ID` shows one template's outputs and inputs.
- `/project` reports recognized manifests and guidance files in the current directory.
- `/plan ID... [--set NAME=VALUE] [--overwrite]` prepares a generation plan using Inifra Core's validation.
- `/apply` opens a No-default confirmation. Files and directories are changed only after an explicit Yes, and the core revalidates the plan before applying it.

The assistant cannot browse the filesystem or apply changes on its own. Slash commands run locally. Their visible results are held for context and sent to the configured provider only with a later message you submit; the interface discloses this before sending. Submitted messages and prior conversation history are sent to the configured provider. Do not include secrets or other sensitive data unless you intend to share them with that provider.

Provider APIs may charge for requests. Inifra displays token usage only when the API reports it; it does not estimate monetary cost. Model listing may be unavailable for a compatible endpoint. Errors include available endpoint and response details while redacting configured credentials. Message, response, conversation, and local-result sizes are bounded by the application.
