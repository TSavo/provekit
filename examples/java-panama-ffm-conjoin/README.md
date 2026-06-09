# Java Panama FFM Conjoin

This showcase proves a cross-language `#euf#` join with real artifacts:

- Rust side: the real `base64 0.22.1` crate is minted from its own vendor tests. The row used here is `decoded_len_estimate(4) == 3`.
- Java side: a JUnit test calls a native symbol through Panama FFM and asserts the same call result.
- Join: the Java Panama lifter emits a call-edge sidecar whose `targetSymbol` names the same rust `#euf#` row.

The good twin asserts `decoded_len_estimate(4) == 3` and verifies. The bad twin asserts `decoded_len_estimate(4) == 4`; it compiles, runs through the same FFM call, and is refused by the conjoined proof.
