# Content-Length and Transfer-Encoding Handling (v2 profile)

This obligation pins how the `protocol-switchyard-http` v2 profile reads
message body length.

## Obligation

For HTTP/1.1 messages, the parser determines body length using exactly
one of the following sources, in this order:

1. `Transfer-Encoding: chunked` when no `Content-Length` header is
   present and the value is exactly the literal byte sequence `chunked`;
2. `Content-Length` when no `Transfer-Encoding` header is present and
   the field appears at most once with a non-negative decimal integer
   value;
3. connection-close framing for responses where neither header is
   present, when permitted by the message semantics.

Any other configuration is refused under the v2 request smuggling
refusal obligation. v2 parsers do not implement a "best effort" recovery
path.

## Out of scope

Body content decoding, decompression, and trailer semantics are named in
separate obligations.

## Difference from v1

The v1 obligation accepted some boundary cases (for example, single
`Content-Length` together with `Transfer-Encoding` if they agreed on
length). v2 closes that boundary: any coexistence of the two headers is
refused.
