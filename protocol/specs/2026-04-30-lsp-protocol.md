# ProvekIt LSP Protocol

**Content-addressed verification as a language server.**

The ProvekIt Language Server Protocol (LSP) implementation provides real-time contract verification, bridge validation, and cross-domain proof transfer directly in the developer's IDE. It treats formal contracts as first-class IDE features — comparable to TypeScript's type checker, but for behavioral properties.

## Architecture

The language server is a **pluggable wrapper** around a JSON-RPC-capable `provekit` backend. It does not reimplement verification logic; it delegates to a configurable backend via JSON-RPC and translates responses to LSP messages.

```
IDE (VS Code, Neovim, Emacs)
  ↓ LSP messages
ProvekIt Language Server (per language, swappable)
  ├── Text Document Synchronization (open/change/close)
  ├── Annotation Extraction (#[provekit::implement], etc.)
  ├── Position Mapping (line/col ↔ symbol)
  └── JSON-RPC Invocation (configurable backend)
        ↓
ProvekIt Backend (pluggable verifier)
  ├── Canonical Rust CLI (default)
  ├── Custom fork with custom solvers
  ├── Remote verifier over TCP/Unix socket
  └── Mock backend for testing
        ↓
Language Server
  ├── Parses JSON-RPC result
  ├── Maps to IDE positions
  └── Publishes diagnostics/hovers/lenses
```

### Why JSON-RPC makes the server pluggable

The language server communicates with the backend via **JSON-RPC over stdio** (line-delimited NDJSON). This is the same protocol already used by:

- `provekit-lift --rpc` (lift plugins)
- `provekit-ir-compiler-smt-lib` (compiler subprocesses)
- `provekit-self-contracts --rpc` (self-contract minting)

Because the boundary is JSON-RPC, **both sides are independently swappable**:

| Swap | What changes | What stays |
|---|---|---|
| **Backend** | Point `provekit.path` to a different binary | Same LSP server, same IDE features |
| **LSP server** | Use a custom parser or different IDE features | Same backend, same verification |
| **Both** | Custom LSP + custom backend for a specialized domain | Same JSON-RPC contract |

### Runtime configuration

The language server reads `.provekit/config.toml` at workspace root:

```toml
[server]
# Which backend to spawn for verification
backend = "provekit"  # default: looks up in PATH
# backend = "/path/to/custom/provekit"
# backend = "tcp://remote-verifier.example.com:8080"
# backend = "unix:///var/run/provekit.sock"

# Backend arguments passed on every invocation
backend_args = ["verify", "--format", "json"]

# Timeout for verification queries (milliseconds)
timeout_ms = 5000

# Cache directory for verification results
cache_dir = ".provekit/cache"
```

### Backend contract

Any backend binary must speak JSON-RPC over stdio:

**Handshake (required):**
```json
{"jsonrpc":"2.0","id":1,"method":"provekit.lsp.handshake","params":{"provekit_version":"1.1.0","protocol_version":"lsp-1.0"}}
```

**Verify (core method):**
```json
{"jsonrpc":"2.0","id":2,"method":"provekit.lsp.verify","params":{"file":"src/lib.rs","function":"my_parse_int","target_cid":"bafy...","workspace":"/project"}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":2,"result":{"status":"verified","transfers":[],"evidence":{}}}
```

A backend that implements these three methods (handshake, verify, and optionally `provekit.lsp.resolve_cid` for bundle loading) is a valid ProvekIt LSP backend. No recompilation of the language server needed.

### Why this matters

- **Custom solvers:** A research team writes a backend that uses a custom SMT solver. They point `backend = "/usr/local/bin/provekit-custom"` in their config. Their team gets the same IDE experience with different verification.
- **Remote verification:** A CI system runs the heavy verifier on a GPU cluster. Developers point `backend = "tcp://ci-cluster.internal:9000"`. Their local IDE stays lightweight.
- **Mock backends:** Language server tests use `backend = "./mock-provekit"` that returns canned responses. No real Z3 needed for unit tests.
- **Specialized domains:** A blockchain team writes a backend that understands EVM semantics. They swap the backend; the IDE (diagnostics, hover, lenses) works unchanged.

