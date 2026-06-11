# Vendored source provenance -- java-b64-strong (THE STRONG TIER)

Every vendored file is verbatim from its upstream tag. The per-character block
equations the kit mints trace, operator by operator and constant by constant,
to `com.sun.source` tree nodes in these files. The SMT emitter contains no
Base64 knowledge of its own.

This showcase realizes paper 26's #1 residue item: *"the closed-form 24-bit
work area was synthesized from the walked per-byte accumulation -- production
needs a real symbolic-execution step there, not a pattern match (this is THE
seam between tiers)."* The seam is now a real symbolic-execution pass.

## Apache Commons Codec -- tag `rel/commons-codec-1.16.1`

Source: https://github.com/apache/commons-codec, tag `rel/commons-codec-1.16.1`.
License: Apache-2.0. Files are byte-identical to those in the sibling
`java-codec-universe` and `java-urlsafe-seam` showcases.

| File (under `good/vendor/commons-codec/` and `bad/vendor/commons-codec/`) | Upstream path | sha256 |
|---|---|---|
| `Base64.java` | `src/main/java/org/apache/commons/codec/binary/Base64.java` | `d6e02dcc3b277f5f366724b1b2d74fda3cff1db37ca8ca709db60cd3adee0fdf` |
| `BaseNCodec.java` | `src/main/java/org/apache/commons/codec/binary/BaseNCodec.java` | `930594ae7da6cb20595c4af0f69c7be938a20d089d265bae9d983da496a84e35` |

Raw URLs:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/Base64.java
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/BaseNCodec.java

## What is walked, with tree provenance

The strong-tier walker symbolically executes the vendor's full-block encode
path. Every emitted constant and operator traces to a node of `Base64.java`:

| Emitted bv-expr piece | Vendor source | Line |
|---|---|---|
| accumulation `work = (work << 8) + b` (run 3x) | `context.ibitWorkArea = (context.ibitWorkArea << 8) + b;` | 778 |
| accumulation shift `8` | the `<< 8` literal in line 778 | 778 |
| extraction shift `18` (out0) | `context.ibitWorkArea >> 18 & MASK_6BITS` | 780 |
| extraction shift `12` (out1) | `context.ibitWorkArea >> 12 & MASK_6BITS` | 781 |
| extraction shift `6` (out2) | `context.ibitWorkArea >> 6 & MASK_6BITS` | 782 |
| extraction shift `0` (out3, bare `& MASK_6BITS`) | `context.ibitWorkArea & MASK_6BITS` | 783 |
| 6-bit mask `0x3f` (`MASK_6BITS`) | `private static final int MASK_6BITS = 0x3f;` (field-resolved) | 129 |
| operators `<<`/`>>`/`&`/`+` → `bvshl`/`bvlshr`/`bvand`/`bvadd` | `BinaryTree` node kinds | 778-783 |
| 64 table codepoints (index → codepoint) | `STANDARD_ENCODE_TABLE` / `URL_SAFE_ENCODE_TABLE` `NewArrayTree` literals (resolved via the existing G1 selector) | 75 / 99 |

The table SELECTION (standard vs urlsafe) goes through the existing weak-tier
selector machinery (`encodeBase64String` → standard, `encodeBase64URLSafeString`
→ urlsafe); the strong row uses whichever table the existing walk resolved.

## Input chosen and its encoding

Input: `"bar"` (UTF-8, 3 bytes = exactly one full block, no mod-3 tail).

Standard base64: `YmFy`
  python3: `base64.b64encode(b'bar').decode() == 'YmFy'`

The bad twin asserts `"ZmFy"` (which is `encode("foo")`). `"ZmFy"` is
ALPHABET-VALID: every char (Z, m, F, y) is a member of the standard table, so
the WEAK tier (`str.chars-in-set`) discharges it. Only the strong-tier block
equations refute it: `out0 = table[(work >> 18) & 0x3f] = table[24] = 'Y'`, not
`'Z' = table[25]`. The conjunction is UNSAT -> unsatisfied.

## HONEST SCOPE (the named gaps -- the walk-or-silence law)

This is **PHASE 1: full 3-byte blocks only.** The discipline is total-accounting:
what is not walked is named, never approximated or faked.

- **Multiple-of-3 inputs only.** A callsite whose string-literal input has
  length `len % 3 == 0` (a whole number of full blocks) gets the strong row.
  The equations are UNROLLED for that concrete byte count -- a finite
  conjunction, one bv-equation per output character, no quantifiers, no index
  arithmetic.

- **mod-3 tails are PHASE 2, REFUSED BY NAME.** The 1-byte and 2-byte tails
  (`Base64.java:740-760`: the `switch (modulus)` cases that emit 2 or 3 chars
  plus `'='` padding, with the `encodeTable == STANDARD_ENCODE_TABLE` pad guard)
  are walked here only as a **named refusal**. A non-multiple-of-3 callsite gets
  the WEAK row alone, plus a diagnostic:
  `"strong universe refused: input length N is not a multiple of 3 -- the mod-3
  tail (Base64.java:740-760, 1/2-byte block + '=' padding) is PHASE 2 and not
  yet walked; weak tier (str.chars-in-set) emitted alone"`.
  An honest partial beats a faked total.

- **Non-literal input → no strong row.** Only a single string literal (via the
  existing `getBytesUtf8`/`getBytes` bridge) has a known byte length. A
  non-literal argument gets the weak row only (the strong gate simply does not
  fire; the weak walker names its own refusals).

- **Uninterpretable encode-body shape → REFUSED BY NAME.** If the symbolic pass
  meets a statement/expression it cannot interpret (a method call inside the
  index expression, a non-literal shift, an unsupported operator), the strong
  row is refused with a named `<strong-universe-walker>` diagnostic and the weak
  row stands. The interpreter handles only: the work-area local, int literals,
  static-final int fields (mask/shift constants), and `<<`/`>>`/`>>>`/`&`/`|`/`+`.

- **`>>` rendered as `bvlshr`.** Java `>>` is arithmetic, but the vendor masks
  the result with `& MASK_6BITS` (6 bits) and the work area is a non-negative
  24-bit value, so `bvlshr` and `bvashr` agree on the masked output. The
  sample-gate confirms it end-to-end: z3 derives `encode("bar") == "YmFy"`,
  and the conjunction with the sworn equality is consistent.

## The gate

`run.sh` runs real `sugar mint -> sugar prove -> sugar verify` and parses real
JSON receipts (verdicts from parsed consistency-row statuses, not exit codes):

- GOOD: `assertEquals("YmFy", encodeBase64String(getBytesUtf8("bar")))` -> discharged.
- BAD: `assertEquals("ZmFy", ...)` -> unsatisfied; prints the line
  *"ZmFy is alphabet-valid -- the weak tier alone would discharge it; the block
  equations refuted it."*
- DERIVE: `sugar derive --blocks-payload <extracted from the minted .proof>`
  -> z3.model computes `"YmFy"` -- derived from the walked equations, not executed.
