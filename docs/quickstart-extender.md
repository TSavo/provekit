# Quickstart: Extend Sugar

Sugar is a substrate, not a tool. This document walks through the architecture and shows you the three high-leverage extension points: writing a new kit lifter, extending the protocol via a normative spec, and adding a new lift adapter for an existing annotation library.

Before reading further: the architectural derivation (`docs/launch/the-pieces-on-the-table.md`) and the manifesto (`docs/launch/substrate-not-blockchain.md`) are required reading, not supplemental. The extension points only make sense against that backdrop. If you have not read them, read them first.

## Architecture in five minutes

Note on the editor path below: the four-stage editor-squiggle pipeline
(linker daemon plus LSP server plus editor) is the roadmap target, not the
shipped path. What runs today is a batch plugin protocol: the `provekit` CLI
spawns each kit as a subprocess per invocation, drives it over JSON-RPC, and
reads the result. See "The kit plugin RPC surface" below for the protocol the
kits actually speak now. The diagram is the intended end state.

The pipeline has four stages, each with a clean seam:

```
[source annotations]
        |
        v
   [kit lifter]           -- per-language; extracts contracts + call edges from source
        |
        v  (NDJSON RPC: provekit-lsp-plugin/1)
   [linker daemon]        -- provekit-linkerd; unions all kits; derives bridges
        |
        v
   [LSP server]           -- provekit-lsp; converts linker-error mementos to LSP diagnostics
        |
        v
   [editor]               -- red squiggle at the call site
```

**Kit lifter.** A binary that speaks the `provekit-lsp-plugin/1` NDJSON protocol over stdio. It receives `initialize` / `parse` / `shutdown` JSON-RPC calls and responds. The `parse` method receives `{uri, text}` for a source file and returns `{annotations: [...]}` plus optionally `{callEdges: [...]}`. The lifter does one thing: translate the host language's contract syntax into Sugar IR.

**Linker daemon (`provekit-linkerd`).** The daemon holds the union of all kits' contract and call-edge streams in memory. When a kit lifter (via the LSP server) sends a `parseFile` RPC, the daemon lifts the file, re-derives affected bridges, and returns `{diagnostics: [...]}`. The daemon owns cross-language linkage. It is the only component that sees all kits at once.

**LSP server (`provekit-lsp`).** A thin adapter: receives `textDocument/didOpen` and `textDocument/didChange` events from the editor, forwards them to the daemon via `parseFile` RPC when `--daemon-socket` is active, and calls `publishDiagnostics` with the results.

