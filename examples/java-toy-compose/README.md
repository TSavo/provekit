# Java Toy Compose

This showcase is the small cross-library composition demo:

1. Gson serializes a record to JSON.
2. Commons Codec standard-Base64 encodes the JSON bytes.
3. Commons IO round-trips the encoded bytes through a stream.
4. Commons Text escapes/unescapes the transported string.
5. Commons Codec decodes the Base64 and Gson parses the JSON back.

The good twin wires compatible seams and the witness round-trip passes.
The bad twin wires a standard-Base64 producer into a URL-safe-Base64
consumer precondition. Its sampled unit test still passes in isolation,
but the composed proof receipt marks the universal seam obligation
`post_standard_base64 |= pre_url_safe_base64` unsatisfied and names the
colliding app/library pair.

Scope: this receipt composes the Java logo-wall library proof CIDs by
content address and checks the cross-library seam obligations. It does
not claim a new proof of the library internals; those are the separate
real-library logo proofs.
