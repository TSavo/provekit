# Protocol Catalog Format

**Status:** v1.1.0 normative
**Date:** 2026-04-30

## §0. The protocol is the bytes

The ProvekIt protocol is not this document. The ProvekIt protocol is not the Rust implementation, or the C++ implementation, or the Go implementation. The protocol is the **set of byte sequences** that all conformant implementations produce for any given input.

This document describes those byte sequences in English. The Rust crates implement them. Both are descriptions of the protocol, not the protocol itself. They are caches. Caches can drift. The bytes cannot.

If this English text says one thing and a conformant implementation's bytes say another, the bytes win and the English is updated. If two implementations disagree on bytes for the same input, at least one is non-conformant; byte equality is the arbiter, not prose.

The protocol catalog is the root content-addressed object that names a version of the protocol. Its CID is the version. This document describes its structure, its hashing rule, and the rule for hashing the spec files it references. The rules listed here are normative descriptions of byte-level behavior; an implementation whose bytes disagree with what these rules describe is non-conformant.

## §1. Catalog file structure

A protocol catalog is a JSON document with the following required top-level properties:

| Property      | Type     | Description |
|---------------|----------|-------------|
| `kind`        | string   | MUST be the literal `"catalog"` |
| `name`        | string   | Protocol identifier; for ProvekIt, `"provekit-protocol"` |
| `version`     | string   | Human-facing version label (e.g. `"v1.1.0-2026-04-30"`). Informational only. The version that matters cryptographically is the CID. |
| `algorithms`  | object   | Permitted cryptographic primitives. Each entry maps a role (`hash`, `signature`, `pubkey`) to an array of self-identifying tags (e.g. `"blake3-512"`, `"ed25519"`). |
| `properties`  | object   | Map from spec-key (string) to spec-CID (`<algorithm>:<digest>` self-identifying form). |
| `declaredAt`  | string   | ISO-8601 UTC timestamp of the declaration. |

The catalog MAY carry additional fields prefixed `_` (e.g. `_unsigned`, `_breakingChanges`). Underscore-prefixed fields participate in canonicalization and the catalog CID like any other field; they are not stripped before hashing. They exist for human readers, but the bytes that name the catalog include them.

## §2. Two distinct hashing rules

There are two distinct content-addressing rules in the protocol, and they are NOT interchangeable. Implementations MUST apply the correct rule to each artifact.

### §2.1 Spec files: raw-byte hashing

A spec file (any `.md`, `.json`, or other artifact referenced from `properties`) is hashed as the verbatim bytes on disk:

```
spec_cid = "blake3-512:" || hex(BLAKE3-512(spec_file_bytes))
```

No canonicalization is applied. No newline normalization. No whitespace stripping. The bytes are read from disk and hashed. The recompute tool MUST exit non-zero if any byte in any spec file differs from what produced the catalog's stated `properties[spec-key]`.

Rationale: spec files are prose with embedded examples. Canonicalization would require parsing them, which would require defining a canonical form per spec format. The simpler invariant is: the bytes on disk are the spec.

### §2.2 The catalog itself: JCS-canonical hashing

The catalog file's CID is computed over its JCS-canonical form, NOT its raw file bytes:

```
catalog_cid = "blake3-512:" || hex(BLAKE3-512(JCS(catalog_json)))
```

`JCS(catalog_json)` is the JSON Canonicalization Scheme per RFC 8785 (and the rules in `2026-04-30-canonicalization-grammar.md`): keys sorted lexicographically by code point, no insignificant whitespace, integers in plain decimal form, strings with `\u00XX` escapes for U+0000..U+001F.

A reader who hashes the raw catalog file bytes directly will get a different CID, and that CID is NOT the protocol-defined one. Implementations MUST NOT publish raw-byte hashes of the catalog as if they were the catalog CID.

Rationale: the catalog is structured data, not prose. Different producers may serialize it with different whitespace, key orders, or trailing newlines while preserving the same logical content. JCS canonicalization is the only rule that gives every conformant producer the same bytes for the same logical catalog.

## §3. Recursion: the rule is in the catalog

This document is itself content-addressed and listed in the catalog under the property key `protocol-catalog-format`. Its CID is computed by the rule in §2.1 (raw bytes). Anyone who has the catalog has the rule that says how the catalog itself is hashed. The system bootstraps from the CID of the catalog upward; the catalog bootstraps from this document.

## §4. Determinism guarantees

The two rules together yield a determinism property: given the same set of spec files on disk and the same catalog metadata (kind/name/version/algorithms/properties/declaredAt and any underscore fields), every conformant implementation in any language produces:

- the same byte string for the JCS-canonical form of the catalog
- the same BLAKE3-512 digest of those bytes
- the same self-identifying CID string

This property is not optional. An implementation that produces a different value for any of these for the same inputs is broken.

## §5. Conformance test

The reference tool at `tools/recompute-spec-cids/` exits with status 0 when run with `--verify` if and only if:

1. Every spec file referenced from `properties` exists at the expected path.
2. Every spec file's raw-byte BLAKE3-512 matches the value in `properties[key]`.
3. The catalog's own JCS-canonical-byte BLAKE3-512 matches the value the tool computes from the on-disk catalog after substitution.

Other conformant tools (in any language) must produce byte-identical JCS canonicalization and byte-identical BLAKE3-512 digests when given the same inputs. The Rust JCS encoder at `implementations/rust/provekit-canonicalizer/src/jcs.rs` is the reference; the C++ encoder at `implementations/cpp/provekit/canonicalizer/jcs.cpp` is conformant. New language ports MUST include the `unicode_atomic_predicates_round_trip_verbatim` test fixture (or equivalent) to catch the UTF-8 byte-iteration bug class.

## §6. Forbidden patterns

The following patterns MUST NOT be used to publish catalog CIDs:

- `BLAKE3-512(raw_catalog_file_bytes)` — produces a value that depends on insignificant whitespace and key order; not the protocol-defined CID.
- Truncated digests — the catalog CID is the full 128-hex-char (64-byte) BLAKE3-512 output. No 32-byte form.
- Non-self-identifying digests — every CID published in any context (announcements, papers, READMEs, error messages) MUST carry the `blake3-512:` prefix.
- Cached or remembered CIDs — recompute on demand. A remembered value disagreeing with the recomputed value is the bug, not the recompute.
