# Tokio Await Implication Edge

This showcase proves the narrow async edge:

`producer.post |= consumer.pre` across `.await`.

The good twin awaits a producer whose postcondition establishes `result == 6`,
then passes that value to a consumer whose precondition requires `x == 6`.
The bad twin awaits a producer whose postcondition establishes `result == 5`
while the consumer still requires `x == 6`, so the implication edge refuses.

The witness axis is the real `#[tokio::test]` cargo test run.
