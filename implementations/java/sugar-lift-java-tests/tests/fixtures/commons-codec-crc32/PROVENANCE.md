# Vendored source provenance - commons-codec-crc32 fixture

The CRC implementation source is vendored verbatim from Apache Commons Codec tag
`rel/commons-codec-1.16.1`.

| File | Upstream path | sha256 |
|---|---|---|
| `vendor/commons-codec/org/apache/commons/codec/digest/PureJavaCrc32.java` | `src/main/java/org/apache/commons/codec/digest/PureJavaCrc32.java` | `62af32a1ea4b3252d1111cd8ccadd8446d0a5a2e54964f7c89dcb50a736e034d` |

Raw URL:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/digest/PureJavaCrc32.java

Vendor test warrant:
- `src/test/java/org/apache/commons/codec/digest/PureJavaCrc32Test.java`
- Raw URL: https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/test/java/org/apache/commons/codec/digest/PureJavaCrc32Test.java
- The vendor test compares `PureJavaCrc32` against `java.util.zip.CRC32`
  through `checkSame()` after per-byte updates and byte-array updates.

The local harness exposes the standard CRC32 check value for `"123456789"` so the
current Java kit can emit a closed `crc32.eq-walked` value-pin contract. The
source audit itself resolves the SourceMemento back to the vendored
`PureJavaCrc32.java` bytes above.
