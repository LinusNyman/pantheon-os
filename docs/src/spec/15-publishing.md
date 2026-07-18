## 15. Publishing workflow

Personal-first, published so others can use it: clean and installable, without heavy contributor governance. **Published means GitHub** — the source is public from the first commit (§14) and the Releases page is the registry; nothing goes to crates.io (§15.4).

### 15.1 License & hygiene
Dual **MIT / Apache-2.0**. `README.md` (one-line install + quickstart), `SECURITY.md`, short `CONTRIBUTING.md` ("issues welcome, PRs by discussion"). The edition, the MSRV, and the toolchain pinned at it are §14's files, not restated here. `dependabot.yml` for `cargo` + `github-actions`, **weekly** — `cargo-deny` already fails a PR on an advisory (§15.2), so dependabot carries version churn, not urgency, and churn is what a personal-first project wants batched.

### 15.2 CI (GitHub Actions, every PR)
`fmt` · `clippy` (pedantic, `-D warnings`) · `cargo-nextest` · `cargo-deny` · build matrix (linux x86_64/aarch64, macOS x86_64/aarch64, windows x86_64, each on its native runner) · `dist plan` — from the first binary, dist ignoring a package that defines no binary (§16) — which catches release breakage early.

Four things earn their absence. **No MSRV job** — `rust-toolchain.toml` pins the toolchain *at* the MSRV (§13–§14), so CI cannot compile a newer feature; the floor is the build, not a check beside it. **No `insta` stage** — `INSTA_UPDATE` defaults to `auto`, which reads `no` wherever `CI` is set, so a plain `cargo nextest run` already fails on a contract snapshot that moved or was never written (§7.2); the snapshots are tests, not a second gate. **No `cargo-audit`** — `cargo-deny`'s advisories check reads the same RustSec database (with `yanked = "deny"`, which it only warns on by default) and brings licences, bans, and sources with it; audit's binary scan has nothing here to scan. **No cross-compilation** — linux aarch64 has a native runner, free for public repos.

### 15.3 Releases
- **`release-plz`** opens **one Release PR for the workspace** — a version bump and changelog for each crate whose commits earned one; merge when ready. Independent cadence survives the single PR: a crate no commit touched takes no bump and joins no release.
- On merge, release-plz's release job **cuts and pushes the per-crate tags itself** (`album-v0.2.1`) — no hand pushes a tag. It runs **`git_only`**: versions are read off those tags rather than the crates.io index, and nothing is published to a registry. `git_tag_name` is spelled out rather than defaulted, its default deferring to a count of publishable packages that is now zero.
- **`dist`** on a version tag runs *plan → build → host → publish → announce*: prebuilt binaries plus **shell, PowerShell, and Homebrew installers**, attached to a GitHub Release with artifact attestations (`github-attestations`, off by default and turned on).

Flow: merge the Release PR → release-plz tags each app that moved → each tag fires `dist` for *that app alone* (a package-prefixed tag is what dist calls a singular announcement) → every platform, its own installers, its own formula. Nobody downloads the suite to get one tool.

### 15.4 Distribution channels
GitHub Releases *is* the registry: a per-app one-liner (`curl … | sh`), a PowerShell installer, a Homebrew tap (`brew install you/tap/album` — one formula per app, since one tag ships one app, §15.3), and for the Rust hand `cargo install --git … album --tag album-v0.2.1 --locked` and the same `--git` form under `cargo binstall`.

**The tap is a second repo, and Homebrew chooses that.** `brew tap you/tap` expands to `github.com/you/homebrew-tap` — the prefix is not optional — so the short form reaches a `homebrew-*` repo and nothing else, and the workspace (§14) can never be its own tap under it. The two-argument form takes any URL and Homebrew still reads a `HomebrewFormula/` dir, so the monorepo *could* serve; it shouldn't. A tap is cloned whole, with history, and re-fetched on every `brew update` — every user would carry the Rust source to get one binary — and `dist` writes `Formula/` with no config to aim it elsewhere. The tap is therefore a small second repo of nothing but formulas: release furniture beside the hygiene files (§14).

