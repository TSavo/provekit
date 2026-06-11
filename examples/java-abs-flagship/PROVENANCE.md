# Vendored source provenance — java-abs-flagship

## AbsTests.java (jtreg test, JDK stdlib flagship)

Source: https://github.com/openjdk/jdk, branch `master`.
License: GNU General Public License, version 2, with the Classpath Exception.
Upstream path: `test/jdk/java/lang/Math/AbsTests.java`

Raw URL: https://raw.githubusercontent.com/openjdk/jdk/master/test/jdk/java/lang/Math/AbsTests.java

| File (under `good/src/jtreg/`) | sha256 |
|---|---|
| `AbsTests.java` | `6127b1b0e3cbe4ed9563d16248470d4645eecd34ea84ff92076f594780604e63` |

This file is placed VERBATIM (unmodified) as the source under test.
The kit lifts it directly without any transformation.

The key line (110):

```java
errors += testIntAbs(Math::abs, Integer.MIN_VALUE, Integer.MIN_VALUE);
```

The JDK comments this `// Strange but true`. Sugar proves it.

### What this file proves

Line 110 encodes: `abs(Integer.MIN_VALUE) == Integer.MIN_VALUE`.

This is the mic-drop: the language's own test suite is the spec. The JDK itself
tests and documents that `Math.abs(Integer.MIN_VALUE) == Integer.MIN_VALUE`
(a negative result from abs). The industry believes `abs(x) >= 0`. Both cannot
be true. The walked body (`(a < 0) ? -a : a` under 32-bit two's complement)
proves the JDK is right.

## JDK Math.java — tag `jdk-21+35`

Source: https://github.com/openjdk/jdk, tag `jdk-21+35`.
License: GNU General Public License, version 2, with the Classpath Exception.
Upstream path: `src/java.base/share/classes/java/lang/Math.java`

Raw URL: https://raw.githubusercontent.com/openjdk/jdk/jdk-21+35/src/java.base/share/classes/java/lang/Math.java

| File (under `good/vendor/jdk21/java/lang/` and `bad/vendor/jdk21/java/lang/`) | sha256 |
|---|---|
| `Math.java` | `1264b299cbffe5611764dc9a626f9beb2a02728a1651f3f3fee1e0b767924151` |

The abs(int) body (letter-for-letter from the AST):

```java
@IntrinsicCandidate
public static int abs(int a) {
    return (a < 0) ? -a : a;
}
```

Under two's complement (JLS §4.2.1): `-Integer.MIN_VALUE == Integer.MIN_VALUE == -2147483648`.
Therefore: `abs(Integer.MIN_VALUE) == Integer.MIN_VALUE == -2147483648`.

## JUnit 5 framework source (bad suite assertion vocabulary)

`bad/vendor/junit5/` is copied from `examples/java-abs-universe/bad/vendor/junit5/`
(junit5 tag `r5.10.2` — see PROVENANCE.md in that directory).
The bad suite uses JUnit `assertEquals` to encode the industry belief, which
the kit lifts via the vocab-learned pathway (NOT the jtreg error-sentinel path).
