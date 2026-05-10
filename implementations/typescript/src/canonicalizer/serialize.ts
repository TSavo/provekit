/**
 * Pass 7: canonical serialization.
 *
 * Serialization format: canonical JSON per RFC 8785 (JSON Canonicalization
 * Scheme, JCS). This is the v1 fallback choice documented in the spec:
 *
 *   "a kit MAY use JSON canonical encoding RFC 8785 if its host language's
 *    CBOR support is poor."
 *
 * The preferred format is canonical CBOR (RFC 8949 §4.2). CBOR is not
 * installed in this package (cbor-x would add a dep). JCS is implemented
 * here (~80 lines) using only Node's built-in `JSON.stringify` with a
 * key-sorting replacer and explicit number normalization. This choice is
 * recorded here so downstream consumers can detect it:
 *
 *   SERIALIZATION_FORMAT = "jcs-json-rfc8785"
 *
 * Mementos produced under JCS are NOT cross-comparable with mementos
 * produced under CBOR serialization for the same logical claim. A future
 * migration to CBOR would require re-hashing all mementos under the new
 * serialization.
 *
 * JCS rules implemented:
 * - Object keys sorted lexicographically by their Unicode code point.
 * - Numbers: IEEE 754 doubles serialized per RFC 8785 §3.2.2.3, which
 *   normatively delegates to ECMA-262 §7.1.12.1 (Number::toString incl.
 *   "Note 2"). On V8/Node, `JSON.stringify(n)` for a finite number IS
 *   that algorithm: RFC 8785 cites V8 as the reference implementation,
 *   and Appendix A's reference canonicalizer uses `JSON.stringify` for
 *   the number path verbatim. -0 is normalized to "0" defensively
 *   (pass 2 does not currently rewrite -0 in CanonicalConst). NaN and
 *   ±Infinity throw, per the §3.2.2.3 prohibition.
 *   Conformance is pinned in equivalence.test.ts §11 against all 24
 *   RFC 8785 Appendix B fixtures; any future hand-rolled number
 *   formatter that drifts from §3.2.2.3 is caught there.
 * - Strings: no extra escaping beyond JSON spec.
 * - bigint: serialized as a JSON number (valid for values that fit in
 *   safe integer range; bigints outside safe range are serialized as
 *   strings with a "bigint:" prefix to preserve round-trip identity).
 * - null: serialized as JSON null.
 * - Arrays: order preserved (ordering is the caller's responsibility).
 *
 * WARNING: the JCS spec (RFC 8785) is designed for objects with string
 * values. Our canonical AST is object-structured; the implementation
 * below is sufficient for the AST node types defined in ast.ts.
 *
 * Cross-kit note: this conformance argument is V8/Node-specific. Each
 * non-TS kit's serializer (Go, Rust, Python, ...) needs its own
 * §3.2.2.3 conformance suite: a host-language `n.toString()` does NOT
 * match V8's output for many edge cases.
 */

import type { CanonicalFolAst } from "./ast.js";

/** The serialization format in use. Recorded for cross-kit compatibility checks. */
export const SERIALIZATION_FORMAT = "jcs-json-rfc8785" as const;

/**
 * Serialize a canonical FOL AST to a deterministic byte buffer.
 * The buffer is the input to the hash function.
 */
export function serializeCanonicalAst(ast: CanonicalFolAst): Buffer {
  const json = canonicalJsonStringify(ast);
  return Buffer.from(json, "utf8");
}

// -----------------------------------------------------------------------
// Canonical JSON serializer (RFC 8785)
// -----------------------------------------------------------------------

/**
 * Produce a canonical JSON string for any value that appears in the
 * CanonicalFolAst tree. Object keys are sorted; arrays are ordered;
 * numbers are normalized.
 */
function canonicalJsonStringify(value: unknown): string {
  return stringify(value);
}

function stringify(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return stringifyNumber(value);
  if (typeof value === "bigint") return stringifyBigInt(value);
  if (typeof value === "string") return JSON.stringify(value); // handles escaping
  if (Array.isArray(value)) {
    return "[" + value.map(stringify).join(",") + "]";
  }
  if (typeof value === "object" && value !== null) {
    // Sort keys by Unicode code point order (RFC 8785 §3.2.3).
    const keys = Object.keys(value as Record<string, unknown>).sort();
    const pairs = keys.map((k) => {
      const v = (value as Record<string, unknown>)[k];
      return JSON.stringify(k) + ":" + stringify(v);
    });
    return "{" + pairs.join(",") + "}";
  }
  // Fallback (undefined, symbol, function: should not appear in AST).
  return "null";
}

function stringifyNumber(n: number): string {
  // -0 → "0" per RFC 8785 §3.2.2.3 (ECMA-262 ToString-applied-to-Number;
  // Note 2 collapses -0). Pass 2 does not currently rewrite -0 in
  // CanonicalConst, so this branch is load-bearing.
  if (Object.is(n, -0)) return "0";
  if (!isFinite(n)) {
    // NaN and ±Infinity are not permitted in canonical JSON
    // (RFC 8785 §3.2.2.3, last paragraph). Throw rather than emit
    // a non-conformant token.
    throw new Error(`Cannot serialize non-finite number ${n} in canonical JSON`);
  }
  // RFC 8785 §3.2.2.3 delegates to ECMA-262 §7.1.12.1 incl. Note 2.
  // V8's JSON.stringify implements that algorithm exactly; the RFC
  // cites V8 as the reference implementation and Appendix A's
  // reference canonicalizer uses this same call for the number path.
  // Conformance pinned in equivalence.test.ts §11 (Appendix B fixtures).
  return JSON.stringify(n);
}

function stringifyBigInt(n: bigint): string {
  // Check safe range.
  const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER);
  const MIN_SAFE = BigInt(Number.MIN_SAFE_INTEGER);
  if (n >= MIN_SAFE && n <= MAX_SAFE) {
    // Safe integer: serialize as JSON number.
    return n.toString();
  }
  // Outside safe range: serialize with a disambiguating prefix so the
  // canonical form is still unambiguous and round-trippable.
  return JSON.stringify(`bigint:${n.toString()}`);
}
