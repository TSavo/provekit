// Minimal echo RPC server. Demonstrates substrate-honest lift→lower:
// rust source uses ONLY constructs whose ProofIR-canonical form the
// substrate's java lower vocabulary can emit today (concept:while,
// concept:if, concept:assign, concept:call, concept:return + the 9
// boundary primitives). Each @sugar function realizes one concept;
// each call goes through a @boundary primitive whose target shim
// provides the body.

use serde_json::Value;

// ---- Boundary primitives (filled by target shims) ------------------

#[sugar::boundary(
    concept = "concept:stdio-read-line",
    library = "sugar-shim-stdio-rust",
    family = "concept:family:stdio-stream",
    version = "rust-1",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stdin_read_line() -> Option<String> {
    unimplemented!("materialize-fillable boundary")
}

#[sugar::boundary(
    concept = "concept:stdio-write-line",
    library = "sugar-shim-stdio-rust",
    family = "concept:family:stdio-stream",
    version = "rust-1",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stdout_write_line(line: &str) {
    unimplemented!("materialize-fillable boundary")
}

// ---- @sugar realizations (lift to ProofIR, lower to java) ---------

/// `concept:rpc-minimal-echo-line` — read one line from stdin, write it
/// back to stdout. The boundary contract for stdin_read_line uses
/// Option<String> in rust but the concept-hub sort is concept:String
/// (with null-=-None as the cross-language loss morphism). At java
/// emission the unwrap is implicit (you already have the String).
#[sugar::sugar(
    concept = "concept:rpc-minimal-echo-line",
    library = "sugar-rpc-minimal",
    loss = [],
)]
pub fn echo_one_line() {
    let line = stdin_read_line_required();
    stdout_write_line(&line);
}

/// Helper boundary: read a line, panicking on EOF. Java realization
/// is the same as stdin_read_line (returns String directly).
#[sugar::boundary(
    concept = "concept:stdio-read-line",
    library = "sugar-shim-stdio-rust",
    family = "concept:family:stdio-stream",
    version = "rust-1",
    boundary_contract = "boundary:stdio-line-stream",
    loss = [],
)]
pub fn stdin_read_line_required() -> String {
    unimplemented!("materialize-fillable boundary")
}

/// `concept:rpc-minimal-three-line-echo` — read three lines, echo each.
/// No mutability, no loops, no destructuring. Just sequential calls.
#[sugar::sugar(
    concept = "concept:rpc-minimal-three-line-echo",
    library = "sugar-rpc-minimal",
    loss = [],
)]
pub fn three_line_echo() {
    let line1 = stdin_read_line_required();
    stdout_write_line(&line1);
    let line2 = stdin_read_line_required();
    stdout_write_line(&line2);
    let line3 = stdin_read_line_required();
    stdout_write_line(&line3);
}
