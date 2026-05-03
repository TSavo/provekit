# Linker Daemon Protocol

**Status:** v1.0.0 normative spec
**Date:** 2026-05-04
**Companion specs:** `2026-05-03-bridge-linkage-protocol.md`, `2026-04-30-lsp-protocol.md`
**Companion docs:** `docs/launch/the-pieces-on-the-table.md`, `docs/launch/path-to-default.md`

---

Terms: MUST, SHALL, SHOULD, MAY per RFC 2119.

## §0. Motivation

The bridge-linkage protocol (`2026-05-03-bridge-linkage-protocol.md`) specifies what the linker computes: bridges derived from `(contracts ∪ call-edges)`, emitted as a `LinkBundle` with a rank-3 pin `(contractSetCid, callEdgeSetCid, bridgeSetCid)`. The question that spec defers: where does the linker run.

Embedding the linker inside each per-kit LSP plugin is Option A. Option A is rejected because it forces every kit to carry a multi-MB linker library, replicates the cross-language cache per-kit, and requires a migration to a shared process once memory or cross-kit cache locality becomes a concern. The migration cost is certain; paying it later costs more.

Option B is a long-running daemon, `provekit-linkerd`, that all per-kit LSP plugins connect to as thin protocol clients. The daemon holds the union of all kits' contract and call-edge streams hot in memory. Per-kit LSP plugins forward each `textDocument/didChange` event as a `parseFile` RPC; the daemon re-derives affected bridges, updates its cache, and returns diagnostics. Cross-language linkage errors are visible in every kit's IDE pane because the daemon owns the complete union.

This spec defines the wire protocol between per-kit LSP plugins (clients) and `provekit-linkerd` (daemon).

## §1. Definitions

**LinkerDaemon** -- the `provekit-linkerd` binary process. One process per project.

**Project** -- scoped by a content-addressed project CID, not a path string. The project CID is `blake3-512(JCS(<sorted list of kit-source-roots from the project manifest>))`. The daemon MAY serve multiple concurrent projects; each project's state is isolated by projectCid.

**Client** -- a per-kit LSP plugin process connecting via JSON-RPC 2.0.

**KitId** -- one of: `rust`, `go`, `cpp`, `csharp`, `python`, `ruby`, `swift`, `ts`, `zig`, `java`, `c`.

**LinterError** -- a linker-error memento (per `2026-05-03-bridge-linkage-protocol.md` R3) scoped to a file path and source range.

## §2. Connection

**R1. Transport.** The daemon SHALL bind a Unix domain socket at `${XDG_RUNTIME_DIR}/provekit/linkerd-<projectCid>.sock` on Linux and macOS. On Windows the daemon SHALL bind a named pipe at `\\.\pipe\provekit-linkerd-<projectCid>`. No TCP or UDP listener is permitted.

**R2. Permissions.** The socket file SHALL be created with mode 0600, owner-only. The daemon MUST reject connections from any peer whose effective UID differs from the daemon owner's UID.

**R3. Encoding.** All messages SHALL be NDJSON (newline-delimited JSON), one JSON-RPC 2.0 message per line. Framing is newline; no length prefix.

**R4. Lifecycle.** The daemon SHALL start on first client connect (spawned by the connecting client if not already running). The daemon SHALL shut down after 5 minutes with zero connected clients, writing a cache snapshot before exit (see §5). A client that connects to a stopped daemon MUST spawn it before sending requests.

## §3. Methods

**R5. `parseFile`.** The daemon SHALL accept:

```json
{
  "jsonrpc": "2.0",
  "id": <id>,
  "method": "parseFile",
  "params": { "kitId": <KitId>, "file": <absolute path>, "source": <file contents> }
}
```

The daemon SHALL update the kit's contract and call-edge streams from the lifted content of `source`, re-derive any affected bridges per `2026-05-03-bridge-linkage-protocol.md` R2, and return:

```json
{ "jsonrpc": "2.0", "id": <id>, "result": { "diagnostics": [<LinterError>] } }
```

`parseFile` is idempotent: two calls with byte-identical `(kitId, file, source)` MUST produce byte-identical `diagnostics`.

**R6. `getDiagnostics`.** The daemon SHALL accept:

```json
{
  "jsonrpc": "2.0",
  "id": <id>,
  "method": "getDiagnostics",
  "params": { "file": <absolute path> }
}
```

The daemon SHALL return current linker-error mementos for the named file without re-lifting. Used by LSP plugins refreshing diagnostics without a source change.

**R7. `projectStatus`.** The daemon SHALL accept:

