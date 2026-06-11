# Vendored source provenance — java-abs-universe

## JDK Math.java — tag `jdk-21+35`

Source: https://github.com/openjdk/jdk, tag `jdk-21+35`.
License: GNU General Public License, version 2, with the Classpath Exception.
Upstream path: `src/java.base/share/classes/java/lang/Math.java`

Raw URL: https://raw.githubusercontent.com/openjdk/jdk/jdk-21+35/src/java.base/share/classes/java/lang/Math.java

| File (under `good/vendor/jdk21/java/lang/` and `bad/vendor/jdk21/java/lang/`) | sha256 |
|---|---|
| `Math.java` | `1264b299cbffe5611764dc9a626f9beb2a02728a1651f3f3fee1e0b767924151` |

The abs(int) body — letter-for-letter from the AST:

```java
@IntrinsicCandidate
public static int abs(int a) {
    return (a < 0) ? -a : a;
}
```

Shape: ternary-with-comparison returning param (`a`) or unary-negation of param (`-a`).
Under two's complement (JLS §4.2.1 — a COMPILER AXIOM): `-Integer.MIN_VALUE == Integer.MIN_VALUE`.
Therefore: `abs(Integer.MIN_VALUE) == Integer.MIN_VALUE == -2147483648`.
The walked universe does NOT discharge `abs(x) >= 0` as a universal fact.

## JUnit 5 framework source (assertion vocabulary)

`good/vendor/junit5/` and `bad/vendor/junit5/` are copied from
`examples/java-assertion-consistency/*/vendor/junit5/` — see the
`PROVENANCE.md` inside those directories (junit5 tag `r5.10.2`).
