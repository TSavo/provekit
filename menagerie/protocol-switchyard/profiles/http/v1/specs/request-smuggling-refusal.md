# HTTP Request Smuggling Refusal Obligation (v1 profile)

This obligation pins one boundary rule for the `protocol-switchyard-http`
profile, version v1.

## Obligation

A conformant request parser MUST refuse any HTTP/1.1 request whose
framing is ambiguous between `Content-Length` and `Transfer-Encoding:
chunked`. Specifically, the parser refuses the request when both header
fields are present in the same message and they disagree about message
length.

## Refusal shape

The parser produces a refusal record with:

- the offending header pair as observed bytes;
- the framing decision the parser would have taken under each header in
  isolation;
- the reason code `RFC9112-frame-ambiguity-refused`.

## Out of scope

This obligation does not cover obsolete line folding, header field
canonicalization, proxy forwarding rewrites, or chunked decoder state
beyond the framing decision. Those obligations are named separately in
the profile.

## Status

This is the v1 profile shape. v2 of this profile strengthens the
refusal: see the v2 profile spec for the strengthened obligation.
