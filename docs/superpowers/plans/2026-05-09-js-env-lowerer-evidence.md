# JavaScript Env Lowerer Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote the Supply Chain Rails JavaScript lowerer from token scanning to an AST-backed `runtime.no-env-secret-read` evidence/refusal engine with content-addressed receipts.

**Architecture:** Keep the lowerer as the exhibit-local Rust JSON-RPC kit, but split JavaScript analysis into explicit parse, scan, and receipt steps. Use a real JavaScript parser, fail closed on parse/unsupported dynamic surfaces, and preserve ProvekIt's `lower -> witness/refusal -> .proof` flow.

**Tech Stack:** Rust 2021, `tree-sitter`, `tree-sitter-javascript`, `serde_json`, existing ProvekIt lower JSON-RPC protocol, existing Supply Chain Rails smoke tests.

---

### Task 1: Add Red Tests For Env Receipts

**Files:**
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/supply-chain-js-lowerer.rs`
- Test: `menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/supply-chain-js-lowerer.rs`

- [ ] **Step 1: Write failing unit tests in the lowerer file**

Add a `#[cfg(test)] mod tests` with helpers that call a pure `analyze_runtime_no_env_secret_read("index.js", source)` function. The tests must assert:

```rust
#[test]
fn runtime_no_env_accepts_source_without_env_reads() {
    let result = analyze_runtime_no_env_secret_read(
        "index.js",
        "export function parseJson(input) { return JSON.parse(input); }\n",
    )
    .expect("analysis succeeds");
    assert!(result.findings.is_empty());
    assert!(result.unsupported.is_empty());
}

#[test]
fn runtime_no_env_rejects_direct_process_env_read_with_span() {
    let result = analyze_runtime_no_env_secret_read(
        "index.js",
        "export function parseJson(input) {\n  return process.env.SAFE_JSON_TOKEN || input;\n}\n",
    )
    .expect("analysis succeeds");
    assert_eq!(result.findings[0].reason_code, "env-secret-read");
    assert_eq!(result.findings[0].expression, "process.env.SAFE_JSON_TOKEN");
    assert_eq!(result.findings[0].span.line_start, 2);
}

#[test]
fn runtime_no_env_rejects_aliased_env_read() {
    let result = analyze_runtime_no_env_secret_read(
        "index.js",
        "const env = process.env;\nexport function parseJson(input) { return env.SAFE_JSON_TOKEN || input; }\n",
    )
    .expect("analysis succeeds");
    assert_eq!(result.findings[0].reason_code, "env-secret-read");
    assert_eq!(result.findings[0].expression, "env.SAFE_JSON_TOKEN");
}

#[test]
fn runtime_no_env_fails_closed_on_dynamic_process_env_read() {
    let result = analyze_runtime_no_env_secret_read(
        "index.js",
        "const key = 'env';\nexport function parseJson(input) { return process[key].SAFE_JSON_TOKEN || input; }\n",
    )
    .expect("analysis succeeds");
    assert_eq!(result.unsupported[0].reason_code, "dynamic-env-access");
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run: `cargo test --manifest-path menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml runtime_no_env -- --nocapture`

Expected: compile failure because `analyze_runtime_no_env_secret_read` and the analysis structs do not exist.

### Task 2: Implement AST Analysis

**Files:**
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml`
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/supply-chain-js-lowerer.rs`

- [ ] **Step 1: Add parser dependencies**

Add to the kit RPC manifest:

```toml
tree-sitter = "0.24"
tree-sitter-javascript = "0.23"
```

- [ ] **Step 2: Implement analysis data structures**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSpan {
    line_start: usize,
    column_start: usize,
    line_end: usize,
    column_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvFinding {
    reason_code: &'static str,
    expression: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnsupportedSurface {
    reason_code: &'static str,
    expression: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeNoEnvAnalysis {
    findings: Vec<EnvFinding>,
    unsupported: Vec<UnsupportedSurface>,
}
```

- [ ] **Step 3: Parse and walk JavaScript AST**

Implement `analyze_runtime_no_env_secret_read(artifact, source)` using `tree_sitter::Parser`, `tree_sitter_javascript::LANGUAGE`, and a node walk. The walker must:

```rust
// direct read
process.env
process.env.SAFE_JSON_TOKEN

// alias declaration
const env = process.env
let env = process.env
var env = process.env

// alias read
env.SAFE_JSON_TOKEN

// fail-closed dynamic surfaces
process["env"]
process[key]
require(...)
import(...)
```

- [ ] **Step 4: Run focused tests and verify they pass**

Run: `cargo test --manifest-path menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml runtime_no_env -- --nocapture`

Expected: all `runtime_no_env_*` tests pass.

