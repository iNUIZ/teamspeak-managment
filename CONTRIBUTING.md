# Contributing

Thanks for helping improve Teamspeak Managment.

## Development rules

1. Never commit real server data, passwords, API keys, SSH keys, identities,
   cookie files, databases, or backups.
2. Keep `install.sh` compatible with Bash 4.3 or newer.
3. Keep `install.ps1` compatible with Windows PowerShell 5.1 and PowerShell 7.
4. Preserve clean-install behavior. Existing installations must never be
   imported implicitly.
5. Destructive actions must require explicit confirmation.
6. Update both README files when user-facing behavior changes.

## Local checks

```bash
bash -n install.sh
shellcheck install.sh
```

PowerShell:

```powershell
$tokens = $null
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile(
  (Resolve-Path .\install.ps1), [ref]$tokens, [ref]$errors
) | Out-Null
$errors
```

Open a pull request with a short explanation, the platforms tested, and any
relevant installer output with secrets removed.
