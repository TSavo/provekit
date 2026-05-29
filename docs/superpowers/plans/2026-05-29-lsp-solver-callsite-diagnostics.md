# LSP Solver Callsite Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make solver/verifier failures from the Rust daemon path publish LSP diagnostics at the broken contract callsite instead of file start.

**Architecture:** Keep language analysis inside the owning kit. The Rust CLI/LSP/linkerd path remains language-agnostic infrastructure that routes kit facts, runs solver obligations, and projects verifier results into editor diagnostics. The vertical slice preserves `callSiteLocus` from Rust kit call edges through `LinkerError`, linkerd JSON, and LSP range conversion.

**Tech Stack:** Rust workspace crates `provekit-linker`, `provekit-linkerd`, `provekit-lsp`; `serde_json`; `tower_lsp`; existing verifier `StubSolver` test utilities.

---

## File Structure

- Modify `implementations/rust/provekit-linker/src/lib.rs`
  - Add `call_site_locus_json: Option<Json>` to `LinkerError`.
  - Populate it from `LinkerCallEdge.call_site_locus_json` for unresolved symbols and solver failures.
- Modify `implementations/rust/provekit-linker/tests/discharge_obligation.rs`
  - Prove unsatisfied solver obligations preserve the original callsite locus.
- Modify `implementations/rust/provekit-linkerd/src/methods.rs`
  - Include `callSiteLocus` in `parseFile` diagnostic JSON.
- Modify `implementations/rust/provekit-linkerd/src/state.rs`
  - Include `callSiteLocus` in cached `getDiagnostics` JSON.
  - Add a focused state test using an unresolved call edge to prove cached diagnostics preserve the locus.
- Modify `implementations/rust/provekit-lsp/src/main.rs`
  - Accept `callSiteLocus` on daemon diagnostics.
  - Convert 1-based line / 0-based column source loci to 0-based LSP ranges.
  - Map solver failure kinds to stable `provekit.lsp.*` diagnostic codes.
- Modify `implementations/rust/provekit-lsp/tests/daemon_routed.rs`
  - Add or extend coverage proving published daemon diagnostics use the daemon-provided callsite range.

### Task 1: Preserve Callsite Locus In Linker Errors

**Files:**
- Modify: `implementations/rust/provekit-linker/src/lib.rs`
- Test: `implementations/rust/provekit-linker/tests/discharge_obligation.rs`

- [ ] **Step 1: Write the failing linker test**

Add this assertion to `logically_incompatible_emits_implication_unprovable` after the existing `let err = &out.linker_errors[0];` block, or create the `err` binding if absent:

```rust
    assert_eq!(
        err.call_site_locus_json.as_ref(),
        Some(&json!({
            "file": "caller.rs",
            "line": 1,
            "column": 1
        })),
        "solver failure must preserve the callsite locus for LSP diagnostics"
    );
```

- [ ] **Step 2: Run the failing linker test**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linker --test discharge_obligation logically_incompatible_emits_implication_unprovable
```

Expected: FAIL to compile because `LinkerError` has no `call_site_locus_json` field.

- [ ] **Step 3: Add the minimal linker field and propagation**

In `implementations/rust/provekit-linker/src/lib.rs`, change `LinkerError`:

```rust
pub struct LinkerError {
    pub kind: String,
    pub target_symbol: String,
    pub source_contract_cid: String,
    pub reason: String,
    pub file: Option<String>,
    pub call_site_locus_json: Option<Json>,
}
```

In the unresolved-symbol construction inside `derive_link_bundle_inner`, add:

```rust
call_site_locus_json: Some(edge.call_site_locus_json.clone()),
```

After `discharge_obligation(...)` returns `Some(mut err)`, set:

```rust
err.file = locus_file;
err.call_site_locus_json = Some(edge.call_site_locus_json.clone());
```

Inside `discharge_obligation`, add `call_site_locus_json: None` to every `LinkerError` literal.

- [ ] **Step 4: Run the linker test**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linker --test discharge_obligation logically_incompatible_emits_implication_unprovable
```