### Task 3: Emit Receipt Fields From Analysis

**Files:**
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/supply-chain-js-lowerer.rs`
- Test: `menagerie/supply-chain-rails/tests/smoke.rs`

- [ ] **Step 1: Add failing CLI-level smoke assertions**

Extend the existing Supply Chain Rails smoke test to assert the lower result exposes:

```rust
assert_eq!(
    report["redRails"]["witness"]["reasonCode"],
    "env-secret-read"
);
assert!(report["redRails"]["witness"]["evidenceCid"]
    .as_str()
    .expect("evidenceCid")
    .starts_with("blake3-512:"));
assert_eq!(
    report["redRails"]["witness"]["unsupportedSemantics"],
    serde_json::json!([])
);
```

- [ ] **Step 2: Run the smoke test and verify it fails**

Run: `cargo test --release --manifest-path implementations/rust/Cargo.toml -p provekit-supply-chain-rails --test smoke all_exhibits_show_conventional_green_then_provekit_red -- --nocapture`

Expected: failure because `redRails.witness.evidenceCid` or `unsupportedSemantics` is not surfaced yet.

- [ ] **Step 3: Wire analysis into `realize`**

For `runtime.no-env-secret-read`, replace token scanning with AST analysis and emit evidence containing:

```json
{
  "kind": "javascript-runtime-no-env-evidence",
  "contract": "runtime.no-env-secret-read",
  "contractCid": "<cid over proofIr>",
  "artifact": "index.js",
  "artifactCid": "<cid over source bytes>",
  "lowerer": {"name":"supply-chain-js-lowerer","version":"0.2.0"},
  "mode": "witness",
  "findings": [],
  "unsupportedSemantics": [],
  "sourceSpans": []
}
```

Rejected output must include `reasonCode`, `evidenceCid`, `findings`, `unsupportedSemantics`, and `sourceSpans`.

- [ ] **Step 4: Run focused smoke and lowerer tests**

Run:

```bash
cargo test --manifest-path menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml runtime_no_env -- --nocapture
cargo test --release --manifest-path implementations/rust/Cargo.toml -p provekit-supply-chain-rails --test smoke all_exhibits_show_conventional_green_then_provekit_red -- --nocapture
```

Expected: both pass.

### Task 4: Update Walkthrough Receipts

**Files:**
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/walkthrough/04-preserve-contracts-fail-witness.sh`
- Modify: `menagerie/supply-chain-rails/authenticated-betrayal/walkthrough/README.md`
- Modify: `menagerie/supply-chain-rails/README.md`

- [ ] **Step 1: Highlight raw evidence receipt lines**

Update `04-preserve-contracts-fail-witness.sh` to highlight:

```bash
highlight_raw_line "$lower_json" '"evidenceCid":' "The refusal evidence is content-addressed."
highlight_raw_line "$lower_json" '"sourceSpans":' "The lowerer points at the source span that contradicts the preserved contract."
highlight_raw_line "$lower_json" '"unsupportedSemantics": []' "The failure is a concrete counterexample, not an unsupported-language escape hatch."
```

- [ ] **Step 2: Update honesty language**

Change docs from “toy-shaped JS lowerer” to “AST-backed for `runtime.no-env-secret-read`, still limited to the documented coverage envelope.”

- [ ] **Step 3: Run script syntax checks**

Run: `bash -n menagerie/supply-chain-rails/authenticated-betrayal/walkthrough/04-preserve-contracts-fail-witness.sh`

Expected: exit 0.

### Task 5: Final Verification And PR

**Files:**
- Modify: `.provekit/ci/accepted/**` only if `provekit ci accept --check` reports a stale checked-in witness.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --manifest-path menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml --check
cargo fmt --manifest-path implementations/rust/Cargo.toml --package provekit-supply-chain-rails --check
```

- [ ] **Step 2: Run targeted tests**

Run:

```bash
cargo test --manifest-path menagerie/supply-chain-rails/authenticated-betrayal/kit-rpc/Cargo.toml runtime_no_env -- --nocapture
cargo test --release --manifest-path implementations/rust/Cargo.toml -p provekit-supply-chain-rails --test smoke -- --nocapture
```

- [ ] **Step 3: Run checked-in CICP gate**

Run: `implementations/rust/target/release/provekit ci accept --all-kits --clean --check --out .provekit/ci/accepted`

Expected: verifies existing accepted witnesses, or reports the exact missing witness to refresh.

- [ ] **Step 4: Run diff hygiene**

Run: `git diff --check`

- [ ] **Step 5: Commit and PR**

Commit message:

```bash
git commit -m "Promote JS env lowerer receipts"
```

Open a non-draft PR and cite #499.

