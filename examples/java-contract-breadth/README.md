# Java Contract Breadth

This showcase proves Java callsite implication rows:

`producer.post |= consumer.pre`

`java-jsr380-contracts` lifts JSR-380 `@Min`, `@Max`, `@Size`, and
`@NotNull` annotations into method pre/post contracts. `java-implications`
then emits callsite bridges for consumers of producer results, reusing the
same substrate implication machinery as the narrower Java implication edge.
`java-junit-witness` compiles and runs the JUnit suite as the execution witness
axis.

The receipt is scoped to the callsite implication rows named in `run.sh`. It
does not reimplement Bean Validation at runtime and does not prove Java type or
compiler facts; compiled Java is the legality boundary.

