# Menagerie Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Bug Zoo into `menagerie/bug-zoo/` and add the Menagerie rail-system entry point.

**Architecture:** Preserve the existing Bug Zoo crate and runner behavior while changing its physical home. Add Menagerie-level docs and a manifest, then update code, tests, docs, Cargo workspace membership, and self-contract paths to the new hierarchy.

**Tech Stack:** Rust/Cargo workspace, Markdown docs, YAML manifest, shell-based path verification.

---

### Task 1: Red Tests For The New Home

**Files:**
- Modify: `implementations/rust/sugar-cli/tests/cli_surface.rs`

- [ ] **Step 1: Write the failing location test**

Change `bug_zoo_machinery_is_self_contained` so it expects `menagerie/bug-zoo/Cargo.toml`, rejects a root `bug-zoo/Cargo.toml`, and checks that CLI support code still does not own the harness:

```rust
#[test]
fn bug_zoo_machinery_is_self_contained() {
    let root = repo_root();
    assert!(
        root.join("menagerie/bug-zoo/Cargo.toml").exists(),
        "Bug Zoo should live as a Menagerie destination under menagerie/bug-zoo/"
    );
    assert!(
        !root.join("bug-zoo/Cargo.toml").exists(),
        "Bug Zoo should no longer live at the repository root"
    );
    assert!(
        !root
            .join("implementations/rust/sugar-cli/src/cmd_zoo.rs")
            .exists(),
        "Bug Zoo should not be embedded as a sugar CLI command"
    );
    assert!(
        !root
            .join("implementations/rust/sugar-cli/tests/support/bug_zoo.rs")
            .exists(),
        "Bug Zoo harness code should live under menagerie/bug-zoo/, not sugar-cli tests"
    );
}
```

- [ ] **Step 2: Run the focused test and confirm red**

Run:

```sh
cargo test --manifest-path implementations/rust/sugar-cli/Cargo.toml bug_zoo_machinery_is_self_contained
```

Expected: FAIL because `menagerie/bug-zoo/Cargo.toml` does not exist yet.

### Task 2: Move Bug Zoo And Fix Build Paths

**Files:**
- Move: `bug-zoo/` to `menagerie/bug-zoo/`
- Modify: `menagerie/bug-zoo/Cargo.toml`
- Modify: `menagerie/bug-zoo/tests/smoke.rs`
- Modify: `menagerie/bug-zoo/src/lib.rs`
- Modify: `menagerie/bug-zoo/src/lib.invariant.rs`
- Modify: `implementations/rust/Cargo.toml`
- Modify: `implementations/rust/sugar-self-contracts/src/lib.rs`

- [ ] **Step 1: Move the directory**

Run:

```sh
mkdir -p menagerie
git mv bug-zoo menagerie/bug-zoo
```

- [ ] **Step 2: Update Cargo paths**

In `menagerie/bug-zoo/Cargo.toml`, set:

```toml
workspace = "../../implementations/rust"
sugar-canonicalizer = { path = "../../implementations/rust/sugar-canonicalizer" }
```

In `implementations/rust/Cargo.toml`, replace workspace member:

```toml
"../../menagerie/bug-zoo",
```

- [ ] **Step 3: Update Bug Zoo repo-root logic and default paths**

In `menagerie/bug-zoo/tests/smoke.rs`, change `repo_root()` so it walks from `menagerie/bug-zoo` back to the repository root:

```rust
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
```

In `menagerie/bug-zoo/src/lib.rs`, change default species paths from `bug-zoo/species` to `menagerie/bug-zoo/species`.

In `menagerie/bug-zoo/src/lib.invariant.rs`, update the header path comment to `menagerie/bug-zoo/src/lib.rs`.

- [ ] **Step 4: Update self-contract include paths**

In `implementations/rust/sugar-self-contracts/src/lib.rs`, change:

```rust
#[path = "../../../../menagerie/bug-zoo/src/lib.invariant.rs"]
mod bug_zoo_invariants;
```

and update the slab metadata path to:

```rust
path: "../../menagerie/bug-zoo/src/lib.invariant.rs",
```

- [ ] **Step 5: Run the focused test and confirm green**

Run:

```sh
cargo test --manifest-path implementations/rust/sugar-cli/Cargo.toml bug_zoo_machinery_is_self_contained
```

Expected: PASS.

### Task 3: Add Menagerie Entry Point

**Files:**
- Create: `menagerie/README.md`
- Create: `menagerie/manifest.yaml`

- [ ] **Step 1: Add `menagerie/README.md`**

Create a README that defines Menagerie as Sugar's executable proof-workflow map and lists these destinations:

```text
Bug Zoo
Hashbound Mainline
Supply Chain Rails
Bridgeworks
Protocol Switchyard
Change Station
```

- [ ] **Step 2: Add `menagerie/manifest.yaml`**

Create manifest entries for:

