# Vendored source provenance — java-crc32-universe

The keystone (the merged array/loop-unroll `RecurrenceUniverseWalker`, extended
in this work to treat a `static {}` initializer as a first-class **construction
site**) symbolically executes the vendor's CRC table-generation recurrence,
operator by operator and constant by constant, from `com.sun.source` tree nodes.
The SMT emitter and the walker contain no CRC knowledge of their own.

## The construction-site axiom (JLS §12.4)

A `static final int[] TABLE` (or `int[][]`) filled by a loop in a `static {}`
block has its value **present and fixed at every read of the field**. The Java
Language Specification §12.4 guarantees the static initializer runs *exactly
once*, in *textual order*, *before the first active use of the class*, and
deterministically. So by the time any callsite reads the table, the JVM has
already constructed it, and the JLS *swears* that is the value every reader sees.

We do **not** simulate class loading. We **quote** that guarantee — the same way
the kit quotes `final` = single-assignment (the compiler is the bailiff) and the
platform-axioms quote identity bridges. The static-init block is the
construction; happens-before-any-read is the axiom; constant-folding the
literal-bounded fill loop is right-by-construction. Every constant / polynomial /
shift / mask / array-index in the folded table traces to an AST node;
uninterpretable nodes are **refused by name**, never faked.

## Vendor: OpenJDK `java.util.zip.CRC32C` — tag `jdk-21+35`

Source: https://github.com/openjdk/jdk, tag `jdk-21+35`.
License: GPLv2 with the Classpath Exception (license header intact in the file).
A **pure-Java** checksum: it builds its lookup table in a Java `static {}` block
from the reversed Castagnoli polynomial and runs the classic reflected update.

| File (under `good/vendor/jdk-crc32c/` and `bad/vendor/jdk-crc32c/`) | Upstream path | sha256 |
|---|---|---|
| `CRC32C.java` | `src/java.base/share/classes/java/util/zip/CRC32C.java` | `ac23a23e3527a19eb88b958ef8f06b8b748e9745e9754ef3565c4b8cd03fd799` |

Raw URL:
- https://raw.githubusercontent.com/openjdk/jdk/jdk-21+35/src/java.base/share/classes/java/util/zip/CRC32C.java

## The oath is the VENDOR's

OpenJDK's own test suite swears the check value, verbatim:

```
// test/jdk/java/util/zip/TestCRC32C.java
public class TestCRC32C {
    public static void main(String[] args) {
        ChecksumBase.testAll(new CRC32C(), 0xE3069283L);
    }
}
```

`ChecksumBase` (`test/jdk/java/util/zip/ChecksumBase.java`) feeds the canonical
check input `"123456789"` (US-ASCII) and asserts `getValue() == expected`. So
**`0xE3069283` is the value the vendor swore** — the CRC-32C (Castagnoli)
analogue of the canonical CRC-32 check value `0xCBF43926`. We did not author it.
`run.sh` independently cross-checks `CRC-32C("123456789") == 0xE3069283` from a
from-scratch table+update so the oath is double-attested, but the GOOD/BAD
contracts pin the **vendor-sworn** value, not the cross-check.

Source: https://github.com/openjdk/jdk/blob/jdk-21%2B35/test/jdk/java/util/zip/TestCRC32C.java

## What is WALKED, with tree provenance

The construction-site walk enters the `static {}` block (CRC32C.java:85) and
fully unrolls the FIRST table-generation loop (CRC32C.java:88–98):

```
static {                                                          // line 85
  for (int index = 0; index < byteTables[0].length; index++) {    // line 88  bound 256
     int r = index;                                               // line 89  seed
      for (int i = 0; i < Byte.SIZE; i++) {                       // line 90  bound 8
          if ((r & 1) != 0) {                                     // line 91  bit-gate
              r = (r >>> 1) ^ REVERSED_CRC32C_POLY;               // line 92  XOR vs poly
          } else {
              r >>>= 1;                                           // line 94  compound-assign
          }
      }
      byteTables[0][index] = r;                                   // line 97  2-D sub-array store
  }
  ...
}
```

Every node is folded from the AST:

