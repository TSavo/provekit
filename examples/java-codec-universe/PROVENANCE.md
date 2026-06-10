# Vendored source provenance — java-codec-universe

Every vendored file is verbatim from its upstream tag. The universe walked by
the kit traces, character by character, to `LiteralTree` nodes in these files.

## Apache Commons Codec — tag `rel/commons-codec-1.16.1`

Source: https://github.com/apache/commons-codec, tag `rel/commons-codec-1.16.1`.
License: Apache-2.0.

| File (under `good/vendor/commons-codec/` and `bad/vendor/commons-codec/`) | Upstream path | sha256 |
|---|---|---|
| `Base64.java` | `src/main/java/org/apache/commons/codec/binary/Base64.java` | `d6e02dcc3b277f5f366724b1b2d74fda3cff1db37ca8ca709db60cd3adee0fdf` |
| `BaseNCodec.java` | `src/main/java/org/apache/commons/codec/binary/BaseNCodec.java` | `930594ae7da6cb20595c4af0f69c7be938a20d089d265bae9d983da496a84e35` |

Raw URLs:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/Base64.java
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/BaseNCodec.java

## Vendor sample citation (the gate)

The GOOD suite's assertion is the vendor's own RFC 4648 section-10 vector,
verbatim from the vendor's test suite at the same tag:

- `src/test/java/org/apache/commons/codec/binary/Base64Test.java`, line 878:
  `assertEquals("Zm9v", Base64.encodeBase64String(StringUtils.getBytesUtf8("foo")));`
- Raw URL: https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/test/java/org/apache/commons/codec/binary/Base64Test.java
- sha256 of `Base64Test.java` at that tag:
  `ef97352ff2460ff416ae5850dfbb38fc36064c7ac1f16fba6f14fe224ebb1604`
  (cited, not vendored — the GOOD suite carries the assertion itself).

The BAD suite's input (`encodeBase64URLSafeString` over `"bar"`) appears in
NO assertion of the vendor's test suite at this tag — that is the point: the
refutation comes from the walked universe, not from a vendor vector.

## JUnit framework source (assertion vocabulary)

`good/vendor/junit5/` and `bad/vendor/junit5/` are copied from
`examples/java-assertion-consistency/*/vendor/junit5/` — see the
`PROVENANCE.md` inside those directories (junit5 tag `r5.10.2`).
