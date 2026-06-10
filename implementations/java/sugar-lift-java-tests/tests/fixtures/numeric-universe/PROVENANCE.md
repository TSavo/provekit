# Vendored source provenance — numeric-universe fixture

## JDK Math.java — tag `jdk-21+35`

Source: https://github.com/openjdk/jdk, tag `jdk-21+35`.
License: GNU General Public License, version 2, with the Classpath Exception.

| File (under `vendor/jdk21/java/lang/`) | sha256 |
|---|---|
| `Math.java` | `1264b299cbffe5611764dc9a626f9beb2a02728a1651f3f3fee1e0b767924151` |

abs(int) body: `return (a < 0) ? -a : a;`
Under two's complement: abs(Integer.MIN_VALUE) == Integer.MIN_VALUE == -2147483648.