The language server is **just the IDE glue**. The backend is **just the verifier**. The JSON-RPC boundary is the plugin surface. Both are independently deployable, independently versioned, and independently authored.

## Capabilities

The server advertises these LSP capabilities:

| Capability | Description |
|---|---|
| `textDocumentSync` | Full document sync |
| `hoverProvider` | Contract and bridge details on hover |
| `diagnosticProvider` | Real-time contract violation detection |
| `codeLensProvider` | "Verify" / "Show DAG" / "Transfer domains" lenses |
| `codeActionProvider` | Quick fixes for common contract violations |
| `inlayHintProvider` | Inline contract summaries (optional) |

## Messages

### `textDocument/didOpen` and `textDocument/didChange`

On document open or change, the server:
1. Parses the document with tree-sitter (or host-language parser)
2. Extracts `#[provekit::implement]`, `#[provekit::contract]`, `#[provekit::verify]` annotations
3. Resolves target contract CIDs against the `.proof` index
4. Queues background verification for new or changed functions

### `textDocument/hover`

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "textDocument/hover",
  "params": {
    "textDocument": { "uri": "file:///project/src/lib.rs" },
    "position": { "line": 42, "character": 12 }
  }
}
```

**Response (on function with `#[provekit::implement]`):**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "contents": {
      "kind": "markdown",
      "value": "## ✅ Contract Verified\n\n**Function:** `my_parse_int`\n**Implements:** `bafy...js-parseInt-v24`\n\n**Postcondition:**\n```ir\nparseInt(s) ≥ 0\n```\n\n**Cross-domain transfers:**\n- `js-parseInt-v24` → `ref-parseInt-v1` (ECMAScript reference)\n- `js-parseInt-v24` → `v8-parseInt-impl` (V8 implementation)\n\n**Evidence:** Z3 unsat (cached, minted 2026-04-30)\n\n[Show Proof DAG](command:provekit.showDag?bafy...) | [Re-verify](command:provekit.reverify?bafy...)"
    }
  }
}
```

**Response (on function with failing bridge):**
```json
{
  "result": {
    "contents": {
      "kind": "markdown",
      "value": "## ❌ Bridge Verification Failed\n\n**Function:** `my_parse_int`\n**Target:** `bafy...js-parseInt-v24`\n\n**Error:** Body does not satisfy contract postcondition\n\n**Counterexample:**\n```\ns = \"\"\n```\n\n**Analysis:** `s.parse().unwrap()` panics on empty string. Contract requires `parseInt(s) ≥ 0` for all `s`.\n\n**Suggestion:** Use `s.parse().unwrap_or(0)`\n\n[Apply Fix](command:provekit.applyFix?) | [Show Z3 Trace](command:provekit.showTrace?) | [Ignore](command:provekit.ignore?)"
    }
  }
}
```

### `textDocument/publishDiagnostics`

Pushed from server to client when verification completes.

**Verified contract:**
```json
{
  "uri": "file:///project/src/lib.rs",
  "diagnostics": [
    {
      "range": { "start": {"line":42,"character":0}, "end": {"line":42,"character":20} },
      "severity": 4,  // Hint
      "code": "provekit.verified",
      "source": "provekit",
      "message": "✅ Bridge verified: my_parse_int → js-parseInt-v24 (3 domain transfers)",
      "relatedInformation": [
        {
          "location": {
            "uri": "file:///project/target/provekit/bafy....proof",
            "range": { "start": {"line":0,"character":0}, "end":{"line":0,"character":0} }
          },
          "message": "Contract definition in @types/node-v24.proof"
        }
      ]
    }
  ]
}
```

**Contract violation:**
```json
{
  "uri": "file:///project/src/lib.rs",
  "diagnostics": [
    {
      "range": { "start": {"line":45,"character":4}, "end": {"line":45,"character":28} },
      "severity": 1,  // Error
      "code": "provekit.violation",
      "source": "provekit",
      "message": "❌ Contract violation: parseInt(s) ≥ 0 fails for s = \"\" (panic)",
      "relatedInformation": [
        {
          "location": {
            "uri": "file:///project/src/lib.rs",
            "range": { "start": {"line":42,"character":0}, "end":{"line":42,"character":20} }
          },
          "message": "Target contract: bafy...js-parseInt-v24"
        }
      ]
    }
  ]
}
```

**Unresolved bridge target:**
```json
{
  "severity": 2,  // Warning
  "code": "provekit.unresolved-target",
  "message": "⚠️ Target proof not found: bafy...nonexistent-contract",
  "relatedInformation": [
    {
      "location": {
        "uri": "file:///project/src/lib.rs",
        "range": { "start": {"line":42,"character":0}, "end":{"line":42,"character":20} }
      },
      "message": "No .proof bundle in dependency tree contains this CID"
    }
  ]
}
```

### `textDocument/codeLens`

Provides action buttons above annotated functions.

```json
{
  "range": { "start": {"line":42,"character":0}, "end": {"line":47,"character":1} },
  "command": {
    "title": "✅ Verified (3 domains)",
    "command": "provekit.showDag",
    "arguments": ["bafy...js-parseInt-v24"]
  }
}
```

For unverified functions:
```json
{
  "range": { "start": {"line":42,"character":0}, "end": {"line":47,"character":1} },
  "command": {
    "title": "⚠️ Verify",
    "command": "provekit.verify",
    "arguments": ["my_parse_int", "bafy...js-parseInt-v24"]
  }
}
```

### `textDocument/codeAction`

Quick fixes for common violations.

**Request:**
```json
{
  "textDocument": { "uri": "file:///project/src/lib.rs" },
  "range": { "start": {"line":45,"character":4}, "end": {"line":45,"character":28} },
  "context": { "diagnostics": [{"code":"provekit.violation"}] }
}
```

**Response:**
```json
{
  "actions": [
    {
      "title": "Replace unwrap() with unwrap_or(0)",
      "kind": "quickfix",
      "diagnostics": [{"code":"provekit.violation"}],
      "edit": {
        "changes": {
          "file:///project/src/lib.rs": [
            {
              "range": { "start": {"line":45,"character":4}, "end": {"line":45,"character":28} },
              "newText": "s.parse().unwrap_or(0)"
            }
          ]
        }
      }
    },
    {
      "title": "Add precondition: require non-empty string",
      "kind": "quickfix",
      "edit": {
        "changes": {
          "file:///project/src/lib.rs": [
            {
              "range": { "start": {"line":43,"character":0}, "end": {"line":43,"character":0} },
              "newText": "    assert!(!s.is_empty());\n"
            }
          ]
        }
      }
    },
    {
      "title": "Ignore this contract violation",
      "kind": "quickfix",
      "command": {
        "command": "provekit.ignoreViolation",
        "arguments": ["bafy...js-parseInt-v24", "s.parse().unwrap()"]
      }
    }
  ]
}
```

### `workspace/executeCommand`

Server-side commands invoked by client actions.

**Command: `provekit.verify`**
- Input: `(functionName: string, targetCid: string)`
- Action: Run `provekit verify` on the function body against the target contract
- Result: Publish diagnostics with verification result

**Command: `provekit.showDag`**
- Input: `(cid: string)`
- Action: Compute transitive closure of bridges from CID
- Result: Open webview panel showing interactive DAG visualization

**Command: `provekit.reverify`**
- Input: `(cid: string)`
- Action: Invalidate cache and re-run verification
- Result: Publish fresh diagnostics

## Background Verification

The language server runs verification **asynchronously** to avoid blocking the IDE:

1. **Fast path** (synchronous): CID resolution, bridge graph lookup, cached result check
2. **Slow path** (async, cancellable): Z3 invocation, body lifting, proof minting

When the user types, the server:
1. Cancels any in-flight verification for the changed document
2. Re-parses the document (incremental if supported)
3. Queues verification for affected functions
4. Publishes diagnostics when results arrive

The verification queue is **prioritized**:
1. Functions with `#[provekit::implement]` (highest — explicit contract)
2. Functions with `#[provekit::verify]` (high — need verification)
3. Functions with `#[provekit::contract]` (medium — may affect callers)
4. Call sites to verified functions (low — inherited verification)