```json
{ "jsonrpc": "2.0", "id": <id>, "method": "projectStatus", "params": {} }
```

The daemon SHALL return the current rank-3 pin per `2026-05-03-bridge-linkage-protocol.md` R5:

```json
{
  "jsonrpc": "2.0",
  "id": <id>,
  "result": {
    "contractSetCid": <string>,
    "callEdgeSetCid": <string>,
    "bridgeSetCid":   <string>,
    "linkBundleCid":  <string>
  }
}
```

**R8. `flushCache`.** The daemon SHALL accept:

```json
{ "jsonrpc": "2.0", "id": <id>, "method": "flushCache", "params": {} }
```

The daemon SHALL invalidate all cached derivations for this project and return `{ "result": null }`. Used when the developer changes a manifest file or adds a kit.

**R9. `shutdown`.** The daemon SHALL accept:

```json
{ "jsonrpc": "2.0", "id": <id>, "method": "shutdown", "params": {} }
```

The daemon SHALL write a cache snapshot (§5), close the socket, and exit cleanly. Return `{ "result": null }` before closing.

## §4. Error Semantics

**R10.** The daemon SHALL return standard JSON-RPC 2.0 error objects:

| Code    | Meaning                                                    |
|---------|------------------------------------------------------------|
| -32601  | Method not found                                           |
| -32602  | Invalid params (malformed request)                         |
| -33001  | KitId not present in project manifest                      |
| -33002  | Kit lifter unavailable (language toolchain not installed)  |
| -33003  | Linker discharge failure (z3 unavailable or timed out)     |

**R11.** All errors are non-fatal. The daemon SHALL continue serving subsequent requests after returning any error response.

## §5. Cache

**R12.** The daemon SHALL maintain a per-kit LRU cache keyed by `(contractSetCid, callEdgeSetCid)` mapping to derived bridge sets. The default cap is 1024 entries; implementors MAY make this configurable.

**R13.** Cache correctness invariant: a cache hit MUST be byte-identical to a fresh derivation over the same inputs. Because the keys are content-addressed, this invariant is mechanical: the same inputs produce the same outputs deterministically.

**R14. Warm-start snapshot.** On shutdown (graceful or idle-timeout), the daemon SHALL write its cache to `${XDG_CACHE_HOME}/provekit/linkerd/<projectCid>/snapshot.bin`. On start, if a snapshot exists, the daemon SHALL load it before accepting connections. Snapshot format is implementation-defined; snapshot integrity MUST be verified by blake3-512 checksum before use.

## §6. Multi-Project

**R15.** Each project gets one daemon instance, bound to `linkerd-<projectCid>.sock`. Concurrent projects run as concurrent daemon instances. Project state is not shared between daemons. Content-addressing makes this safe: different projects have different projectCids and therefore different contract and call-edge namespaces.

## §7. Authentication

**R16.** The Unix socket at mode 0600 is the authentication boundary. The owning user is the only permitted principal. No tokens, no capabilities, no cross-user IPC. No network listener of any kind.

## §8. Conformance

A `provekit-linkerd` implementation conforms to this spec if all of the following hold:

1. All five methods (R5 through R9) implement the documented request and response shapes.
2. `parseFile` is idempotent: two calls with byte-identical `(kitId, file, source)` return byte-identical `diagnostics`.
3. Two clients connecting concurrently to the same project socket see consistent diagnostic streams (no torn reads from concurrent `parseFile` calls on different files).
4. `projectStatus().linkBundleCid` is byte-identical across two `parseFile` sequences that produce the same `(contractSetCid, callEdgeSetCid)`.
5. LRU eviction (R12) does not affect output correctness, only cache hit or miss timing.
6. Error codes R10 are returned for the named conditions; the daemon does not exit on any of them (R11).
7. The socket is created at mode 0600 (R2) and the daemon rejects connections from non-owner UIDs.

## §9. Architectural Framing

The daemon is what makes the LSP+linker pair work at typing speed. Per-kit LSP plugins are thin protocol adapters: they translate `textDocument/didChange` events to `parseFile` RPCs and translate `LinterError` responses back to LSP diagnostics. The daemon owns the cross-language brain: it holds the union of all kits' streams in memory, caches derived bridges by content-addressed keys, and re-derives only what changed.

The substrate's three primitives (sign, hash, reference) plus the bridge-linkage derivation (`substrate-not-blockchain.md` §10) plus the daemon's LRU cache together achieve typing-speed LSP feedback for cross-language contract errors. No new substrate primitives are introduced. The daemon is operational infrastructure over the existing substrate.
