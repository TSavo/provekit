# Vendored source provenance -- java-urlsafe-seam

Every vendored file is verbatim from its upstream tag. The universe walked by
the kit traces, character by character, to `LiteralTree` nodes in these files.

This showcase is the marquee of paper 26: "the bad twin asserts the URL-safe
confusion on an input the vendor never tested and the real CLI returns
unsatisfied."

## Apache Commons Codec -- tag `rel/commons-codec-1.16.1`

Source: https://github.com/apache/commons-codec, tag `rel/commons-codec-1.16.1`.
License: Apache-2.0.

Files are identical copies of those in `examples/java-codec-universe/*/vendor/commons-codec/`.

| File (under `good/vendor/commons-codec/` and `bad/vendor/commons-codec/`) | Upstream path | sha256 |
|---|---|---|
| `Base64.java` | `src/main/java/org/apache/commons/codec/binary/Base64.java` | `d6e02dcc3b277f5f366724b1b2d74fda3cff1db37ca8ca709db60cd3adee0fdf` |
| `BaseNCodec.java` | `src/main/java/org/apache/commons/codec/binary/BaseNCodec.java` | `930594ae7da6cb20595c4af0f69c7be938a20d089d265bae9d983da496a84e35` |

Raw URLs:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/Base64.java
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/BaseNCodec.java

## Input chosen and its encodings

Input: `"provekit~seam"` (UTF-8, 13 bytes)

Standard base64 (RFC 4648):
  `cHJvdmVraXR+c2VhbQ==`
  python3: `base64.b64encode(b'provekit~seam').decode()`

URL-safe base64 (RFC 4648 section 5):
  `cHJvdmVraXR-c2VhbQ==`
  python3: `base64.urlsafe_b64encode(b'provekit~seam').decode()`

The standard encoding contains `+` (at position 12). The URL-safe spelling
replaces that `+` with `-`. The character `-` is NOT a member of the
`STANDARD_ENCODE_TABLE` walked from `vendor/commons-codec/Base64.java`.

The vendor's test suite (`Base64Test.java`, tag `rel/commons-codec-1.16.1`)
has no assertion over `"provekit~seam"`. run.sh confirms this by grepping the
vendored source and asserting absence.

## The marquee story

GOOD suite: the consumer asserts the correct standard encoding
`"cHJvdmVraXR+c2VhbQ=="` for an input the vendor never tested. The equality
row and universe row (str.chars-in-set over STANDARD_ENCODE_TABLE) conjoin
consistently: SAT, discharged. `+` IS in the standard table.

BAD suite: the consumer asserts the URL-safe spelling `"cHJvdmVraXR-c2VhbQ=="`
-- the classic standard-vs-urlsafe confusion. `-` is NOT in STANDARD_ENCODE_TABLE.
The universe row conjoins with the equality claim: UNSAT, unsatisfied. The
refutation comes from the universe walked from the vendor's source, not from
any point sample. No vendor test ever spoke the word "provekit~seam".

That asymmetry is the product: a false claim about an input that was never
tested is refuted by the universe's membership constraint, not by luck of
collision with a vendor vector.

## Honest scope

Weak tier: membership constraint only (str.chars-in-set over the static final
encode table). The per-block function (precise 24-bit accumulator, mod-3 tail
branches, pad logic) is not modeled -- that is the named gap in paper 26,
scheduled work, not memoir.

## JUnit framework source (assertion vocabulary)

`good/vendor/junit5/` and `bad/vendor/junit5/` are copied from
`examples/java-codec-universe/*/vendor/junit5/` (junit5 tag `r5.10.2`).
See `examples/java-codec-universe/PROVENANCE.md` for the JUnit provenance
chain.
