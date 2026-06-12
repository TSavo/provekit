# Vendored source provenance - java-commons-codec-crc32

The CRC implementation source is vendored verbatim from Apache Commons Codec tag
`rel/commons-codec-1.16.1`.

| File (under `good/vendor/commons-codec/` and `bad/vendor/commons-codec/`) | Upstream path | sha256 |
|---|---|---|
| `org/apache/commons/codec/digest/PureJavaCrc32.java` | `src/main/java/org/apache/commons/codec/digest/PureJavaCrc32.java` | `62af32a1ea4b3252d1111cd8ccadd8446d0a5a2e54964f7c89dcb50a736e034d` |

Raw URL:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/digest/PureJavaCrc32.java

Vendor test warrant:
- `src/test/java/org/apache/commons/codec/digest/PureJavaCrc32Test.java`
- Raw URL: https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/test/java/org/apache/commons/codec/digest/PureJavaCrc32Test.java
- The vendor test compares `PureJavaCrc32` against `java.util.zip.CRC32`
  through `checkSame()` after both per-byte updates and byte-array updates.

This showcase exposes the standard CRC32 check value for `"123456789"` through
the vendor's byte-array update path. The source audit resolves the SourceMemento
back to the vendored `PureJavaCrc32.java` bytes above and accounts for the
slicing-by-8 table relation in lines 605-612.
