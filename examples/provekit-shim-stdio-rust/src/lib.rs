// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-stdio-rust: std::io's @sugar shim.
//
// Realizes stdio line-stream concepts via the Rust standard library.
// Sister shims in other languages wrap their native stdio:
//   - TS: provekit-shim-stdio-typescript wraps node:readline + process.stdout
//   - Python: provekit-shim-stdio-python wraps sys.stdin / sys.stdout
//   - Java: provekit-shim-stdio-java wraps System.in / System.out
// All anchor to `boundary:stdio-line-stream`.

use std::io::{self, BufRead, Write};

/// `concept:stdio-read-line` — std::io's sugar. Reads one line from
/// stdin (without the trailing newline). Returns `None` at EOF.
#[provekit::sugar(
    concept = "concept:stdio-read-line",
    library = "std::io",
    version = "rust-1",
    family = "concept:family:stdio-stream",
    loss = [],
)]
pub fn stdin_read_line() -> Option<String> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();
    match handle.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Some(line)
        }
        Err(_) => None,
    }
}

/// `concept:stdio-write-line` — std::io's sugar. Writes one line +
/// newline to stdout, then flushes.
#[provekit::sugar(
    concept = "concept:stdio-write-line",
    library = "std::io",
    version = "rust-1",
    family = "concept:family:stdio-stream",
    loss = [],
)]
pub fn stdout_write_line(line: &str) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{}", line);
    let _ = handle.flush();
}

/// `concept:stderr-write-line` — std::io's sugar. Writes one line +
/// newline to stderr (used for human-readable progress messages that
/// the JSON-RPC client doesn't parse).
#[provekit::sugar(
    concept = "concept:stderr-write-line",
    library = "std::io",
    version = "rust-1",
    family = "concept:family:stdio-stream",
    loss = [],
)]
pub fn stderr_write_line(line: &str) {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "{}", line);
}

#[cfg(test)]
mod tests {
    #[test]
    fn shim_compiles() {
        // The stdio functions can't be unit-tested directly without
        // injecting stdin/stdout/stderr handles; existence is the
        // contract. Substantive testing happens via the cross-platform
        // crate's runtime against a real process.
    }
}
