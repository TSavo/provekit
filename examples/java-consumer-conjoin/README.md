# Java Consumer Conjoin

This mirrors `examples/numpy-consumer-demo` on the Java seat.

The consumer imports real `.proof` files from the actual Java library showcase
runs for Gson, Commons Codec, Commons IO, and Commons Text. The test binds
results from real calls into those libraries.

The good consumer asserts agreeing values on the bound results. The bad
consumer asserts `4` and `5` for the same bound Commons Codec result.
`sugar mint` conjoins the Java assertion facts; `sugar verify --json` computes
the consistency verdict through the real Java lifter and z3.
