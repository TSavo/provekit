# IR Compiler Protocol (`provekit-ir-compiler/1`)

Status: draft for inclusion in protocol catalog v1.2.0 (the v1.1.0
catalog is signed and frozen; this spec is RECOMPUTE-pending until
v1.2.0 lands).

## Why

ProvekIt's verifier consumes a canonical IR-JSON formula and ships it
to a solver. Each solver speaks its own surface syntax: SMT-LIB v2.6
for Z3 / CVC5, the bitvector fragment for Bitwuzla, dReal's
delta-precision extension, TPTP for the Vampire / E / iProver lineage,
Lean tactic mode, Coq's Gallina, Isabelle/HOL.

The translator from canonical IR-JSON to solver-native syntax is the
**IR compiler**. ProvekIt does not own one per solver; it owns the
protocol that lets any compiler plug in.

This spec defines:

1. The plugin manifest format.
2. The JSON-RPC method shapes.
3. The dialect registry (initial entries + extension procedure).
4. Capability negotiation.
5. The error model.
6. Bootstrapping: the bundled `smt-lib-v2.6` compiler doubles as both
   an in-Rust trait implementation and a standalone subprocess binary.

The protocol mirrors the agent plugin protocol
(`2026-04-30-agent-plugin-protocol.md`). Same JSON-RPC over stdio shape,
same manifest layout, same error code structure. A plugin author who
has shipped one knows how to ship the other.

## Plugin discovery

Plugins live at:

```
~/.config/provekit/ir-compilers/<name>/manifest.toml
```

Missing directory is **not** an error. The dispatcher walks the path
if it exists and registers each child directory whose `manifest.toml`
parses; otherwise the registry is empty and only built-in compilers
are available.

Manifest schema:

```toml
name = "smt-lib-reference"
version = "0.1.0"
protocol_version = "provekit-ir-compiler/1"
binary = "provekit-ir-smt-lib"           # absolute path or PATH-resolvable
dialects = ["smt-lib-v2.6"]              # the dialect names this binary serves
```

`protocol_version` must match the catalog declared by the running
ProvekIt CLI; mismatch is a hard error reported by `provekit ir-compiler list`.

`dialects` enumerates the dialect identifiers this compiler claims to
serve. The dispatcher builds a `dialect -> binary` index from the union
of all manifests; the first manifest claiming a dialect wins. The CLI
warns on duplicate claims and uses the first.

## JSON-RPC methods

All requests are line-delimited JSON-RPC 2.0 over the plugin's stdin;
all responses go to stdout. Any non-JSON output on stdout is a hard
error. Logging belongs on stderr.

### `provekit.ir.handshake`

The first call. Establishes capability + version compatibility.

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "provekit.ir.handshake",
  "params": {
    "provekit_version": "0.1.0",
    "protocol_version": "provekit-ir-compiler/1",
    "catalog_cid": "blake3-512:..."
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "name": "smt-lib-reference",
    "version": "0.1.0",
    "protocol_version": "provekit-ir-compiler/1",
    "dialects": ["smt-lib-v2.6"],
    "supported_sorts": ["Int", "Bool", "Real", "String"],
    "supported_predicates": [
      "=", "distinct", "<", "<=", ">", ">=",
      "and", "or", "not", "implies",
      "forall", "exists"
    ]
  }
}
```

A verifier inspects the handshake response to decide, before invoking
`compile`, whether the given compiler can handle the sorts and
predicates appearing in a particular formula. The verifier may consult
multiple compilers and pick one; it never falls back silently after a
`compile_error.unsupported_*` failure.

### `provekit.ir.compile`

Translate one canonical IR-JSON formula to the target dialect.

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "provekit.ir.compile",
  "params": {
    "ir_json": { "kind": "atomic", "name": ">", "args": [
      { "kind": "var", "name": "x" },
      { "kind": "const", "value": 0,
        "sort": { "kind": "primitive", "name": "Int" } }
    ]},
    "target_dialect": "smt-lib-v2.6"
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "preamble": "(set-logic ALL)\n(declare-const x Int)\n",
    "body": "(assert (not (> x 0)))\n(check-sat)\n",
    "free_vars": [
      { "name": "x", "sort": "Int" }
    ]
  }
}
```

Contract:

- `preamble + body` is the complete script that the verifier hands to
  the solver. Compilers MUST NOT split a logically required prelude
  across the boundary; everything the solver needs before the
  obligation goes in `preamble`.
- `body` contains the obligation assertion and the terminator
  (`(check-sat)` for SMT-LIB, the equivalent driver for other
  dialects). Splitting at this boundary lets the verifier inject extra
  declarations or context after the preamble without re-parsing.
- `free_vars` is the list of variables the compiler had to declare in
  `preamble`. Each element is `{ "name": string, "sort": string }`
  where `sort` is the dialect-native sort identifier as it appears in
  the preamble.

The IR-JSON input MUST be canonical per
`protocol/specs/2026-04-30-ir-formal-grammar.md`. Compilers MAY
fail-closed on non-canonical input or pass it through; the spec does
not require canonicalization on the compiler side.

### `provekit.ir.shutdown`

Graceful close. After this, the plugin process should exit zero on
stdin EOF.

## Dialect registry

