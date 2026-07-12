//! Auspex — the rules engine (§9). Reads the cores' readings for signs and turns
//! them into intentions (Pensum tasks); it proposes deeds, never does them. The
//! only reactive writer (I2). Rules are files discovered by walking the tree
//! (§9.1); a rule is a pure function — context on stdin, proposals on stdout
//! (§9.3). Trust via SHA-256 in `pantheon_trust.toml`, TTY-only blessing (§9.6).
//! No daemon — woken by hooks (§9.4). Scaffold.
