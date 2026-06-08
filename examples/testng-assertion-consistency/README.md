# TestNG assertion consistency showcase

This mirrors `examples/java-test-assertion-consistency`, but points the unchanged
Java assertion-vocabulary lifter at a second real framework: TestNG.

The vocab source is the real `org.testng.Assert` class from the pinned TestNG
jar, read through `javap -classpath ... -public org.testng.Assert`.
Signature-derived tolerance overloads such as
`assertEquals(double, double, double)` are classified as approximate and are not
lifted as exact equality. Exact equality and truth for the jar body gap come
from the declared override file under `.sugar/vocab-exceptions/`, not from an
in-code TestNG table.

The good twin proves both axes: scalar assertion consistency discharges, and
the real TestNG suite passes. The bad twin is contradictory: consistency refuses
and the real TestNG suite fails.

