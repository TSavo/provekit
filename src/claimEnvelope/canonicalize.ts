/**
 * Canonical JSON encoding for claim envelopes.
 *
 * Spec: docs/specs/2026-04-29-universal-claim-envelope.md §CID construction
 * (line ~271): "Canonical encoding: JSON with sorted keys, no whitespace, UTF-8."
 *
 * Encoding choice: canonical JSON rather than CBOR. The spec is explicit
 * about the algorithm (sorted-keys JSON, no whitespace, UTF-8). This
 * implementation is ~50 lines with no external dependency, deterministic
 * across Node versions, and binary-identical to the output any conforming
 * implementation in any host language would produce.
 *
 * Why not cbor-x? The spec mandates canonical JSON. CBOR would be a
 * different byte sequence and would break cross-language CID agreement.
 * We document the choice here so implementors in other host languages
 * know what to target.
 */

/**
 * Produce canonical JSON bytes for the given value. Deterministic:
 * - Object keys sorted lexicographically (by UTF-16 code unit, matching
 *   JSON.stringify and RFC 8785 §3.2.3).
 * - No whitespace.
 * - Arrays preserve element order.
 * - undefined values in objects are omitted (consistent with JSON.stringify).
 * - UTF-8 encoded.
 *
 * Two calls with structurally identical values produce byte-identical output.
 */
export function canonicalEncode(value: unknown): Buffer {
  return Buffer.from(canonicalJsonString(value), "utf-8");
}

/**
 * Produce the canonical JSON string for the given value.
 * Exported for testing; prefer `canonicalEncode` for hashing.
 */
export function canonicalJsonString(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "null";

  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";

    case "number":
      if (!isFinite(value)) {
        throw new TypeError(
          `Cannot canonicalize non-finite number: ${value}`,
        );
      }
      return String(value);

    case "string":
      return JSON.stringify(value); // handles escaping correctly

    case "bigint":
      return String(value);

    case "object":
      if (Array.isArray(value)) {
        return "[" + value.map(canonicalJsonString).join(",") + "]";
      }
      // Plain object: sort keys lexicographically.
      {
        const obj = value as Record<string, unknown>;
        const keys = Object.keys(obj).sort();
        const pairs = keys
          .filter((k) => obj[k] !== undefined)
          .map((k) => JSON.stringify(k) + ":" + canonicalJsonString(obj[k]));
        return "{" + pairs.join(",") + "}";
      }

    default:
      throw new TypeError(`Cannot canonicalize value of type ${typeof value}`);
  }
}