## Workspace Indexing

On workspace open, the server:

1. **Scans for `.proof` files:**
   ```bash
   find <workspace> -name "*.proof" -type f
   ```

2. **Loads each bundle:**
   - Verify filename matches content hash
   - Verify catalog signature
   - Verify `binaryCid` matches running binary (if present)
   - Index all members by CID
   - Index bridges by source symbol and source contract CID

3. **Scans for source annotations:**
   - Tree-sitter parse of all source files
   - Extract `#[provekit::implement]`, `#[provekit::contract]`, `#[provekit::verify]`
   - Build symbol → contract CID mapping

4. **Builds bridge graph:**
   - Nodes: contract CIDs
   - Edges: BridgeDeclaration (source → target)
   - Compute transitive closure lazily (on first access)

## Inlay Hints (Optional)

Show contract summaries inline:

```rust
fn parse_int(s: &str) -> i64  /* parseInt(s) ≥ 0 */ {
    s.parse().unwrap_or(0)
}
```

```json
{
  "inlayHints": [
    {
      "position": { "line": 42, "character": 30 },
      "label": "parseInt(s) ≥ 0",
      "kind": 1,  // Type hint
      "paddingLeft": true,
      "paddingRight": true
    }
  ]
}
```

## CLI Invocation Format

The language server invokes the canonical CLI for all verification. The CLI is the single implementation of the verifier; the LSP never reimplements verification logic.

