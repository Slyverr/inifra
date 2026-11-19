# Shell completions

Generate a completion script for Bash, Elvish, Fish, PowerShell, or Zsh with:

```sh
inifra completions fish
```

Use `--install` to write it to the shell's user completion directory, and `--uninstall` to remove only the marked Inifra file:

```sh
inifra completions fish --install
inifra completions fish --uninstall
```

Installation never edits shell startup files. Fish loads the installed file automatically from its user completions directory. Bash can load its file through `bash-completion`; otherwise, source the printed path from `~/.bashrc`. For Zsh, add the printed `fpath` entry before `compinit` in `~/.zshrc`. For Elvish, add `use inifra` to its interactive rc file. For PowerShell, dot-source the printed script path from the profile. The install command prints the activation step when one is needed, and uninstall reminds you to remove any startup line you added yourself.

Install paths honor `XDG_CONFIG_HOME` and `XDG_DATA_HOME`. Bash, Elvish, Fish, and Zsh fall back to `~/.config` and `~/.local/share`; Windows PowerShell uses the platform's user config and data directories.

The completion script uses Inifra's dynamic completion support for commands, options, template IDs, preset IDs, and declared input names. Those lookups read the current user catalog when completion is requested. Static script generation and installation do not need to load the catalog. Completion files are written atomically; an unmanaged file or symlink at the target is left untouched and reported as an error.
