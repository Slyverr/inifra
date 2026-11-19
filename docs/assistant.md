# Assistant Profiles

Run `inifra assistant` to open the inline profile manager. It lets you create or edit profiles, switch the default, remove a profile, or reset assistant data. Press Esc to return to the shell.

The manager separates API selection from provider selection. For example, choose **Chat Completions API** and then **NVIDIA NIM** or **OpenRouter**. A profile stores the provider, API protocol, model ID, optional base URL, and the name of its API-key environment variable. It never stores a key in `assistant.toml`.

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
