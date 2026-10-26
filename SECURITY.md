# Security policy

## Reporting a vulnerability

Please report security problems privately, not in a public issue: use GitHub's private vulnerability reporting at
<https://github.com/bullterrier-studio/bullterrier-control/security/advisories/new> ("Report a vulnerability" on the repository's
Security tab). Say what is affected, how to reproduce it (a small file helps), and what an attacker could do with it.

You can expect an acknowledgement within a week. Once a fix is released, the advisory is published with credit to you,
unless you prefer otherwise.

## Supported versions

bullterrier-control is before 1.0: only the latest release receives security fixes.

| Version | Supported |
| --- | --- |
| latest 0.x release | yes |
| older releases | no |

## What bullterrier-control does to stay safe

- **Audited code and dependencies.** ruff's security rules (flake8-bandit) are part of every `poe check`, and
  `uv run poe audit` checks every release pinned in `uv.lock` with pip-audit. No known advisory is open; one that has no
  fixed release yet would be listed here by its id.
- **No runtime dependency yet, and numpy only when the first signal arrives**, so there is little to audit and nothing that downloads or executes at import.
- **Plugins** are Python packages: installing one runs its code, as with any package.