| AST node | Folds to | Mechanism |
|---|---|---|
| `REVERSED_CRC32C_POLY` (line 67) | `0x82F63B78` | `Integer.reverse(CRC32C_POLY)`, `CRC32C_POLY = 0x1EDC6F41` (line 66) — pure-int builtin folded at construction time (`foldIntBuiltin`) |
| `byteTables[0].length` (line 88) | `256` | second dimension of `byteTables = new int[8][256]` (line 75) — allocated length, fixed at construction (`allocatedArrayLength`) |
| `Byte.SIZE` (line 90) | `8` | JLS-fixed bit-width compile-time constant (`constInt` SIZE case) |
| `(r & 1) != 0 ? … : …` (lines 91–95, `if/else`) | `bv32.ite(bv32.ne(bv32.and(r,1),0), …, …)` | statement-form branch-gate folded to `ite` (`execIfGate`) |
| `r >>> 1`, `^`, `r >>>= 1` | `bv32.lshr`, `bv32.xor`, compound `bv32.lshr` | 1:1 operator map (`interpret`, `execCompound`) |
| `byteTables[0][index] = r` (line 97) | store key `byteTables#0` at index | 2-D sub-array store with literal outer index (`arrayStoreKey`) |

**Result (asserted in `run.sh`):** 256 unrolled steps, 23 040 AST nodes
interpreted, and the walked FOL **constant-folds to the real CRC32C table**
(`table[0] = 0x00000000`, `table[255] = 0xAD7D5351`, both verified against an
independent recomputation). This is symbolic execution of the vendor AST, not a
copied table. "silent = 0" is structural: `nodes_walked` is the exact count of
interpreted AST nodes; any uninterpreted node would have produced a refusal and
an early return.

## What is REFUSED BY NAME (close the house)

- **Slicing-by-8 SECOND loop** (CRC32C.java:100–122): builds `byteTables[1..7]`
  from `byteTables[0]` and, on big-endian, byte-reverses every entry. Its inner
  loop is a `for-each (int[] table : byteTables)` whose bound is `table.length`
  on the *loop variable* (not a field) — **refused by name**: "loop bound
  `table.length` is not a literal/static-final int — open/non-literal bound."
  This is sound: the slicing tables are an *optimization*; `byteTables[0]` (the
  one `update(int b)` actually reads on little-endian) is fully walked.

- **The value-pin via the update loop is the NEXT RUNG, named here.** The CHECK
  in this showcase rides the FLOOR tier (the vendor-sworn value as a point
  contract on the real `getValue()` callsite; the BAD suite refutes a wrong
  value by within-test contradiction). Wiring the *walked table* into a
  derivation-tier value-pin — `crc("123456789") == 0xE3069283` as a closed bv32
  tree — requires the keystone to also walk the vendor's stateful instance
  `update(int b)` (CRC32C.java:138–139, `crc = (crc>>>8) ^ byteTable[(crc^(b&0xFF))&0xFF]`)
  over the literal input, and to resolve the `byteTable` field as an alias of
  `byteTables[0]` (set inside the endianness `if/else` at CRC32C.java:108–120).
  That alias and the field-stateful update are the named break between the
  walked construction site and the value-pin; they are **not faked** here.

## JUnit5 assertion vocabulary (for VocabDeriver)

From tag `r5.10.2` of `junit-team/junit5`, License Eclipse Public License v2.0.
Byte-identical to the sibling `java-mt-reference` / `java-b64-strong` showcases.

| File (under `vendor/junit5/`) | sha256 |
|---|---|
| `Assertions.java` | `536e6f91e8b2d5123c5e4441bf680ec6dd9df2ca7a0b2cfa01b9f84f53cd06a6` |
| `AssertEquals.java` | `092762d66b2bb516ffe9fc2ba262a5c63eeb4283d16d42f1fff040ef1f8b4e19` |
| (other Assert*.java delegates) | see `java-mt-reference/good/vendor/junit5/PROVENANCE.md` |

## Honest scope summary

- **Real vendor**: OpenJDK CRC32C, pure Java, GPLv2+CE, sha256-pinned.
- **Real vendor oath**: `TestCRC32C.java` swears `0xE3069283` for `"123456789"`.
- **Real construction-site walk**: the `static {}` table-gen recurrence unrolled
  to FOL that constant-folds to the genuine CRC32C table.
- **CHECK**: GOOD discharges the sworn value on the real callsite; BAD refutes a
  wrong value (within-test contradiction). Real prove + verify receipts.
- **Named break**: the slicing-by-8 second loop, and the value-pin-via-update
  rung (the `byteTable` alias + stateful instance update). Refused, not faked.
