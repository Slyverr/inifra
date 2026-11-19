# Assistant Profiles

Run `inifra assistant` to open the inline profile manager. It lets you create or edit profiles, switch the default, remove a profile, or reset assistant data. Press Esc to return to the shell.

The manager separates API selection from provider selection. For example, choose **Chat Completions API** and then **NVIDIA NIM** or **OpenRouter**. A profile stores the provider, API protocol, model ID, optional base URL, and the name of its API-key environment variable. It never stores a key in `assistant.toml`.

For Chat Completions, enter the API base URL (for example, `https://integrate.api.nvidia.com/v1`). Inifra appends `/chat/completions` while preserving path prefixes; it does not append the route twice if the full endpoint URL is entered. Custom endpoints omit optional streaming usage settings because support varies between compatible servers.

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

The session shows the active provider and model. `/models` asks the configured API for model IDs when that API supports listing; pressing Tab after `/model ` fetches IDs on demand and completes matching models. Use `/model ID` to switch models for the current session. `/help` lists the available commands:

- `/templates [query]` searches the loaded catalog.
- `/inspect ID` shows one template's outputs and inputs.
- `/project` reports recognized manifests and guidance files in the current directory.
- `/plan ID... [--set NAME=VALUE] [--overwrite]` prepares a generation plan using Inifra Core's validation.
- `/apply` opens a No-default confirmation. Files and directories are changed only after an explicit Yes, and the core revalidates the plan before applying it.

The assistant cannot browse the filesystem or apply changes on its own. Slash commands run locally. Their visible results are held for context and sent to the configured provider only with a later message you submit; the interface discloses this before sending. Submitted messages and prior conversation history are sent to the configured provider. Do not include secrets or other sensitive data unless you intend to share them with that provider.

Provider APIs may charge for requests. Inifra displays token usage only when the API reports it; it does not estimate monetary cost. Model listing may be unavailable for a compatible endpoint. Errors include available endpoint and response details while redacting configured credentials. Message, response, conversation, and local-result sizes are bounded by the application.
