# Lift Plugin Protocol (`provekit-lift/1`)

Status: v1.2.0 normative. Listed in the v1.2.0 catalog under property key `lift-plugin-protocol`. CID is computed from the bytes of this file (raw-bytes BLAKE3-512); see the catalog at `protocol/specs/2026-04-30-protocol-catalog.json` for the value.

## Why

ProvekIt is the composition of two protocols (canonical IR + content addressing). Lift adapters are decoration: they consume host-language source and emit canonical IR mementos that the protocol then signs, hashes, bundles. Today the Rust CLI bundles Rust-only lift adapters as Rust crates (proptest, contracts, kani, prusti, creusot, flux, quickcheck, verus, rust-tests, .invariant.rs orchestrator). The C++, Go, TypeScript, and Python lifters live in their own per-language tooling and are invoked separately.

`provekit prove` should be one command that produces a `.proof` for any of the four peer impls just by changing directory. The lift plugin protocol is the seam that makes this possible: the Rust CLI subprocess-dispatches to per-language lift plugins via JSON-RPC over stdio (LSP shape; same shape MCP, nvim plugins, and the language-server ecosystem use). The plugin produces canonical IR; the Rust CLI signs/bundles/writes the `.proof`.

This spec defines:

1. The plugin manifest format.
2. The JSON-RPC method shapes.
3. Capability negotiation.
4. The error model.
5. The configuration surface in `.provekit/config.toml`.

## Plugin discovery

Plugins live at:

```
~/.config/provekit/lift/<name>/manifest.toml
```

Or, project-local:

```
.provekit/lift/<name>/manifest.toml
```

Project-local plugins shadow user-global plugins of the same name. Both are searched; project-local wins.

Manifest schema:

```toml
name = "go-self-contracts"
version = "1.0.0"
protocol_version = "provekit-lift/1"
command = ["go", "run", "./cmd/mint-go-self-contracts", "--rpc"]
working_dir = "implementations/go/provekit-self-contracts"

[capabilities]
authoring_surfaces = ["go-tests", "go-self-contracts"]
ir_version = "v1.1.0"
emits_signed_mementos = true
```

`command` is the argv used to spawn the plugin. The plugin SHOULD support a `--rpc` (or equivalent) flag indicating it should speak JSON-RPC on stdio rather than its default human-readable output.

`working_dir` is optional; if present, the Rust CLI sets the plugin's CWD to this path resolved relative to the workspace root.

## Transport

JSON-RPC 2.0 over stdio. One JSON object per line (NDJSON). The Rust CLI is the client; the plugin is the server. The plugin reads requests from stdin and writes responses to stdout. The plugin MAY write progress messages and diagnostics to stderr; the Rust CLI captures stderr and surfaces it to the user but does not parse it.

## Methods

### `initialize`

