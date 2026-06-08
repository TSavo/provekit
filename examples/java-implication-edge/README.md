# Java Implication Edge

This showcase proves the Java callsite implication row:

`producer.post |= consumer.pre`

`java-jsr380-contracts` lifts JSR-380 `@Min` annotations into method
pre/post contracts. `java-implications` then emits the callsite bridge for
`consumer(producer())`, reusing the same substrate implication machinery used
by the Rust and Python seats. `java-junit-witness` compiles and runs the JUnit
suite as the execution witness axis.

The receipt is scoped to the `bridge=consumer` callsite implication row. It
does not reimplement Bean Validation at runtime and does not prove Java type or
compiler facts; compiled Java is the legality boundary.
