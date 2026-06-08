# Java Real-Library Capstone

This showcase proves the Java seat on a real third-party library: Apache
Commons Codec 1.17.1, fetched from pinned Maven Central source and test-source
artifacts.

The committed showcase does not carry Commons Codec source or tests. `run.sh`
downloads the pinned artifacts, extracts the library's real source files, places
the Maven test resources where the real tests expect them, and then runs two
axes:

- `java-test-assertions` learns real JUnit assertion vocabulary with `javap`
  from the pinned `junit-platform-console-standalone` jar, plus the externalized
  JUnit body-gap override for exact equality/truth.
- `java-junit-witness` compiles and runs the real Commons Codec JUnit suite on
  battleaxe/JDK 21, content-addressing the test outcomes.

Scope is intentionally narrow and loud: this proves consistency for the exact
assertion rows the learned Java assertion lifter can express, and witness
correctness for the real Commons Codec test suite. It does not claim whole
library semantic correctness, Bean Validation runtime behavior, or any property
outside consistency plus witness. Commons Codec 1.17.1 does not ship JSR-380
method constraints in this artifact, so the implication-edge stretch is not
applicable here.
