# Vendored source provenance — java-crc32-valuepin

THE VALUE-PIN RUNG. The merged construction-site walk (`RecurrenceUniverseWalker`,
#2046) folds OpenJDK CRC32C's genuine lookup table from its `static {}` initializer
(see `java-crc32-universe/PROVENANCE.md`). This showcase **connects that folded
table to the value**: it symbolically WALKS the vendor's stateful instance
`update(int b)` over the canonical literal input `"123456789"`, reading the folded
table at each concrete index, then applies `getValue()`'s final inversion — pinning
`crc("123456789") == value` as **one closed bv32 FOL**. The SMT emitter and the
walker contain no CRC knowledge of their own.

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
**`0xE3069283` is the value the vendor swore** — the CRC-32C (Castagnoli) analogue
of the canonical CRC-32 check value `0xCBF43926`. We did not author it. `run.sh`
independently cross-checks it from a from-scratch table+update; the GOOD/BAD
contracts pin the **vendor-sworn** value.

Source: https://github.com/openjdk/jdk/blob/jdk-21%2B35/test/jdk/java/util/zip/TestCRC32C.java

## Vendor: OpenJDK `java.util.zip.CRC32C` — tag `jdk-21+35`

Source: https://github.com/openjdk/jdk, tag `jdk-21+35`.
License: GPLv2 with the Classpath Exception (license header intact in the file).
A **pure-Java** checksum: it builds its 8×256 lookup table in a `static {}` block
from the reversed Castagnoli polynomial and runs the classic reflected update.

| File (under `good/vendor/jdk-crc32c/` and `bad/vendor/jdk-crc32c/`) | Upstream path | sha256 |
|---|---|---|
| `CRC32C.java` | `src/java.base/share/classes/java/util/zip/CRC32C.java` | `ac23a23e3527a19eb88b958ef8f06b8b748e9745e9754ef3565c4b8cd03fd799` |

Raw URL:
- https://raw.githubusercontent.com/openjdk/jdk/jdk-21+35/src/java.base/share/classes/java/util/zip/CRC32C.java

## What is WALKED (the value-pin rung), with tree provenance

1. **Static-init table** (CRC32C.java:85–98): the merged construction-site walk
   fully unrolls the table-generation recurrence (256 steps), folding the
   polynomial from `Integer.reverse(CRC32C_POLY)` (line 67), the bound 256 from
   `byteTables[0].length` (line 88), the bound 8 from `Byte.SIZE` (line 90), and
   the bit-gate `if ((r&1)!=0) … else …` (lines 91–95) to a `bv32.ite`. Each table
   entry is the genuine folded value (table[0]=0x00000000, table[255]=0xAD7D5351).

2. **The `byteTable` alias** (CRC32C.java:109–110): `update()` reads the field
   `byteTable`, which the static initializer's endianness `if/else` aliases to
   `byteTables[0]` on `LITTLE_ENDIAN`. The value-pin walk RESOLVES the alias by
   walking that `if/else` to the branch whose RHS is the folded sub-array
   `byteTables[0]` → store key `byteTables#0`. **If the alias were not statically
   resolvable to a folded sub-array, the walk REFUSES BY NAME** (no branch guess —
   see the `crc-valuepin-noalias` kit-test fixture).

3. **The stateful update** (CRC32C.java:138–139):
   `crc = (crc >>> 8) ^ byteTable[(crc ^ (b & 0xFF)) & 0xFF]`
   is walked over the 9 literal bytes of `"123456789"`. The `crc` instance state is
   threaded as SSA (crc_0 = 0xFFFFFFFF from the field initializer at line 125;
   crc_{i+1} = f(crc_i, b_i, table)). Each step reads the folded table at the index
   `(crc_i ^ b_i) & 0xff` — **concrete once crc_i is threaded** (a nested constant
   index into the merged folded table). Each operator (`>>>`, `^`, `&`) maps 1:1 to
   a `bv32.lshr` / `bv32.xor` / `bv32.and` node.

4. **The inversion** (CRC32C.java:210–211): `getValue()` returns
   `(~crc) & 0xFFFFFFFFL`. The walk confirms this shape and applies it
   (`~crc` = `crc ^ -1`; `& 0xFFFFFFFF` is a no-op on bv32).

**Result:** one closed bv32 tree with **no free variables** that constant-folds to
the genuine `0xE3069283`. It is symbolic execution of the vendor AST — table-gen +
stateful update + inversion — not a copied value.

## The CHECK — the universe does the work

The kit emits a self-contained `crc32.eq-walked(<asserted>, <walked crc-FOL>)`
contract whose invariant is the single equation `(= <asserted_hex> <walked_smt>)`,
rendered by the SMT emitter (the walked FOL carried as a String-const payload, its
bv32 nodes rendered from raw JSON — mirroring the base64 strong tier).

- **GOOD** (`good/.../Crc32cValuePinTest.java`): asserts the vendor-sworn
  `0xE3069283` on `getValue()` after checksumming `"123456789"` byte-by-byte
  through the real `update(int)`. The value-pin equation `(= #xe3069283 <walked>)`
  is **sat → discharged**: the sworn value IS the value the walked computation
  produces.
- **BAD** (`bad/.../Crc32cWrongValueTest.java`): a **single** wrong assertion,
  `0xE3069284`. The value-pin equation `(= #xe3069284 <walked>)` is **unsat →
  unsatisfied — refuted BY THE WALKED TABLE+UPDATE COMPUTATION**, NOT by a
  within-test contradiction. There is exactly one assertion; the floor
  `::assertion` point row alone is *discharged* (a lone point claim is satisfiable),
  and only the value-pin row refutes. The universe does the work, like
  `java-b64-strong` refuting `"ZmFy"`. This is distinct from the
  `java-crc32-universe` floor showcase's within-test contradiction (two assertions
  on one callsite).

## What is REFUSED BY NAME (close the house)

- **Unresolvable `byteTable` alias** → REFUSE (no branch guess, no faked table
  read). Tested in `tests/fixtures/crc-valuepin-noalias`.
- **Non-literal update input** → no value-pin (floor only), named. The input must
  be reconstructable from literal `update(int)` / `update(byte[],0,len)` callsites.
- **`updateBytes` slicing-by-8 path** (CRC32C.java:218–287): for inputs ≥ 8 bytes
  the array-update path enters the slicing-by-8 loops (refused by the
  construction-site walk on their `.length` bounds). The value-pin therefore drives
  the per-byte `update(int)` callsite, whose table read is fully walked.

## JUnit5 assertion vocabulary (for VocabDeriver)

From tag `r5.10.2` of `junit-team/junit5`, License Eclipse Public License v2.0.
Byte-identical to the sibling `java-crc32-universe` / `java-mt-reference` /
`java-b64-strong` showcases.

## Honest scope summary

- **Real vendor**: OpenJDK CRC32C, pure Java, GPLv2+CE, sha256-pinned.
- **Real vendor oath**: `TestCRC32C.java` swears `0xE3069283` for `"123456789"`.
- **Real value-pin walk**: static-init table-gen (256 steps) + stateful
  `update(int)` over `"123456789"` (9 steps) + `getValue()` inversion → one closed
  bv32 FOL that constant-folds to the genuine value. The `byteTable` alias is
  resolved by walking the endianness `if/else`.
- **CHECK**: GOOD discharges the sworn value AGAINST the walked computation; BAD's
  single wrong value is refuted UNSAT BY the walked computation (not a
  contradiction). Real prove + verify receipts.
- **Named refusals**: unresolvable table alias; non-literal input; the slicing-by-8
  array-update path. Refused, not faked.
