# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately via GitHub's **Report a vulnerability** (Security Advisories), not a public issue. Expect an initial response within a week.

## Scope note

Auspex runs rule files — arbitrary code — only after they are blessed through `aus trust` (TTY-only, §9.6). The trust ledger (`pantheon_trust.toml`) defends against *unreviewed code running inadvertently*. It does **not** defend against a hostile agent that already has full shell access to `$PANTHEON_ROOT`; no in-tree mechanism can. Treat the tree as you treat your dotfiles.
