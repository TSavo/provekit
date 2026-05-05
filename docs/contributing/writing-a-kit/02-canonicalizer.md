# Writing a kit, step 2: the canonicalizer

The canonicalizer is the load-bearing piece of every kit. If your canonicalizer agrees with the Rust canonicalizer byte-for-byte on every fixture, the rest of the kit is mechanical. If it does not, every other component is broken and you will not know which one.

This step is dense. Read it twice.

## What the canonicalizer does

Input: an `IrFormula` (or `IrTerm`, `Sort`, `Declaration`, etc.), a structured value matching the CDDL grammar at [`protocol/provekit-ir.cddl`](../../../protocol/provekit-ir.cddl).

Output: the BLAKE3-512 of the JCS-canonicalized JSON serialization of that value.

In pseudo-code:

```
fn canonicalize(formula: IrFormula) -> [u8; 64]:
    json_value = to_json(formula)        # structured JSON
    canonical_bytes = jcs_encode(json_value)  # RFC 8785
    return blake3_512(canonical_bytes)
```

Three pieces, each precisely specified:

1. **Structured JSON shape.** The CDDL grammar defines what `Term`, `Formula`, etc. look like as JSON. There is one canonical representation per IR value.
2. **JCS encoding.** RFC 8785. The bytes of the canonical JSON representation.
3. **BLAKE3-512 hash.** The 64-byte CID.

If you get any of these wrong by one byte, every downstream component fails its conformance fixture.

## JCS (RFC 8785) is not your default JSON encoder

This is the place every first-time port loses days. RFC 8785 specifies JSON Canonicalization Scheme. Most languages' default JSON encoders do not implement it. The differences are subtle and the failure mode is "all your hashes are wrong."

The non-negotiable requirements:

### Number representation

Numbers are serialized per ECMA-262 section 7.1.12.1 ("ToString applied to Number type"). Concretely:

- Integers that fit in i53 emit as integer literals: `0`, `42`, `-1`. No trailing `.0`.
- Non-integers emit per ECMA-262: `1.5`, `0.1`, `1e21`, `1e-7`. Specific thresholds for scientific notation.
- `NaN`, `+Infinity`, `-Infinity` are not valid JSON; if your IR reaches one, the canonicalizer must error.

Your language's `json.dumps(0.0)` probably emits `"0.0"`. JCS demands `"0"`. Your language's `json.dumps(1.0)` probably emits `"1.0"`. JCS demands `"1"`. Your language's `json.dumps(1e30)` probably emits `"1e+30"`. JCS demands `"1e+30"` if and only if the threshold is correct for that magnitude.

Implement a number serializer that follows ECMA-262 exactly, or import one. Do not trust your default encoder.

### Key ordering

Object keys are serialized in Unicode codepoint order (JCS §3.2.3). Most languages iterate object keys in insertion order. You must sort keys explicitly before serialization.

For nested objects, sort recursively at every level.

UTF-8 codepoint order is not the same as locale-aware string ordering. Use the language's raw codepoint comparison, not `strcmp` or locale-aware sort.

### String escaping

JCS specifies the minimal JSON escape set:
- `\"` for `"`
- `\\` for `\`
- `\b` for `U+0008`
- `\f` for `U+000C`
- `\n` for `U+000A`
- `\r` for `U+000D`
- `\t` for `U+0009`
- `\uXXXX` (lowercase hex) for any other control character `U+0000` through `U+001F`

Everything else passes through as raw UTF-8. Specifically:

- **Do not escape forward slashes.** Many encoders emit `\/` for `/`. JCS demands `/`.
- **Do not escape non-ASCII.** Many encoders emit `é` for `é`. JCS demands the raw UTF-8 bytes `0xc3 0xa9`.
- **Use lowercase hex** in `\uXXXX` escapes (`é`, not `é`).

### No whitespace, no indentation

Compact output. No newlines between elements. No spaces after `:` or `,`. No trailing newline.

### Arrays preserve order

Unlike objects, array elements stay in the order given. Your IR should produce them in canonical order in the first place; the canonicalizer does not sort arrays.

### Boolean and null

`true`, `false`, `null`. Lowercase. No surprises.

## BLAKE3-512

The hash is BLAKE3 with output length 64 bytes (512 bits). Use the BLAKE3 XOF (extensible output function) interface and request 64 bytes. Do not use BLAKE3's default 32-byte output.

BLAKE3 is deterministic; the same input always produces the same 64 bytes regardless of platform, SIMD path, or threading. The vendored BLAKE3 in `tools/blake3-vendored/` builds with all SIMD paths disabled, which guarantees portability.

Your language's BLAKE3 binding may default to SIMD-accelerated paths. Verify against the vendored reference: feed the same input to both, compare outputs. If they differ, your binding is broken.

## The CID format

The CID is a string with a multibase prefix:

```
blake3-512:dc2f42ff8a4a66289cc19bfbd628898b8bd8e61d2148ecf609324cc2421c5c440a6c0e70e20ffbecabeb78e0253101d72823b7e3ab120a4d56cb67c8e31dc641
```

Format: `blake3-512:<128 lowercase hex digits>`. Lowercase. No leading `0x`. No padding.

When the protocol describes "the CID" of a memento, it means this string. When the protocol describes "the 64 bytes," it means the raw hash output. The string is `blake3-512:` plus the lowercased hex of the 64 bytes.

## Testing your canonicalizer

The conformance fixtures cover the canonicalizer as a black box: they feed an IR value and check the output bytes. But during development, you'll want to test individual layers:

1. **JCS encoding tests.** Feed structured JSON values, check the canonical bytes. The Rust workspace includes JCS test vectors borrowed from the RFC. Re-run them in your language.
2. **BLAKE3 vector tests.** The BLAKE3 spec ships with test vectors. Hash them, compare outputs.
3. **End-to-end fixture tests.** The conformance harness covers this; run it locally during development.

If JCS and BLAKE3 are correct individually, end-to-end is correct. If end-to-end is wrong, narrow down which layer fails by comparing intermediate bytes against the Rust kit's intermediate output.

## The compose order

A common mistake is to hash the unserialized IR object directly, or to hash a re-deserialized representation. The correct order is always:

1. IR object → JCS bytes (the canonical bytes).
2. JCS bytes → BLAKE3-512 hash (the CID).

Hash the bytes you emit. Never hash a structure; structures don't have a canonical byte representation in your language. The whole point of JCS is to give you canonical bytes.

## When this step is done

The fixtures `eq_atomic`, `pattern1_bounded_loop`, `contract_decl` pass. The bytes your canonicalizer emits agree with the canonical bytes. The CIDs your canonicalizer derives agree with the canonical CIDs.

Steps 3 onward layer Ed25519 signing, CBOR encoding, and self-contracts on top of bytes that are now provably correct. The hard part is behind you.

## Read next

- [03-claim-envelope.md](03-claim-envelope.md): signed memento envelope (Ed25519).
- [docs/reference/ir/canonical-form.md](../../reference/ir/canonical-form.md) (when written): JCS + BLAKE3-512 reference.
- RFC 8785: JCS specification, full text.
- BLAKE3 paper: hash function specification.
