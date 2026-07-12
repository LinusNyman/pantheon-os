# Contributing

Personal-first, published so others can use it — light on governance.

- **Issues welcome.** Bugs, ideas, questions all fine.
- **PRs by discussion.** Open an issue first so we agree on shape before you write code.
- **The invariants (§3) are law.** Every change is downstream of them; a PR that violates one won't land. The non-goals (§18) are fenced off deliberately — don't add them.
- **The contract is the JSON.** Core CLI output is snapshot-tested with `insta`; a change to a core's JSON must show as a reviewed diff.

Before pushing: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.
