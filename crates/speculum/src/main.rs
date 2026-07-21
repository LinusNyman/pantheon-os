//! `spe` — the binary. §14's "~30-line clap shell", and nothing else.
//!
//! Everything — the whole `Cli`, the verbs, the mosaic, and the horizon — lives in the
//! lib (`src/cli.rs`, `src/screen.rs`, `src/mosaic.rs`, `src/horizon.rs`), because an
//! integration test links the lib and a screen in the bin is a screen no test can
//! reach. The shell stays this thin on purpose: there is nothing here to test.

use std::process::ExitCode;

fn main() -> ExitCode {
    speculum::run_cli()
}
