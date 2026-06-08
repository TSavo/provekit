# Java Test-Assertion Consistency

This is the Java/JUnit seat on the sugar substrate.

- `java-test-assertions` learns an assertion library's vocabulary from its own
  Java source/signatures. Tolerance signatures are approximate and refused as
  exact equality. Exact equality is body-derived, not name-derived.
- `java-junit-witness` compiles and runs the JUnit tests, content-addresses the
  per-test outcomes, and discharges only when the suite re-runs cleanly.

The tests use JUnit as the runner, but the assertion vocabulary is learned from
the local `demo.assertions.LearnedAssertions` source. No JUnit assertion names
are hardcoded in the lifter.
