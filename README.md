# PantheonOS

*A suite of small terminal tools over one idea: your life, modeled as a directory tree you can read, edit, and reason about by hand — and so can an LLM, and so can a script.*

No database, no app. The ontology **is** the filesystem: a node of your life is a directory, its records are plain JSON files under it. Three hands — you, an LLM on your behalf, and deterministic code — act on the same files through the same grammar.

## Install

Install one tool or all of them; each ships as a standalone binary.

```sh
# one tool (example: Annales, the log core)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/LinusNyman/pantheon-os/releases/latest/download/annales-installer.sh | sh

# or via cargo
cargo install annales
```

## Quickstart

```sh
export PANTHEON_ROOT=~/pantheon      # the tree
ann ecv weight 78.4                  # log a reading; filed at ec_v_valetudo
alb csa                              # browse friends (bare short → TUI)
pan tree                             # emit the ontology as JSON
```

## The suite

| Kind | Tools |
|---|---|
| spine | `pan` (Pantheon) |
| cores | `alb` `map` `rat` `fas` `pen` `ann` `tab` |
| automation | `aus` (Auspex — the one reactive writer) |
| lenses | `spe` `atr` `stu` |

See [`docs/src/PANTHEONOS-SPEC.md`](docs/src/PANTHEONOS-SPEC.md) for the full specification and [`PORTICUS-SPEC.md`](docs/src/PORTICUS-SPEC.md) for the shared TUI chrome.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