**Protocol.** The wire contracts between these components are normative specs in `protocol/specs/`. The two load-bearing ones for extension work are:
- `2026-05-03-bridge-linkage-protocol.md` (spec #114): the lifter output format and linker derivation
- `2026-05-04-linker-daemon-protocol.md` (spec #126): the daemon wire protocol

## Required reading, in order

1. `docs/launch/substrate-not-blockchain.md`: manifesto §1-§12. The substrate posture and why it stays small.
2. `docs/launch/the-pieces-on-the-table.md`: twelve-step architectural derivation. Steps 6-9 cover bridges and the linker specifically.
3. `docs/launch/path-to-default.md`: adoption strategy. Explains why the LSP path is the priority.
4. `protocol/specs/2026-05-03-bridge-linkage-protocol.md`: normative. The lifter output contract: two streams (contracts + call-edges), the derived bridge shape, the linker derivation algorithm.
5. `protocol/specs/2026-05-04-linker-daemon-protocol.md`: normative. The daemon wire protocol: five JSON-RPC methods, socket location, lifecycle, cache/snapshot.
6. `protocol/specs/2026-04-30-ir-formal-grammar.md`: normative. The IR grammar: how predicates are expressed as content-addressed JSON trees.

After those six, the remaining specs in `protocol/specs/` are reference material to consult as needed.

## Build from source

The top-level Makefile orchestrates the full build:

```sh
# Build the Rust workspace (CLI, LSP server, linker daemon, tools)
make build-rust

# Run the Linux-profile conformance gate
make conformance

# Run the Linux native test aggregate
make test-all

# Run both
make ci
```

`make ci` is the local Linux-profile gate. If it is green, the Linux profile's self-contracts round-trip to pinned CIDs, the catalog hash matches, and the Linux native test aggregate passes. The full GitHub workflow adds the macOS Swift profile and per-kit verifier jobs.

Per-language builds for non-Rust kits:

```sh
make build-go       # Go modules in implementations/go/
make build-ts       # TypeScript packages (pnpm install)
make build-csharp   # dotnet build
make build-cpp      # clang++ + vendored BLAKE3
```

To build only the binaries you need for a Rust kit contribution:

```sh
cargo build --release --manifest-path implementations/rust/Cargo.toml
```

This builds the CLI (`provekit`), LSP server (`provekit-lsp`), linker daemon (`provekit-linkerd`), lifter (`provekit-lift`), and all supporting crates in one pass.

## Add a new kit lifter

A kit lifter is a binary that speaks `provekit-lsp-plugin/1`. The daemon dispatches to it by file extension.

### The protocol

The lifter receives three JSON-RPC calls over stdio (NDJSON, one message per line):

**`initialize`**
```json
{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}
```
Respond with name and version:
```json
{"jsonrpc": "2.0", "id": 1, "result": {"name": "provekit-lsp-mylang", "version": "0.1.0"}}
```

**`parse`**
```json
{"jsonrpc": "2.0", "id": 2, "method": "parse", "params": {"path": "/abs/path/to/file.ml", "source": "<file contents>"}}
```
Respond with contracts and call edges:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "declarations": [
      {
        "schemaVersion": "1",
        "kind": "contract",
        "name": "my_function",
        "pre": {"op": "gt", "args": [{"var": "n"}, {"const": 0}]},
        "post": null,
        "inv": null,
        "locus": {"file": "/abs/path/to/file.ml", "line": 10, "col": 0}
      }
    ],
    "callEdges": [
      {
        "schemaVersion": "1",
        "kind": "call-edge",
        "sourceContractCid": null,
        "targetContractCid": null,
        "targetSymbol": "rust-kit:process",
        "callSiteLocus": {"file": "/abs/path/to/file.ml", "line": 15, "col": 4},
        "evidenceTerm": null
      }
    ]
  }
}
```

**`shutdown`**
```json
{"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}}
```
Exit cleanly.

### Register with the daemon

The daemon's `lift_source` dispatch is in `implementations/rust/provekit-linkerd/src/methods.rs`. To add your kit:

1. Add your kit ID to the `KitId` enum in `protocol/specs/2026-05-04-linker-daemon-protocol.md` §1 and the daemon's matching code.
2. In the daemon's `lift_source` function, add a dispatch arm that spawns your binary as a subprocess and sends it a `parse` call.
3. Follow the existing pattern for `go`, `csharp`, `ruby`, or `zig` kits (each uses the subprocess dispatch path with a PATH binary).

### Run the cross-kit conformance gate

After adding your kit, run `make conformance`. The conformance gate verifies that byte-identical IR predicates produce byte-identical CIDs across all kits that have self-contracts. If your kit has self-contracts (a binary that mints its own contracts), add a `make mint-<lang>` target and a self-contracts attestation under `.provekit/self-contracts-attestations/<lang>.json`. See the Makefile's bump-dance comment at the top for the full attestation flow.

## The kit plugin RPC surface (what runs today)

The shipped kit interface is a batch plugin protocol, not a persistent editor
language server. Each kit ships an NDJSON-over-stdio JSON-RPC 2.0 server. The
Rust CLI SPAWNS one as a subprocess per `.provekit/lift/<surface>/manifest.toml`
`command`, handshakes, sends a method, reads the response, then shuts it down.
These servers are spawned per invocation and driven by the CLI, not held open by
an editor.

### Handshake

Every kit answers two methods first:

- `initialize`
- `provekit.plugin.kit_declaration`, which advertises the kit's
  `protocol_version` (today `provekit-lsp-shared/1`), `kit_id`, and the list of
  methods it implements.

(`implementations/python/provekit-lift-py-pytest-witness/src/provekit_pytest_witness/lift_lsp.py:30,166`.)

### Method sets differ by kit family

There is one shared handshake, but the method set is per kit family, and the
naming across kits is not unified yet (you will see `lsp.py`, `lift_lsp.py`,
`rpc.py`, `bind_rpc.py` across the tree). Describe your kit's methods honestly in
its `kit_declaration` rather than assuming a single fixed protocol.

- Lift kits implement `lift` (plus per-kit extras).
- The sugar kit (`bind_rpc.py`) adds `provekit.plugin.recognize` and
  `provekit.plugin.materialize`.
  (`implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_rpc.py:84,90`.)
- Realize kits add `provekit.plugin.invoke`, `assemble`, `emit_module`,
  `body_template_entries`, `resolve_dependency_proofs`, and `check`.
  (`implementations/python/provekit-realize-python-requests/src/provekit_realize_python_requests/rpc.py`.)
- The witness kit adds `provekit.plugin.resolve_witness`, covered next.

### Shipping witnesses from a kit

A witness is arbitrary signed content used as an attestation: a test run, a CI
log, a poem. A kit that ships witnesses RESOLVES bodies; it does not pronounce
verdicts. Verification lives in the Rust CLI. This is the trust boundary: the kit
resolves, the CLI verifies.

The witness kit declares a separate RPC entry in its manifest, alongside the lift
`command`:

```toml
resolve_witness_command = ["...", "python3", "-m", "provekit_pytest_witness.lift_lsp"]
resolve_witness_method = "provekit.plugin.resolve_witness"
```

(`examples/pytest-witness-dummy/.provekit/lift/python-pytest-witness/manifest.toml`.)

The handler returns the body bytes, base64-encoded, NOT a verdict. It has two
resolution strategies: read the CID-named body from the witness package, or
re-run the pinned test and rebuild the canonical body. A body it cannot resolve
is an error, which the verifier treats as refusal.

```python
def handle_resolve_witness(msg_id, params):
    # ... resolve body bytes from the witness package or by re-running ...
    return {"body_b64": base64.b64encode(body).decode("ascii")}
```

(`implementations/python/provekit-lift-py-pytest-witness/src/provekit_pytest_witness/lift_lsp.py:106-153`.)

The Rust CLI does the verification the kit is not trusted to do: it verifies the
Ed25519 signature itself with the substrate's own primitive, fetches the body
over `resolve_witness`, BLAKE3's those bytes itself, and compares to the pinned
`witness_cid`. A body the oracle returns that does not recompute is a broken
oracle, caught because Rust does the math anyway; an honest re-run that differs
is drift. Both refuse, loudly, and are distinguished.
(`implementations/rust/provekit-cli/src/witness_verify.rs:1-18`.)

So a kit that ships witnesses implements exactly one new method,
`provekit.plugin.resolve_witness`, declares it in the manifest, and writes a
witness package of CID-named bodies (see
[docs/how-to/publishing-a-proof.md](how-to/publishing-a-proof.md)). The kit never
grades itself.

### A real editor LSP (shipped for Python)

A real editor language server, where a `provekit prove` or `verify`
contradiction surfaces as an inline diagnostic (a red squiggle on the offending
line), exists for Python:
`implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/editor_lsp.py`,
exposed as the `provekit-editor-lsp-python` console script. Unlike the batch
lift plugin (`lsp.py`, the provekit-lift/1 NDJSON protocol the CLI spawns per
invocation), this is a persistent stdio server speaking the LSP base wire
protocol (Content-Length-framed JSON-RPC): `initialize`, `didOpen`, `didChange`,
`didSave`, `publishDiagnostics`.

It is a thin client over `provekit prove`. On open and save it runs `provekit
prove --json` over the document's project, and for each unsatisfied obligation it
recovers the callsite from the report's `#euf#` property term, matches it against
the document AST, and publishes an `Error`-severity diagnostic on that call. The
prove runner is injected, so the protocol is testable without the toolchain
(`tests/test_editor_lsp.py` drives the wire with the real captured report, plus a
skip-guarded end-to-end test that spawns the real CLI). Verification stays in the
rust CLI; the server never grades correctness itself.

Two boundaries are deliberate. Diagnostics refresh on open and save (against the
on-disk project); live dirty-buffer analysis ("squiggle as you type") would need
a buffer overlay so prove does not run against stale disk, and is not done yet.
And this is the Python-kit-native server; the cross-language, linker-daemon-routed
editor server (the rust `provekit-lsp` / `provekit-linkerd` binaries) is still
roadmap. To add a squiggle server for another language, mirror `editor_lsp.py`:
the wire protocol and prove-report rendering are language-agnostic; only the
callsite-recovery AST pass is language-specific.

The older `lsp.py` `analyzeDocument` method (a forward-propagation demo over a
fixed `checkPositive` contract) remains as-is; it predates this server and backs
the `provekit_lsp` forward-propagator demo and the rust linker tests.

## Extend the protocol

A protocol extension is a normative spec file under `protocol/specs/`. The file name convention is `YYYY-MM-DD-<short-slug>.md`.

### Structure

Every normative spec follows this structure:

```markdown
# <Title>

**Status:** v1.0.0 normative spec
**Date:** YYYY-MM-DD
**Companion specs:** (list related specs)

## §0. Motivation

Why this spec exists. What problem it solves. What gap in the existing specs it closes.

## §1. Definitions

Terms used throughout.

## §2. Normative requirements

R1. ...
R2. ...

Each requirement uses RFC 2119 terms (MUST, SHALL, SHOULD, MAY).
```

### CID registration

Every spec file gets a CID. After writing the spec:

1. Run `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --write` to recompute all spec CIDs and update the catalog.
2. Run `make catalog-verify` to confirm the catalog hash matches.
3. Decide the version class under PEP. Extension-only additions with no required kit emission, lift, canonicalization, or verifier change can be patch-level transitions. Grammar or core semantic obligations generally require a minor or major bump.
4. If the catalog CID changes (it will, since you added a cataloged spec), update the `CATALOG_CID` variable in the Makefile, bump the protocol version in `protocol/specs/2026-04-30-protocol-catalog.json`, and add a PEP transition under `protocol/evolution/v<new-version>/`.
5. Sign the catalog attestation and commit the PEP body/witness artifacts.

Spec CIDs are self-identifying: the CID is a BLAKE3-512 hash of the spec file's bytes. Anyone can verify a spec's CID locally. No central party decides what a spec means; the bytes do.

### Foundation signing

The canonical catalog is signed under the foundation key. Signing a new catalog requires the foundation private key (held by the project maintainer). For contributions, open a PR with your spec and the updated catalog; the maintainer signs the new catalog before merge.

## Add a new lift adapter

A lift adapter is a component of an existing kit lifter that recognizes a specific annotation library. For example, the Rust kit has `provekit-lift-proptest` (walks `proptest!` blocks) and `provekit-lift-contracts` (walks `#[requires]/[ensures]` macros). Adding a new adapter means adding a new pattern recognizer to an existing kit's lifter.

### Anatomy of a Rust lift adapter

```
implementations/rust/provekit-lift/src/
  adapters/
    proptest.rs      -- walk proptest! blocks
    contracts.rs     -- walk #[requires], #[ensures], #[invariant]
    <your_adapter>.rs
```

Each adapter implements:

```rust
pub fn lift(source: &str, file_path: &Path) -> Vec<ContractDecl> {
    // Parse source (using syn for Rust, or a hand-rolled parser for comment annotations).
    // For each annotation found, produce a ContractDecl with:
    //   - name: the function name
    //   - pre: the precondition IR formula (or None)
    //   - post: the postcondition IR formula (or None)
    //   - locus: the source location
    // Return the list.
}
```

The IR formula format is defined in `protocol/specs/2026-04-30-ir-formal-grammar.md`. At the leaf level you have `{"var": "x"}` for arguments and `{"const": 42}` for constants; at the predicate level you have operators like `{"op": "gt", "args": [...]}`. The full grammar is in the spec.

### Conformance test

Every new adapter needs a conformance test that asserts the emitted IR is byte-identical to the reference output for a known annotation. Add your test to the adapter module:

```rust
#[test]
fn my_annotation_lifts_to_expected_ir() {
    let source = r#"
        #[my_annotation(n > 0)]
        fn process(n: i32) -> i32 { n * 2 }
    "#;
    let contracts = lift(source, Path::new("test.rs"));
    assert_eq!(contracts.len(), 1);
    let pre = contracts[0].pre.as_ref().expect("should have pre");
    // Assert the IR structure matches the expected predicate tree.
    assert_eq!(pre["op"], "gt");
}
```

Run `make test-rust` to confirm.

### Adding adapters for other languages

The pattern is the same for other languages, but the implementation language and parser tools differ:

- **Go:** `implementations/go/provekit-lift-go-tests/`: uses Go AST via `go/ast` and `go/parser`.
- **Python:** `implementations/python/provekit-lift-py-tests/`: uses `ast` module from stdlib.
- **C#:** `implementations/csharp/Provekit.Lift.DataAnnotations/`: uses Roslyn `SyntaxTree` API.
- **Ruby:** `implementations/ruby/lib/provekit/lift/`: uses the `parser` gem.

## Where things live

```
implementations/
  rust/               -- canonical reference implementation
    provekit-cli/     -- CLI binary (provekit)
    provekit-lsp/     -- LSP server (provekit-lsp)
    provekit-linkerd/ -- linker daemon (provekit-linkerd)
    provekit-lift/    -- source lifter library + adapters
    provekit-linker/  -- pure linker algebra (no I/O)
    provekit-canonicalizer/ -- IR canonicalization (JCS + BLAKE3-512)
    ...
  go/
    provekit-ir-symbolic/   -- Go IR library
    provekit-self-contracts/ -- Go self-contracts mint
    provekit-lift-go-tests/ -- Go lifter (binary: provekit-lsp-go)
  typescript/         -- TypeScript kit
  python/             -- Python kit
  cpp/                -- C++ kit
  csharp/             -- C# kit
  ruby/               -- Ruby kit
  zig/                -- Zig kit
  swift/              -- Swift kit

protocol/
  specs/              -- normative specs, each content-addressed by CID
  2026-04-30-protocol-catalog.json -- catalog of all spec CIDs

tools/
  recompute-spec-cids/ -- verifies and rewrites spec CID catalog
  foundation-keygen/   -- foundation key generation + self-contracts signing
  blake3-vendored/     -- BLAKE3 1.8.5, portable C, Apache-2.0

docs/
  launch/
    substrate-not-blockchain.md  -- manifesto §1-§12
    the-pieces-on-the-table.md   -- twelve-step architectural derivation
    path-to-default.md           -- adoption strategy
  per-language-status.md         -- kit + adapter coverage matrix
  lift-adoption-paths.md         -- per-source-library adapter guide
  quickstart-end-user.md         -- this project's red-squiggle guide
  quickstart-extender.md         -- this document

examples/
  polyglot-rust-go/   -- canonical cross-language demo (rust callee + go caller)
  build_script_demo/  -- provekit-build build.rs integration

.provekit/
  self-contracts-attestations/  -- per-kit signed attestation envelopes
  self-lifts/                   -- experimental self-lift artifacts

.github/
  workflows/
    ci.yml            -- runs make ci on ubuntu-latest
```

## How CI works

CI runs `make ci` for the Linux profile, then runs additional macOS Swift and per-kit verifier jobs. The local Linux gate has two parts:

**`make conformance`**: six checks in sequence.
1. `catalog-verify`: recomputes all spec CIDs from spec bytes and confirms they match `protocol/specs/2026-04-30-protocol-catalog.json`. Fails on any drift.
2. `protocol-verify`: runs `provekit verify-protocol --signed`, which checks that the local binary's declared catalog CID is signed by the foundation key.
3. `all-mint`: runs the Linux-profile per-kit mint commands. Each mints the kit's self-contracts bundle, computes the `contractSetCid`, and verifies it against the signed attestation envelope in `.provekit/self-contracts-attestations/<lang>.json`. Fails if the contractSetCid has drifted from the signed value.
4. `test-self-contracts`: runs the Rust kit's catalog-format unit tests (19 tests covering R1-R15 of the protocol catalog format spec). Fails if any format invariant regresses.
5. `conformance-region-fixture`: checks the RegionSort fixture coverage.
6. `cross-kit-conformance`: runs the profile-aware cross-kit conformance harness.

**`make test-all`**: runs the Linux native test aggregate in sequence.

To debug a `mint-<lang>` failure: the Makefile prints the new `cid` and `contractSetCid` when the attestation check fails. If you changed self-contracts intentionally, follow the bump dance printed in the error message (the `sign-self-contracts` tool call). If the drift is unintentional, look for a change in the kit's source that altered the lifted contracts.

To debug a `catalog-verify` failure: you added or changed a spec file. Run `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --write` to update the catalog, then commit the updated catalog JSON alongside your spec change.

## The contribution shape

A contribution to Sugar is one of:

- **A new kit lifter binary**: implement the `provekit-lsp-plugin/1` protocol, add a daemon dispatch arm, add conformance tests, open a PR.
- **A new lift adapter**: add pattern recognition to an existing lifter, add a conformance test asserting byte-identical IR for a known annotation, open a PR.
- **A normative spec**: write the spec under `protocol/specs/`, recompute the catalog, open a PR. The maintainer signs the new catalog before merge.
- **An LSP plugin polish fix**: the LSP server and daemon have per-kit gaps documented in the status matrix. Pick a `Gap -- follow-up required` row and close it.

The substrate stays small. Contributions that add new features to the protocol must show that the feature cannot be expressed as composition over the existing three primitives (sign, hash, reference). Almost always it can be, and the right form is a lift adapter or a tooling change, not a protocol extension.
