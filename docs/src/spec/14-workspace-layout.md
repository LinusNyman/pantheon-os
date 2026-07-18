## 14. Repository & workspace layout

A single public Cargo workspace (monorepo) with **independent per-crate versions and per-app releases** (§15.5). One repo because there is one hub: every app links `pantheon` (I5), and the spine's Rust surface is not what semver governs — an app's public API is its CLI JSON (§15.5) — so a spine change lands with every dependent in one commit, against one lockfile.

```
pantheon-os/
├── Cargo.toml                 # [workspace] — members; edition 2024 + MSRV; the dep versions §13 defers here
├── Cargo.lock                 # committed — every crate ships a binary, and `dist` builds `--locked`
├── rust-toolchain.toml        # the toolchain itself, pinned at the MSRV (§13)
├── dist-workspace.toml        # dist: per-app artifacts & installers (§15.3)
├── release-plz.toml           # per-crate versions, tags, changelogs (§15.3)
├── crates/
│   ├── pantheon/              # lib — spine: addressing, resolver, envelope, Core substrate, validation (linked by all; no UI deps)
│   ├── pan/                   # bin — ontology CLI (§5.5) + structural TUI (§10); porticus behind default tui feature
│   ├── porticus/              # lib — TUI chrome over ratatui (§11.1)
│   ├── tessera/               # lib — Tile trait + built-in tiles, over ratatui-core (peer of porticus, not a dependant)
│   ├── album/                 # lib + bin
│   ├── mappa/                 # lib + bin
│   ├── rationes/              # lib + bin
│   ├── fasti/                 # lib + bin
│   ├── pensum/                # lib + bin
│   ├── annales/               # lib + bin
│   ├── tabella/               # lib + bin
│   ├── auspex/                # lib + bin (no daemon — woken by hooks, §9.4)
│   ├── speculum/              # bin (lens)
│   ├── atrium/                # bin (lens)
│   └── studium/               # bin (lens)
├── xtask/                     # dev tasks (seed a demo tree)
└── docs/                      # mdBook (§15.6) — this spec beside PORTICUS-SPEC.md (§11.1)
```

The root carries the hygiene files too — `README.md`, both licences, `SECURITY.md`, `CONTRIBUTING.md`, `deny.toml`, `.github/` (CI, dependabot) — release furniture rather than workspace shape (§15.1–§15.2).

Each core is `lib + bin`: the lib does the work; the bin is a ~30-line `clap` shell that emits the contract — table or JSON by the TTY rule (§7.2–§7.3) — and, behind its default `tui` feature, hosts a Porticus app, linking `tessera` beside it where it drops in a tile of its own readings (§11.2). Every bin is **named by its three-char short** (`[[bin]] name = "alb"`, `path` spelled out, since a bin whose name isn't its crate's is not auto-discovered), ships standalone, versions independently.

`pan` and `aus` are the two system tools, each a crate over the spine with its TUI behind the default feature: `auspex` is `lib + bin` like a core, `pan` a bin alone — its work is already the spine's (§5.5, §10). `pantheon` stays lib-only for two reasons: everything links the spine, and nothing should drag the UI layer in with it (§5); and `porticus` points at `pantheon` (I5), so a `pan` bin *inside* the spine crate could not reach its own chrome without closing a dependency cycle Cargo refuses.

Lenses are thin bins over `porticus`/`tessera` that discover cores on `PATH`. `porticus` rides the default `tui` feature as everywhere else; `tessera` does not — the tiles are a lens's substance, not its chrome, so `--no-default-features` drops the screen and keeps the folds (§12). Nothing trails in behind it: a tile owns no backend (`ratatui-core`, §13), so a headless lens links no terminal. Any TUI-bearing bin builds `--no-default-features` for a headless install.
