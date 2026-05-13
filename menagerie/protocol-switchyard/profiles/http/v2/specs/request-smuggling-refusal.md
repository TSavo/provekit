# HTTP Request Smuggling Refusal Obligation (v2 profile)

This obligation strengthens the v1 framing-ambiguity refusal for the
`protocol-switchyard-http` profile.

## Obligation

A conformant request parser MUST refuse any HTTP/1.1 request that
exhibits any of the following framing conditions:

1. both `Content-Length` and `Transfer-Encoding` headers present, even
   when they would agree on length;
2. duplicate `Content-Length` header fields, even when all values match;
3. any `Transfer-Encoding` value other than exactly `chunked`;
4. `Transfer-Encoding: chunked` with a trailing or leading whitespace,
   case-folded variant, or comma-separated coding list.

The refusal is unconditional: the parser does not attempt to recover a
canonical message under any of these conditions.

## Refusal shape

The parser produces a refusal record with:

- the offending header bytes (raw, pre-normalization);
- the v2 reason code, one of:
  - `v2-cl-and-te-coexist-refused`;
  - `v2-duplicate-cl-refused`;
  - `v2-non-chunked-te-refused`;
  - `v2-chunked-malformed-refused`.

## Out of scope

This obligation covers framing decisions only. Header field
canonicalization and proxy forwarding rewrites are named separately.

## Migration note

v1 conformant parsers refuse only when `Content-Length` and
`Transfer-Encoding` disagree. v2 parsers refuse the coexistence case
unconditionally and add three additional refusal reason codes. v1
witnesses do not imply v2 conformance.
