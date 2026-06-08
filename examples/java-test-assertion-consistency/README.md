# Java Test-Assertion Consistency

This is the Java/JUnit seat on the sugar substrate.

- `java-test-assertions` learns an assertion library's vocabulary from its own
  Java source/signatures. Tolerance signatures are approximate and refused as
  exact equality. Exact equality is body-derived, not name-derived.
- `java-junit-witness` compiles and runs the JUnit tests, content-addresses the
  per-test outcomes, and discharges only when the suite re-runs cleanly.

The tests import and call real `org.junit.jupiter.api.Assertions` from the pinned
`junit-platform-console-standalone` jar. The lifter derives JUnit's signatures
with `javap`; the tolerance overloads such as
`assertEquals(double, double, double)` are classified as approximate from the
real signature and are not lifted as exact equality.

Because `javap` exposes signatures but not method bodies, exact equality and
truth recognition for the jar body gap are declared in
`.sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json`. That file is a
data override, not an in-code JUnit name table; removing it removes only the
equality/truth recognition, not the signature-derived approximate split.
