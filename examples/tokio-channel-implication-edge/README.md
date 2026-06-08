# Tokio Channel Implication Edge

This showcase proves the narrow channel edge:

`send(producer).post |= recv-side consumer.pre` through a local
`tokio::sync::mpsc` conduit.

The good twin sends a value from a producer whose postcondition establishes
`result == 6`, receives from the channel, then passes that value to a consumer
whose precondition requires `x == 6`. The bad twin weakens the send-side
producer postcondition to `result == 5` while the recv-side consumer still
requires `x == 6`, so the channel implication edge refuses.

Scope: this is only the element-contract implication row. It does not prove
which send pairs with which recv, message ordering, channel cardinality,
interleavings, locks, deadlock freedom, data-race freedom, or Rust type/borrow
rules.

The witness axis is the real `#[tokio::test]` cargo test run.
