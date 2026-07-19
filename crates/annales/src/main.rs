//! `ann` — the binary. §14's "~30-line clap shell", and nothing else.
//!
//! Every verb, the whole `Cli`, and the screen live in the lib (`src/cli.rs`,
//! `src/screen.rs`), because an integration test links the lib and a screen in the bin
//! is a screen no test can reach — and step 6 found three defects that only driving a
//! screen caught.
//!
//! The shell stays this thin on purpose: there is nothing here to test, so there is
//! nothing here a test cannot reach.

use std::process::ExitCode;

fn main() -> ExitCode {
    annales::run_cli()
}