```yaml
destinations:
  - id: bug-zoo
    path: bug-zoo
    runnable: true
    claim: bugs as missing edges and fixes as closure receipts
    command: cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
  - id: hashbound-mainline
    path: hashbound-mainline
    runnable: planned
    claim: cross-domain implication chains compressed to 64-byte verification
  - id: supply-chain-rails
    path: supply-chain-rails
    runnable: planned
    claim: artifact admission through rank-3 pins, binaryCid, and CICP
  - id: bridgeworks
    path: bridgeworks
    runnable: planned
    claim: proof preservation across language, package, protocol, and CI boundaries
  - id: protocol-switchyard
    path: protocol-switchyard
    runnable: planned
    claim: protocol versions as roots and migrations as witnessed edges
  - id: change-station
    path: change-station
    runnable: planned
    claim: commits as p -> q proof-carrying transitions
```

### Task 4: Update Current Documentation And Commands

**Files:**
- Modify: `README.md`
- Modify: `Makefile`
- Modify: `docs/contributing/build.md`
- Modify: `docs/explanation/architecture.md`
- Modify: `docs/explanation/bug-zoo.md`
- Modify: `docs/explanation/landing.md`
- Modify: `docs/explanation/use-cases.md`
- Modify: `docs/how-to/bug-zoo.md`
- Modify: `docs/index.md`
- Modify: `docs/reference/per-language-status.md`
- Modify: `docs/reference/protocol-extensions.md`
- Modify: `docs/tutorials/java.md`
- Modify: `protocol/specs/2026-05-06-obligation-realizer-protocol.md`

- [ ] **Step 1: Replace current runnable paths**

Mechanically replace current runnable path references:

```text
bug-zoo/Cargo.toml -> menagerie/bug-zoo/Cargo.toml
bug-zoo/species -> menagerie/bug-zoo/species
../../bug-zoo/README.md -> ../../menagerie/bug-zoo/README.md
../../bug-zoo/species/ -> ../../menagerie/bug-zoo/species/
under `bug-zoo/` -> under `menagerie/bug-zoo/`
```

- [ ] **Step 2: Reframe root README**

Add a short Menagerie section near the Bug Zoo entry point explaining that Bug Zoo is one Menagerie destination and that future destinations include Hashbound Mainline, Supply Chain Rails, Bridgeworks, Protocol Switchyard, and Change Station.

- [ ] **Step 3: Verify no current path references remain**

Run:

```sh
rg -n "bug-zoo/Cargo.toml|bug-zoo/species|under `bug-zoo/`|root \\[../../bug-zoo|\\.\\./\\.\\./bug-zoo" README.md Makefile docs/contributing docs/explanation docs/how-to docs/index.md docs/reference docs/tutorials protocol/specs implementations/rust -S
```

Expected: no matches, except historical design/plan files outside this command scope.

### Task 5: Refresh Generated Link Bundle Receipts If Needed

**Files:**
- Modify if regenerated: `menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/exhibit/cgo-rust-callee/harness/link-bundle.json`
- Modify if regenerated: `menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/fixed/cgo-rust-callee/harness/link-bundle.json`

- [ ] **Step 1: Run the Bug Zoo JSON report**

Run:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all --json
```

Expected: if LinkBundle path CIDs changed, the runner fails with a LinkBundle mismatch showing the newly linked CID.

- [ ] **Step 2: Regenerate link bundles when mismatch is path-only**

Run the underlying `sugar link` commands from the repository root:

```sh
cargo run --manifest-path implementations/rust/sugar-cli/Cargo.toml -- link menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/exhibit/cgo-rust-callee/harness --go-lsp-bin "$(pwd)/menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/kit-rpc/run-go-lsp.sh"
cargo run --manifest-path implementations/rust/sugar-cli/Cargo.toml -- link menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/fixed/cgo-rust-callee/harness --go-lsp-bin "$(pwd)/menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation/rust-go/kit-rpc/run-go-lsp.sh"
```

Expected: the two `link-bundle.json` files update with `menagerie/bug-zoo/...` source paths and new bundle CIDs.

### Task 6: Verify The Migration

**Files:**
- No new files.

- [ ] **Step 1: Format Rust crates**

Run:

```sh
cargo fmt --manifest-path menagerie/bug-zoo/Cargo.toml
cargo fmt --manifest-path implementations/rust/Cargo.toml
```

Expected: formatting succeeds.

- [ ] **Step 2: Run focused tests**

Run:

```sh
cargo test --manifest-path implementations/rust/sugar-cli/Cargo.toml bug_zoo_machinery_is_self_contained
cargo test --manifest-path menagerie/bug-zoo/Cargo.toml runner_help_is_self_contained
```

Expected: both pass.

- [ ] **Step 3: Run Bug Zoo smoke path**

Run:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
```

Expected: all Bug Zoo specimens pass, or any environment dependency failure is reported exactly.

- [ ] **Step 4: Inspect git status**

Run:

```sh
git status --short
```

Expected: moved Bug Zoo files live under `menagerie/bug-zoo/`, Menagerie docs exist, and there is no root `bug-zoo/` directory.