**The suite is one hand-written formula.** Twelve formulas is the default and the point (§15.3) — but nobody should type twelve lines to get all of it either, so the tap carries `pantheon`, whose body is `depends_on` on the other twelve: `brew install you/tap/pantheon` takes the suite, `you/tap/album` takes one tool. Homebrew has no meta-package concept and a formula "requires at least a URL", so this takes the shape the need has taken elsewhere (`aws/homebrew-tap`'s `k8s-tools`): a `file:///dev/null` url, the empty string's `sha256`, a hand-bumped `version`, and an `install` writing one stub file, since an empty one errors. `dist` generates the twelve and never this one, and it commits named files rather than force-pushing, so the hand-written formula survives every release. `homebrew/core` already holds a `pan` — the GNOME newsreader, which ships its own `bin/pan` — so that formula alone takes dist's `formula` override and lands as `pantheon-pan`; the binary is still `pan` (§7.3), and a user who also runs the newsreader gets the link conflict Homebrew reports, which is the price of a three-letter name rather than a reason to spend it.

**Why no registry.** crates.io buys exactly one thing — dropping `--git` from the two cargo forms — and charges twice for it: `cargo publish` refuses an app whose spine is an unpublished path dep, so the libs would publish too, under a semver number governing a Rust surface that is not the contract (I4, §15.5); and `pantheon`, `pan`, `auspex`, and `tessera` are already taken there, so four crates would rename to buy back a short form the rename spends.

**Two things the `--git` forms need.** `cargo binstall` guesses an artifact URL from the tag, and its guesses are `…/download/{version}/` and `…/download/v{version}/` — never the `…/download/album-v0.2.1/` a singular announcement writes — so each app carries a `[package.metadata.binstall]` `pkg-url` naming its own; `dist` writes none. And `--locked` is honoured but never enforced on a git source — Cargo's own docs claim it errors on a missing lockfile, and it does not — so the committed `Cargo.lock` (§14) is what makes the flag mean anything.

**The toolchain pin does not reach an installer.** `cargo install --git` builds under the *caller's* toolchain: rustup reads an override from the invoking directory, never from the checkout under `~/.cargo/git/`, so `rust-toolchain.toml` governs this repo's builds and no one else's. What holds the floor for an installing hand is `rust-version` (§13–§14), which refuses with *requires rustc 1.88.0 or newer* rather than building on whatever they have.

### 15.5 Versioning & compatibility

**Independent per-crate versions**; each app releases on its own cadence. Two separately-installed apps must still agree about the files they share, so exactly **two versions** exist:

| Version | Semantics | Where |
|---|---|---|
| **crate** (semver) | the app's own releases; its CLI JSON *is* its public API (I4), so semver governs the contract — `alb 2.x` means contract v2, and until 1.0 the minor carries the break | `alb version` |
| **format** | the on-disk record layer: naming triple, meta dir, envelope, and all three storage shapes — `.json`, `.jsonl`, a document's frontmatter (§6.1) — shared by the whole ecosystem | `alb version -f json` |

That second row is the format's **surface**: what a bump may move, and all it may move. A format bump is a breaking change for every app and gets a migration (`pan migrate`, §5.5); crate versions drift freely beneath it. `pan doctor` reports both and flags mismatches, reading them off each app's `version -f json` (§7.3) — a lens has no `schema` to read them from (§12) — while the token map it checks beside them comes off each core's `schema`, the same PATH discovery (§5.0, §5.5).

**Nothing stamps the tree**, and the migration needs no stamp. §18 leaves nowhere to write one — no dotfile, no root marker, and a node's meta file annotates rather than declares (§6.6) — so `pan migrate` is shape-directed and idempotent: it rewrites the old forms it meets and passes over the new, which makes it a thing you run rather than aim, and running it twice costs a walk. The constraint that falls out is the useful half — a format break must be **visible in the shape**, since a break a walk cannot see is one no migration can find. The tree is its own version as it is its own ontology (§5.0).

**Static linking only.** Every app links `pantheon` (and, if TUI-bearing, `porticus`) into its own binary; where linked versions differ, the format version keeps their files compatible. See §18.

### 15.6 Docs
`mdBook` in `docs/` → GitHub Pages: the ontology, the contract, one page per core. No `docs.rs` — nothing publishes to a registry to host it (§15.4), and the Rust surface it would document is not the contract (I4).
