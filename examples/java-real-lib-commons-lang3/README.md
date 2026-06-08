# Java Real-Library Logo: Commons Lang3

This showcase proves the Java seat on Apache Commons Lang3 3.14.0 with zero
source changes. `run.sh` fetches the pinned Maven Central source and test-source
artifacts, lays them out as the real Maven module expects, and verifies the
durable `.proof` plus witness bundle.

The consistency axis learns JUnit assertion vocabulary with `javap` from the
real JUnit jar and proves exact assertion rows from the real
`JavaVersionTest.java` subcorpus. The witness axis compiles the real Commons
Lang3 test corpus and runs the real `JavaVersionTest` JUnit class. The full
direct all-class scan is intentionally not claimed here because Lang3's suite
has static test-state coupling under the standalone console runner.