The first call after spawn. The Rust CLI sends:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
    "client":{"name":"provekit-cli","version":"v1.1.0"},
    "protocol_version":"provekit-lift/1",
    "workspace_root":"/abs/path/to/workspace",
    "config_path":".provekit/config.toml"
}}
```

The plugin responds with its capabilities and confirms protocol version:

```json
{"jsonrpc":"2.0","id":1,"result":{
    "name":"go-self-contracts",
    "version":"1.0.0",
    "protocol_version":"provekit-lift/1",
    "capabilities":{
        "authoring_surfaces":["go-tests","go-self-contracts"],
        "ir_version":"v1.1.0",
        "emits_signed_mementos":true
    }
}}
```

If `protocol_version` does not match, the plugin SHOULD fail the request with error code `PROTOCOL_VERSION_MISMATCH`.

### `lift`

The Rust CLI requests canonical IR mementos for a given source set:

```json
{"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "surface":"go-self-contracts",
    "source_paths":["implementations/go/provekit-ir-symbolic/canonicalizer/encoder.go"],
    "options":{"layer":"all"}
}}
```

The plugin responds with one of three shapes:

**(a) Canonical IR (mint pipeline runs in the Rust CLI):**

```json
{"jsonrpc":"2.0","id":2,"result":{
    "kind":"ir-document",
    "ir":[
        {"kind":"contract","name":"go_encode_jcs_is_deterministic","outBinding":"out","pre":{...}},
        ...
    ],
    "diagnostics":[]
}}
```

The `ir` field is the IR-JSON Document shape (an array of declarations) per `2026-04-30-ir-formal-grammar.md`. The Rust CLI takes this, marshals via JCS, mints each declaration as a memento under the foundation key, bundles into a `.proof`. This is the simplest plugin shape — the plugin only authors IR.

**(b) Pre-minted signed mementos (plugin owns the keypair):**

```json
{"jsonrpc":"2.0","id":2,"result":{
    "kind":"signed-mementos",
    "members":{
        "blake3-512:<cid>":{"bytes_base64":"...","kind":"contract"},
        ...
    },
    "signer_cid":"blake3-512:<signer-pubkey-cid>",
    "diagnostics":[]
}}
```

The plugin returns mementos already signed under its own key. The Rust CLI bundles them into a `.proof` envelope but does not re-sign individual members. This is the shape for plugins that need to sign with a non-foundation key (CI bot keys, reviewer keys, organization keys).

**(c) Complete `.proof` bytes (plugin owns the full pipeline):**

```json
{"jsonrpc":"2.0","id":2,"result":{
    "kind":"proof-envelope",
    "filename_cid":"blake3-512:<cid>",
    "bytes_base64":"...",
    "diagnostics":[]
}}
```

The plugin returns a complete `.proof` bundle. The Rust CLI writes the bytes to `<filename_cid>.proof`. This is the shape for plugins that already have their own mint+bundle pipeline (e.g., the existing `mint-self-contracts`, `mint-go-self-contracts`, `mint_cpp_self_contracts` binaries can be wrapped in a thin RPC layer to expose this shape).

A plugin MUST implement at least one of these three response shapes. A plugin MAY implement all three; the Rust CLI selects the first available shape compatible with the workspace's config.

### `shutdown`

The Rust CLI sends:

```json
{"jsonrpc":"2.0","id":99,"method":"shutdown"}
```

The plugin completes any in-flight `lift` calls, releases resources, responds with `{"id":99,"result":null}`, then exits.

## Capability negotiation

The `initialize` response declares which authoring surfaces the plugin supports. The Rust CLI matches the workspace's `[authoring] surface = ...` against the union of all plugins' capabilities and selects the first plugin (by manifest discovery order) that claims the surface.

If two plugins claim the same surface, the project-local plugin wins. If both are project-local or both are user-global, the lexicographically-earlier `name` wins, and the Rust CLI emits a diagnostic.

If no plugin claims the requested surface, `provekit prove` exits with `LIFT_PLUGIN_NOT_FOUND` and lists the candidates.

## Error model

JSON-RPC 2.0 error codes:

| Code | Name | Meaning |
|------|------|---------|
| -32700 | `PARSE_ERROR` | Plugin emitted invalid JSON. |
| -32600 | `INVALID_REQUEST` | Method call malformed. |
| -32601 | `METHOD_NOT_FOUND` | Plugin doesn't implement the method. |
| -32602 | `INVALID_PARAMS` | Required field missing. |
| -32603 | `INTERNAL_ERROR` | Unspecified plugin failure. |
| 1001 | `PROTOCOL_VERSION_MISMATCH` | Plugin's protocol version differs from client's. |
| 1002 | `IR_VERSION_MISMATCH` | Plugin's IR version is not compatible with the requested IR version. |
| 1003 | `SURFACE_NOT_SUPPORTED` | Plugin doesn't implement the requested authoring surface. |
| 1004 | `SOURCE_NOT_FOUND` | Plugin couldn't read a source path. |
| 1005 | `LIFT_FAILED` | Plugin executed but produced no IR (compile error in source, etc.). |

## Configuration

`.provekit/config.toml` extension:

```toml
[authoring]
surface = "go-self-contracts"   # selects which plugin handles `provekit prove`

[lift]
plugins_path = ["~/.config/provekit/lift", ".provekit/lift"]   # discovery order

