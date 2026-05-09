# Content-Length and Transfer-Encoding Handling (v1 profile)

This obligation pins how the `protocol-switchyard-http` v1 profile reads
message body length.

## Obligation

For HTTP/1.1 messages, the parser determines body length using exactly
one of the following sources, in this order:

1. `Transfer-Encoding: chunked`, when present and well-formed;
2. `Content-Length`, when no `Transfer-Encoding` is present;
3. connection-close framing for responses where neither header is
   present, when permitted by the message semantics.

If `Transfer-Encoding` is present and includes any codings other than
`chunked`, or `chunked` is not the final coding in the list, the parser
refuses the message under the framing-ambiguity refusal.

## Out of scope

Body content decoding, decompression, and trailer semantics are named in
separate obligations.