### `provekit verify --format json`

**Input:** The CLI reads the workspace, discovers `.proof` bundles, and verifies the specified function.

**Invocation:**
```bash
provekit verify \
  --function my_parse_int \
  --target-cid bafy...js-parseInt-v24 \
  --file src/lib.rs \
  --workspace /project \
  --format json
```

**Output:**
```json
{
  "status": "verified",
  "function": "my_parse_int",
  "targetCid": "bafy...js-parseInt-v24",
  "transfers": [
    {"domain": "reference", "cid": "bafy...ref-parseInt-v1"},
    {"domain": "javascript", "cid": "bafy...js-parseInt-v24"}
  ],
  "evidence": {
    "producer": "z3@4.13",
    "timeMs": 42,
    "smtLib": "(set-logic QF_LIA)..."
  }
}
```

### `provekit verify --format json` (failure)

```json
{
  "status": "violation",
  "function": "my_parse_int",
  "targetCid": "bafy...js-parseInt-v24",
  "error": "Body does not satisfy contract postcondition",
  "counterexample": {
    "s": ""
  },
  "suggestion": "Use s.parse().unwrap_or(0)"
}
```

The language server parses this JSON and maps it to LSP diagnostics. The CLI is the single source of truth; the LSP is just the transport layer.

## Implementation Notes

### Performance

- The language server is thin; all heavy work happens in the CLI
- CLI results are cached by `(body_hash, target_cid)` in the language server
- CLI timeout: default 5s per query, configurable per workspace
- Large workspaces: the CLI handles `.proof` discovery and indexing; the LSP just passes file paths

### Security

- The LSP runs with user permissions
- It does NOT execute code from `.proof` bundles (metadata is decorative)
- It does NOT fetch external content (no network access for verification)
- All verification is local: the LSP only reads files already on disk

### Fallback

If the language server is unavailable, the IDE falls back to:
- Syntax highlighting from tree-sitter grammar
- Basic hover (just the annotation text)
- No real-time verification (run `provekit prove` manually)

## Protocol Version

This spec is v1.0.0 of the ProvekIt LSP protocol. Future versions add:
- v1.1.0: Folding ranges for nested contract scopes
- v1.2.0: Workspace symbols (search contracts by name/CID)
- v1.3.0: Call hierarchy (show all implementations of a reference contract)
- v2.0.0: Multi-root workspaces (verify across multiple projects)

## Read further

- `protocol/specs/2026-04-30-ir-formal-grammar.md` — IR grammar for contract expressions
- `protocol/specs/2026-04-30-proof-file-format.md` — `.proof` bundle format
- `protocol/specs/2026-04-30-handshake-algorithm.md` — verification handshake
- `docs/per-language-status.md` — LSP implementation status per language
