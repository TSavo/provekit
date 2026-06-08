# libsugar-py v0 -> v1: vendor-shim gap for blake3 FFI

## Where v0 stopped

End-to-end pipeline runs cleanly:

1. `sugar-walk-emit term implementations/rust/sugar-canonicalizer/src/hash.rs blake3_512_of` emits a 6407-byte ProofIR with term CID `blake3-512:63306c27...`.
2. `sugar-realize-python-core` consumes the term via JSON-RPC and returns:

```python
def blake3_512_of(bytes):
    # sugar-realize-python: unsupported canonical call `blake3::Hasher::new`; no Python shim matched `call:new(blake3::Hasher::new, [])`
    raise NotImplementedError("sugar-bind canonical: call:blake3::Hasher::new")
```

`is_stub=True`. The realize-kit honestly refuses at the first FFI call that has no Python shim.

`implementations/python/libsugar-py/libsugar_py/canonical.py` is the verbatim text of the realize-kit's `source` field. No hand-editing, no decoration.

## The gap class

`no-python-shim-for-ffi-call:<symbol>` for the following Rust FFI symbols referenced in the term surface:

- `blake3::Hasher::new` (call)
- `update` (method)
- `finalize_xof` (method)
- `fill` (method)
- `hex::encode` (call)
- `String::with_capacity` (call)
- `len` (method)
- `push_str` (method)

All map to library calls that have well-defined Python equivalents but no body-template entry in `menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-libsugar.json`.

## A separate, deeper gap: lifter statement-level effect loss

The Rust body contains side-effecting statements that the lifter drops:

```rust
let mut hasher = blake3::Hasher::new();
hasher.update(bytes);                          // dropped
let mut out = [0u8; 64];
hasher.finalize_xof().fill(&mut out);          // dropped
let hex = hex::encode(out);
```

The lifter records these as `ffi-call-unresolved-effect` in the loss_record but they do not appear in the term tree. Even if every FFI shim were authored, the resulting Python would be wrong: `out` would always be `[0u8; 64]` and the digest would be the hash of zeros regardless of input.

This is a lifter-level gap, distinct from the realize-kit's shim gap. It is substrate-gated and explicitly out of scope for per-language kit work.

## Two paths to v1

### Path A (per-language kit, no substrate)

Author `python-canonical-bodies-libsugar.json` entries that bind the eight FFI symbols above to Python equivalents:

| Rust symbol | Python equivalent |
| --- | --- |
| `blake3::Hasher::new` | `blake3.blake3()` |
| `Hasher.update` | `.update(...)` |
| `Hasher.finalize_xof` | `(no-op; digest takes length=)` |
| `OutputReader.fill` | `(use digest(length=64))` |
| `hex::encode` | `bytes.hex()` |
| `String::with_capacity` | `""` (Python strings are not preallocated) |
| `str.len` | `len(...)` |
| `String.push_str` | `+= ...` |

This is body-template work; it follows the existing schema and signs the new entries with the libsugar-py kit's key. It does NOT touch substrate.

But: until the lifter gap is closed, the lowered Python is wrong even with full shim coverage. Path A produces a Python `blake3_512_of` that returns the same string regardless of input. That violates `k(I) = t` for any non-trivial `t`. Loudly characterize the loss in the receipt.

### Path B (vendor absorption flow, issue #985)

Run `bootstrap/scripts/absorb_vendor_library.sh <blake3-pyo3-source-path> blake3` to:

1. Bind the blake3 vendor library (Rust crate) into the substrate via `sugar bind --rewrite annotate`.
2. LLM auto-names each anonymous concept at the API edge per issue #980.
3. Re-bind with `--rewrite invisible` to mint named PromotionDecisionMemento entries.
4. Auto-mint concept hubs via `mint_from_promotion_decisions.py`.
5. Generate `python-canonical-bodies-libsugar.json` entries from the bind output.

This produces the same body-template state as Path A but driven by the LLM-narrated cluster -> concept -> name pipeline. It is the canonical "absorption" pattern.

Still gated by the lifter statement-level effect gap.

### Path C (substrate work, not tonight)

Extend `sugar-walk` to track statement-level method-call effects so `hasher.update(bytes)` appears in the lifted term and contributes to the output's data flow. This is the architectural fix that makes blake3_512_of's term faithful to its semantics. Substrate-gated.

## Next chunk's brief

Pick Path A (manual body-template entries) as the minimal forward motion. Write three entries: blake3::Hasher::new, hex::encode, String::push_str. Re-run the realize pipeline. Surface the next refusal class. Iterate.

Defer Path B (#985 vendor absorption driver) until at least one manual Path A entry confirms the realize-kit's catalog-match logic finds new body-template entries correctly.

Defer Path C indefinitely under the per-language kit constraint.