The initial registry. Only `smt-lib-v2.6` ships as a real
implementation in v1.2.0; the rest are **reserved** identifiers so that
future plugins do not collide on naming.

| dialect              | description                                                    | bundled? |
|----------------------|----------------------------------------------------------------|----------|
| `smt-lib-v2.6`       | Standard SMT-LIB v2.6 — Z3, CVC5.                              | yes      |
| `smt-lib-v2.6-bv`    | Bitvector-only fragment of SMT-LIB v2.6 — Bitwuzla.            | no       |
| `smt-lib-v2.6-delta` | SMT-LIB v2.6 + dReal's `delta` precision extension.            | no       |
| `tptp-fof`           | TPTP first-order form — Vampire, E, iProver.                   | no       |
| `tptp-thf`           | TPTP higher-order form — Leo-III, Satallax.                    | no       |
| `lean-tactic-mode`   | Lean / mathlib tactic-mode obligations.                        | no       |
| `gallina`            | Coq / Rocq Gallina syntax.                                     | no       |
| `isabelle-hol`       | Isabelle/HOL.                                                  | no       |

New dialects are added to this table by amending this spec; the
manifest's `dialects = [...]` list is the on-disk interface and any
string is allowed there. Verifiers reject dialect names not in the
registry table.

## Error model

JSON-RPC 2.0 error codes plus ProvekIt extensions:

| code  | symbolic name                            | meaning                                                          |
|-------|------------------------------------------|------------------------------------------------------------------|
| -32700| `parse_error`                            | Non-JSON on stdout.                                              |
| -32600| `invalid_request`                        | Missing or malformed JSON-RPC envelope.                          |
| -32601| `method_not_found`                       | Plugin does not implement the requested method.                  |
| -32602| `invalid_params`                         | Validation failed before dispatch (e.g. missing `target_dialect`). |
| -32603| `internal_error`                         | Plugin crashed during handling.                                  |
| 2000  | `compile_error.unsupported_dialect`      | The plugin does not serve the requested `target_dialect`.        |
| 2001  | `compile_error.unsupported_sort`         | The IR uses a sort the dialect cannot express. `data.sort` names it. |
| 2002  | `compile_error.unsupported_predicate`    | The IR uses an atomic predicate the dialect cannot express. `data.predicate` names it. |
| 2003  | `compile_error.malformed_ir`             | The IR-JSON does not parse against the formal grammar.           |
| 2004  | `compile_error.internal`                 | Compiler bug. Recoverable by the caller only via switching compilers. |

`unsupported_sort` and `unsupported_predicate` are the contract for
verifier fallback. A verifier seeing one of those errors MAY consult a
different compiler that listed broader `supported_sorts` /
`supported_predicates` in handshake. Any other 2xxx code is terminal:
the verifier reports it and stops.

## Capability negotiation

The verifier performs handshake at compiler load time and caches the
result keyed by `(binary_path, mtime)`. Before invoking `compile` for
a given formula, the verifier:

1. Walks the formula collecting the set of sorts and atomic predicates
   it uses.
2. Checks each against the cached `supported_sorts` /
   `supported_predicates` for the configured compiler.
3. If any are missing, fails fast with the same `compile_error.unsupported_*`
   shape the compiler itself would have returned, without spawning the
   subprocess.

This is an optimization. The compiler is still authoritative; the
verifier MUST NOT silently drop predicates it thinks are unsupported.

## Bootstrapping

The bundled `smt-lib-v2.6` compiler ships in two faces:

1. **In-process trait implementation.** Crate
   `provekit-ir-compiler-smt-lib` exports a struct
   `SmtLibCompiler` that implements the `IrCompiler` trait from
   `provekit-ir-compiler`. The verifier crate depends on it directly
   for the fast path (no subprocess, no JSON-RPC framing cost). This
   replaces the inline `provekit_verifier::smt_emitter` module; the
   verifier re-exports the new emitter under the same path so the
   runner does not have to change.

2. **Standalone subprocess binary.** The same crate produces
   `provekit-ir-smt-lib`, a binary that speaks the JSON-RPC protocol
   defined here. It is the conformance reference for plugin authors
   in any language.

Both faces share the same emit code; there is exactly one SMT-LIB
emitter implementation in the workspace. A regression test
(`tests/byte_for_byte.rs` in the smt-lib crate) constructs a fixture IR
and asserts that `<preamble> + <body>` from the trait equals the
single string the historical inline emitter produced, byte-for-byte.

A verifier integrating this protocol thus pays no overhead for the
common case (Z3 + SMT-LIB) and pays one fork+exec per compile call
for the plugin case.

## Reference plugin

`examples/ir-compiler-plugins/echo-compiler/` is a Python plugin
implementing the protocol with canned responses for a hypothetical
`echo` dialect that just stringifies the IR-JSON. Demonstrates that
plugin authoring is under 100 lines in any language.

## Related specs

- `protocol/specs/2026-04-30-ir-formal-grammar.md` — the input grammar.
- `protocol/specs/2026-04-30-agent-plugin-protocol.md` — sister
  protocol, same shape, different domain.
- `protocol/specs/2026-04-30-protocol-catalog.json` — the signed
  catalog this protocol joins in v1.2.0.