Expected: PASS.

- [ ] **Step 5: Commit the linker slice**

```bash
git add implementations/rust/provekit-linker/src/lib.rs implementations/rust/provekit-linker/tests/discharge_obligation.rs
git commit -m "Propagate linker callsite loci"
```

### Task 2: Include Callsite Locus In Linkerd Diagnostics

**Files:**
- Modify: `implementations/rust/provekit-linkerd/src/methods.rs`
- Modify: `implementations/rust/provekit-linkerd/src/state.rs`

- [ ] **Step 1: Write the failing state test**

Append this test module to `implementations/rust/provekit-linkerd/src/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use provekit_linker::{LinkerCallEdge, LinkerContract};
    use serde_json::json;

    const SOURCE_CID: &str = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn diagnostics_for_file_preserve_callsite_locus() {
        let mut state = ProjectState::new(4);
        let locus = json!({
            "file": "/tmp/caller.rs",
            "line": 7,
            "column": 13
        });

        state.update_and_link(
            "rust",
            "/tmp/caller.rs",
            vec![LinkerContract {
                name: "caller".into(),
                kit: "rust-kit".into(),
                contract_cid: SOURCE_CID.into(),
                pre_json: None,
                post_json: Some(json!({"kind": "atomic", "name": "true", "args": []})),
            }],
            vec![LinkerCallEdge {
                source_contract_cid: SOURCE_CID.into(),
                target_contract_cid: None,
                target_symbol: "rust-kit:missing".into(),
                call_site_locus_json: locus.clone(),
                evidence_term_json: json!({"kind": "Atomic", "name": "obligation", "args": []}),
            }],
        );

        let diagnostics = state.diagnostics_for_file("/tmp/caller.rs");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["callSiteLocus"], locus);
    }
}
```

- [ ] **Step 2: Run the failing linkerd state test**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linkerd diagnostics_for_file_preserve_callsite_locus
```

Expected: FAIL because diagnostic JSON omits `callSiteLocus`.

- [ ] **Step 3: Add `callSiteLocus` to linkerd JSON responses**

In `handle_parse_file`, update the diagnostic JSON:

```rust
"callSiteLocus": e.call_site_locus_json,
```

In `ProjectState::diagnostics_for_file`, update the diagnostic JSON:

```rust
"callSiteLocus": e.call_site_locus_json,
```

- [ ] **Step 4: Run the linkerd state test**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linkerd diagnostics_for_file_preserve_callsite_locus
```

Expected: PASS.

- [ ] **Step 5: Commit the daemon JSON slice**

```bash
git add implementations/rust/provekit-linkerd/src/methods.rs implementations/rust/provekit-linkerd/src/state.rs
git commit -m "Return callsite loci from linkerd diagnostics"
```

### Task 3: Convert Daemon Callsite Loci To LSP Ranges

**Files:**
- Modify: `implementations/rust/provekit-lsp/src/main.rs`

- [ ] **Step 1: Write failing LSP conversion tests**

In `implementations/rust/provekit-lsp/src/main.rs`, update the test helper:

```rust
fn make_diag_with_locus(error_kind: &str, target_symbol: &str, reason: &str, locus: serde_json::Value) -> DaemonDiagnostic {
    DaemonDiagnostic {
        error_kind: error_kind.to_string(),
        target_symbol: target_symbol.to_string(),
        reason: reason.to_string(),
        call_site_locus: Some(locus),
    }
}
```

Add this test:

```rust
#[test]
fn callsite_locus_maps_to_lsp_range() {
    let d = make_diag_with_locus(
        "implication-unprovable",
        "checkPositive",
        "solver found a counterexample",
        serde_json::json!({"file": "/tmp/test.rs", "line": 20, "column": 17}),
    );

    let lsp = daemon_diag_to_lsp(&d);
    assert_eq!(lsp.range.start.line, 19);
    assert_eq!(lsp.range.start.character, 17);
    assert_eq!(lsp.range.end.line, 19);
    assert_eq!(lsp.range.end.character, 18);
    assert_eq!(
        lsp.code,
        Some(NumberOrString::String("provekit.lsp.implication_failed".to_string()))
    );
}
```

