# Java Consumer Conjoin

This mirrors `examples/numpy-consumer-demo` on the Java seat.

The consumer imports real `.proof` files from the actual Java library showcase
runs for Gson, Commons Codec, Commons IO, and Commons Text. The test binds
results from real calls into those libraries.

The Codec row is vendor-sourced from Apache Commons Codec 1.17.1
`Base64Test.java`: `b4 = Hex.decodeHex("2bf7cc2701fe4397b49ebeed5acc7090")`
and `Base64.encodeBase64String(b4) == "K/fMJwH+Q5e0nr7tWsxwkA=="`.

The good consumer asserts the same standard Base64 value. The bad consumer
asserts the url-safe value `"K_fMJwH-Q5e0nr7tWsxwkA"` for that same call. That
bad runtime test is expected to fail: the user assumption is wrong. The point of
the showcase is that `sugar mint` and `sugar verify --json` turn that runtime
error into a compile-time contract refusal by conjoining the user assertion with
Codec's real proven row through the existing Java assertion lifter and z3.
