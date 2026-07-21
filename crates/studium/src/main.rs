//! `stu` — the binary. §14's "~30-line clap shell", and nothing else.
//!
//! Everything — the whole `Cli`, the folds, and the screen — lives in the lib
//! (`src/cli.rs`, `src/fold.rs`, `src/screen.rs`), because an integration test links the
//! lib and a screen in the bin is a screen no test can reach.

use std::process::ExitCode;

fn main() -> ExitCode {
    studium::run_cli()
}