- [ ] **Step 2: Run the failing LSP unit test**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-lsp callsite_locus_maps_to_lsp_range
```

Expected: FAIL to compile because `DaemonDiagnostic` has no `call_site_locus` field.

- [ ] **Step 3: Add daemon diagnostic locus parsing and range conversion**

In `DaemonDiagnostic`, add:

```rust
#[serde(rename = "callSiteLocus", default)]
call_site_locus: Option<serde_json::Value>,
```

Add a helper near `daemon_diag_to_lsp`:

```rust
fn locus_to_lsp_range(locus: Option<&serde_json::Value>) -> Range {
    let Some(locus) = locus else {
        return file_start_range();
    };
    let line = locus.get("line").and_then(|v| v.as_u64()).unwrap_or(1);
    let column = locus
        .get("column")
        .or_else(|| locus.get("col"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let end_line = locus
        .get("endLine")
        .and_then(|v| v.as_u64())
        .unwrap_or(line);
    let end_column = locus
        .get("endColumn")
        .and_then(|v| v.as_u64())
        .unwrap_or(column + 1);

    Range {
        start: Position {
            line: line.saturating_sub(1) as u32,
            character: column as u32,
        },
        end: Position {
            line: end_line.saturating_sub(1) as u32,
            character: end_column as u32,
        },
    }
}

fn file_start_range() -> Range {
    Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 1 },
    }
}
```

Replace the hardcoded range in `daemon_diag_to_lsp`:

```rust
let range = locus_to_lsp_range(d.call_site_locus.as_ref());
```

Map codes:

```rust
let code = match d.error_kind.as_str() {
    "implication-unprovable" => "provekit.lsp.implication_failed",
    "unprovable-obligation" => "provekit.lsp.implication_failed",
    "unresolved-symbol" => "provekit.lsp.unresolved_symbol",
    "implication-undecidable" => "provekit.lsp.unprovable_obligation",
    _ => "provekit.lsp.unprovable_obligation",
};
```

Use `code` in the diagnostic.

- [ ] **Step 4: Run the LSP unit tests**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-lsp callsite_locus_maps_to_lsp_range
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-lsp range_is_file_start_marker
```

Expected: PASS. The fallback test should still prove missing loci use `(0,0)..(0,1)`.

- [ ] **Step 5: Commit the LSP conversion slice**

```bash
git add implementations/rust/provekit-lsp/src/main.rs
git commit -m "Map solver diagnostics to callsite ranges"
```

### Task 4: Run Focused Regression Set

**Files:**
- No code changes unless a focused test exposes a real issue.

- [ ] **Step 1: Run focused crate tests**

Run:

```bash
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linker --test discharge_obligation
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-linkerd diagnostics_for_file_preserve_callsite_locus
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-lsp callsite_locus_maps_to_lsp_range
```

Expected: PASS.

- [ ] **Step 2: Run formatting/checks for touched Rust crates**

Run:

```bash
cargo fmt --manifest-path implementations/rust/Cargo.toml --check
cargo check --manifest-path implementations/rust/Cargo.toml -p provekit-linker -p provekit-linkerd -p provekit-lsp
```

Expected: PASS.

- [ ] **Step 3: Inspect final diff**

Run:

```bash
git diff --stat origin/main...HEAD
git diff --check origin/main...HEAD
```

Expected: no whitespace errors; diff limited to the approved spec/plan and Rust/linkerd/LSP vertical slice.

- [ ] **Step 4: Final commit if needed**

If Task 4 required fixes:

```bash
git add <fixed-files>
git commit -m "Stabilize LSP callsite diagnostics"
```

If Task 4 required no fixes, do not create an empty commit.
