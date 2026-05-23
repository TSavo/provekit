# Substrate-symmetric cycle scorecard (no @substrate-term-shape sidecar)

**STATUS (2026-05-23): 10/10 strict byte-identical.** All ten @sugar functions in
`libprovekit-rpc-cross-platform/src/lib.rs` round-trip through rust→java→rust
byte-identical after rustfmt. Verified end-to-end via:
`provekit lift | provekit lower --target java | java lift CLI | provekit-realize-rust --rpc`
followed by per-function rustfmt comparison.

```
run_server              ✓ 0b
handle_line             ✓ 0b
initialize_result       ✓ 0b
lift                    ✓ 0b
build_ir_document       ✓ 0b
content_addressed_name  ✓ 0b
slot_cid                ✓ 0b
blake3_512_cid          ✓ 0b
ok_response             ✓ 0b
error_response          ✓ 0b
```

Substrate carriers added across this work:
- `let_type` end-to-end (concept:assign target leaves)
- `concept:blank-line` (lifted from rust source via syn::Spanned, java lift via
  JavaParser begin/end lines, emitted as empty line both sides)
- `/*@ref*/`, `/*@ref-mut*/` markers for `&x` / `&mut x` preservation
- `/*@for-mut*/`, `/*@for-ref*/` markers for for-each var pattern preservation
- `/*@if-let-variant=*/` marker with java-17 instanceof-pattern lowering
- `/*@map-insert*/`, `/*@set-insert*/` markers
- `/*@unwrap-or-else-marker*/` for closure-arg preservation in unwrap_or_else
- `/*@match-arm-pattern=*/` carrier for variant-arm discrimination
- `doc_lines` plumbed envelope-style through @substrate-signature
- Tuple-return signature recovery via `source_return_type` envelope + concept:array-literal
- `provekit-realize-rust-core::function_source`: rustfmt-then-reindent macro
  bodies (rustfmt on stable doesn't reformat macro internals; we shell out
  to rustfmt and re-indent macro args based on the macro call's final column)

Path: substrate-symmetric (no @substrate-term-shape sidecar). All round-trip
information flows through either structural concept shapes OR through
substrate-signature envelope fields (which are themselves part of the
substrate, not opaque sidecars).

---

Date: 2026-05-22 — after path B (#1391, catalog-driven operation realizations).

## Path B — catalog-driven dispatch (#1391)

Architectural landing:
- 18 operation-realization mementos minted (6 concepts × 3: abstraction + rust +
  java realizations). Catalog at `menagerie/concept-shapes/catalog/realizations/`.
- `OperationRealizationCatalog` (Java, provekit-ir) + `operation_realization_catalog`
  (Rust, realize-rust-core): forward (concept → kit-op) + reverse (kit-op → concept)
  lookups; OnceLock/synchronized-cached on first call.
- SugarRealizer.lowerShapeExpression / realize-rust-core::lower_term_shape_expression:
  hardcoded `if conceptName == "concept:utf8-encode"` chains REPLACED with catalog
  lookup → per-kit-op emitter. Kit-op emitters keyed by `java:string-getBytes-utf8`,
  `rust:vec-new`, etc — catalog'd names, not concept-hub names.
- TermShapeLifter + walk_rpc.rs MethodCall/Call recognizers: reverse-lookup catalog
  whenever the matcher recognizes an AST pattern.

Empirical evidence the catalog is load-bearing:
- Java lift of `examples/provekit-rpc-java-demo/src/main/java/CrossPlatformRpc.java`
  emits `concept:utf8-encode` 4× via reverse lookup. Source: `.getBytes(
  StandardCharsets.UTF_8)` appears 4× in the file.
- Round-trip tests on both sides pass against on-disk catalog:
    Java: `OperationRealizationCatalogTest.operationRealizationCatalogRoundTrips`
    Rust: `provekit_realize_rust_core::tests::operation_realization_catalog_round_trips`

## Cycle measurement (post path B + #1391 follow-ons)

Harness fixes applied this run:
- `cmd_lower::resolve_library_for_concept` now falls back to
  `legacy_realize_candidates` when the sealed registry is empty (previously
  returned None, dropping the `--library` flag silently).
- `realize-rust-core::function_source` now passes return_type + param_types
  through `map_source_type` when they look cross-language (contain a dot, equal
  `JsonNode`, or start with `Result<`).
- `map_source_type` extended with Jackson + java.util + provekit.runtime FQN
  translations + a parametric `Result<X,Y>` decomposer.

End-to-end cycle (rust source → `provekit lift` → `provekit lower --target java`
→ java source → java lift CLI → `provekit-realize-rust --rpc` per term) on
`libprovekit-rpc-cross-platform/src/lib.rs`:

| function | strict (rustfmt) | diff |
|---|---|---|
| ok_response             | ✓ | 0b |
| error_response          | ✓ | 0b |
| initialize_result       | ✓ | 0b |
| content_addressed_name  | ✓ | 0b |
| slot_cid                | ✓ | 0b |
| blake3_512_cid          | ✓ | 0b |
| run_server              |   | -42b (.unwrap_or_else closure preservation, residual only) |
| lift                    |   | -206b (two `ok_or_else(\|\| ...)?` propagation chains lost in java) |
| build_ir_document       |   | +3b (type annot + if-let destructure + chain transform) |
| handle_line             |   | -536b (tuple return + nested match-arm-guard partial recovery) |

**SUBSTRATE-SYMMETRIC STRICT: 6/10**

Improvements landed this run (six progressive commits):
1. Expression-position catalog dispatch — closes content_addressed_name, slot_cid.
2. Tolerant seq lowering + empty-RHS placeholder — un-stubs handle_line.
3. Item-decl round-trip via java line-comment recognition (`// item-decl
   (rust): X` → concept:item-decl(symbol{text:X})) — closes the const-HEX
   gap in blake3_512_cid.
4. Hex literal radix preservation through both edges — closes the
   `0x0F` vs `15` byte-difference.
5. Ref-pattern inference: detect primitive-use of the loop var in body
   (`b >> 4`, `b & 0x0F`, `b as char`) and emit `for &b in &raw` —
   closes the iter-borrow gap in blake3_512_cid.
6. Callee-signature-aware `&` insertion: known-callees registry for
   @boundary/@sugar functions in this source. Guarded against
   double-borrow when caller already passes by reference (skips args
   that name a caller param of canonical reference-type Value/str).
   Closes most of run_server's `&` gaps. (was effectively 0/10 — pre-#1391 numbers
in the baseline below were ad-hoc per-function diffs without round-trip
verification; the 3/10 here is verified end-to-end via real RPC + rustfmt).

Remaining residuals are bounded:
- TWO functions (`handle_line`, `slot_cid`) are JAVA-LOWER refusals; the java
  realizer emits stubs. Closing them needs richer java vocabulary (tuple
  return, match-guard).
- The other FIVE failures are lift-side gaps where rust lift drops source
  information the realize side then can't reconstruct: mutability markers,
  ref-patterns in `for`, function-local `const` decls, hex vs decimal
  literals, the `concept:assign` chain mid-body.

Each gap is a named pattern, not an architectural unknown.

## Pre-#1391 baseline (manual ad-hoc measurement)


## Method
Strip @substrate-term-shape block comments from the substrate-emitted
java; lift via syntax-driven path only; lower back to rust; compare
to original.

## Result on libprovekit-rpc-cross-platform (10 @sugar functions):

| function | diff | status |
|---|---|---|
| ok_response       | +8b  | NEAR — single tail-vs-return wrapper diff |
| error_response    | +9b  | NEAR — same shape as ok_response |
| initialize_result | +328b | partial — nested json! reconstructs but not byte-identical |
| slot_cid          | -87b  | partial — match works, body reconstruction loses some detail |
| content_addressed_name | -259b | substantial — method chains still asymmetric |
| blake3_512_cid    | -261b | substantial — for-loop + indexing patterns missing |
| handle_line       | -1097b | major — nested matches + Result destructure not yet symmetric |
| build_ir_document | -812b  | major — struct destructure + iteration chains not symmetric |
| lift              | -959b  | major — ?-operator chains + method chains not symmetric |
| run_server        | -582b  | major — while-let + complex closure body not symmetric |

Strict byte-identical: 0/10
Loss/refuse records: 7/10 (rust lower bails on un-recognized java body shapes)

## Closed recognizers
- json! macro Supplier-closure → concept:macro-call(json, body)
- match expression scrut+if/else → concept:match(scrut, arm1, arm2)

## Pending recognizers (each closes a slice)
- return-statement-at-fn-tail → strip outer concept:return wrapper
  (closes ok_response, error_response — currently +8/+9b)
- iterator chains: Stream.filter().map().collect() → rust .iter().filter_map().collect()
- struct destructure: getter sequence → concept:destructure-struct
- Result method chains: instanceof Ok/Err + .unwrap() → concept:try
- while-let with __raw → concept:while-let with explicit pattern

## What the data says
The substrate-symmetric closure path is real and bounded. Each recognizer
closes a specific slice. The metadata-sidecar path (@substrate-term-shape)
remains the byte-identical-guaranteed cycle; the symmetric path is the
"no-sidecar" research direction.

For the demo cycle to work TODAY: use the sidecar. The symmetric closure
is incremental work — each recognizer is bounded, each pattern is named.