[lift.options]
layer = "all"   # passed through as `options` in the lift call
```

## Conformance

A conformant plugin:

1. Implements `initialize`, `lift`, `shutdown`.
2. Returns the protocol version `provekit-lift/1` in the `initialize` response.
3. Emits canonical IR-JSON in the v1.1.0 shape (per `2026-04-30-ir-formal-grammar.md`) for the `ir-document` response shape.
4. If returning `signed-mementos` or `proof-envelope`, produces byte-deterministic output for byte-equal inputs (same source paths, same options → same CIDs).
5. Reports any diagnostics under the `diagnostics` field of the response, never silently dropping warnings.

A non-conformant plugin produces a different `provekit prove` output than a conformant one for the same inputs. The Rust CLI MAY refuse to dispatch to plugins that fail capability negotiation.

## Two paths into canonical IR

The lift-plugin protocol covers two architecturally distinct authoring paths into canonical IR. Both produce the same byte shape; they differ only in where the contract text lives before it becomes IR.

**Kit-authored**: the contract is written using the host language's kit-authoring API (Rust `provekit-macros-rt`, C++ `provekit/ir.hpp`, Go kit primitives, TS kit primitives). The kit produces canonical IR-JSON deterministically. Self-contracts (`.invariant.rs`, `.invariant.cpp`, `slabs/*.go`, `.invariant.{json,ts}`) are all kit-authored. The protocol uses this path to express its own correctness.

**Decorator-lifted**: the contract lives in an existing host-language annotation library (Zod schema, kani harness, proptest strategy, class-validator decorator, JSDoc tag, Pydantic model, etc.). A lift adapter parses the native annotation and emits canonical IR-JSON. Lift adapters are the canonical path for host code that already has annotations.

Both paths share the same `lift` method, the same response shapes, and the same IR-JSON output. The plugin manifest declares which path(s) it implements via the `authoring_surfaces` capability list. A single plugin MAY support both — for example, a Rust plugin could expose `surface = "rust-self-contracts"` (kit-authored) AND `surface = "kani"` (decorator-lifted) under the same binary.

The protocol doesn't care which path produced the IR. Both paths hash to identical CIDs for identical propositions. The only test is byte equality.

## Reference plugins

Three reference CLI plugins ship with ProvekIt:

| Surface | Plugin command | Reference |
|---------|---------------|-----------|
| `rust-self-contracts` | `mint-self-contracts --rpc` | `implementations/rust/provekit-self-contracts/` |
| `go-self-contracts` | `go run ./cmd/mint-go-self-contracts --rpc` | `implementations/go/provekit-self-contracts/cmd/` |
| `cpp-self-contracts` | `./target/mint_cpp_self_contracts --rpc` | `implementations/cpp/provekit-self-contracts/` |

Each implements the protocol over NDJSON-on-stdio, returning the `proof-envelope` shape (c). The dispatcher resolves these via `.provekit/lift/<surface>/manifest.toml` per peer's directory.

The apex demonstration:

```sh
$ cd implementations/rust  && provekit mint
$ cd implementations/cpp   && provekit mint
$ cd implementations/go    && provekit mint
```

One Rust CLI binary. Three `.proof` files. Configuration via `.provekit/config.toml` per directory. Same protocol catalog. Same foundation key. Different surfaces, different IR contents, byte-deterministic content-addressed output.

### TypeScript: kit + toolchain, not CLI

TypeScript is not a CLI peer in this protocol. It ships as a library, a kit (the authoring API mirrored from Rust), and a supported toolchain (vitest plugin, Zod adapter, class-validator adapter, fast-check adapter, JSDoc lifter). JS/TS projects consume the kit programmatically and produce `.proof` bundles via their own test runner / build step.

The TS self-contracts CID is produced by vitest:

```sh
$ pnpm vitest run \
    implementations/typescript/src/bin/mint-ts-self-contracts.test.ts
```

The test calls `runMintSelfContracts(outDir)` and asserts the CID matches the pinned value. This is the toolchain-native invocation. There is no `provekit-ts` CLI; the `provekit` binary is exclusively Rust.

A third-party TS plugin for `provekit mint` could be written by anyone (wrap `runMintSelfContracts` behind an NDJSON loop in Node, ship a manifest pointing at it). ProvekIt itself doesn't ship one. The lift-plugin protocol is open for any language; the reference set ships three CLI plugins because that's what's needed for the apex demo, not because TS is excluded.

This is the architectural division ProvekIt enforces: the CLI is one thing in one language; everything else is library, kit, toolchain, and (optionally) a plugin.
