# Vendored source provenance -- java-b64-tails (THE STRONG TIER, MADE TOTAL)

Every vendored file is verbatim from its upstream tag, byte-identical to the
sibling `java-b64-strong` showcase. The per-character tail equations the kit
mints trace, operator by operator and constant by constant (including the '='
pad value), to `com.sun.source` tree nodes in these files. The SMT emitter
contains no Base64 knowledge of its own.

This showcase closes `java-b64-strong`'s declared PHASE-2 gap: the mod-3 tails.
The full-block strong tier handled multiple-of-3 inputs and REFUSED 1/2-byte
tails by name. Here the tails are walked through the SAME symbolic interpreter,
so the encode universe is total: every literal-input length now mints per-
character equations, the 1/2-byte leftovers and the '=' padding included.

## Apache Commons Codec -- tag `rel/commons-codec-1.16.1`

Source: https://github.com/apache/commons-codec, tag `rel/commons-codec-1.16.1`.
License: Apache-2.0. Files are byte-identical to those in the sibling
`java-b64-strong`, `java-codec-universe` and `java-urlsafe-seam` showcases.

| File (under `good/vendor/commons-codec/` and `bad/vendor/commons-codec/`) | Upstream path | sha256 |
|---|---|---|
| `Base64.java` | `src/main/java/org/apache/commons/codec/binary/Base64.java` | `d6e02dcc3b277f5f366724b1b2d74fda3cff1db37ca8ca709db60cd3adee0fdf` |
| `BaseNCodec.java` | `src/main/java/org/apache/commons/codec/binary/BaseNCodec.java` | `930594ae7da6cb20595c4af0f69c7be938a20d089d265bae9d983da496a84e35` |

Raw URLs:
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/Base64.java
- https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/BaseNCodec.java

## What is walked for the tails, with tree provenance

The tail walker symbolically executes the vendor's EOF tail path. The leftover
1 or 2 bytes accumulate into the SAME work area (m accumulations from the
Java-default 0, no block-completion reset); each tail sextet index goes through
the SAME `interpret()` as the full block. Every emitted constant/operator
traces to a node of the vendored source:

### 2-byte tail (`case 2`, 16 bits = 6+6+4)

| Emitted bv-expr piece | Vendor source | Line |
|---|---|---|
| accumulation `work = (work << 8) + b` (run 2x over b0,b1) | `context.ibitWorkArea = (context.ibitWorkArea << 8) + b;` | 778 |
| extraction shift `10` (out0) | `context.ibitWorkArea >> 10 & MASK_6BITS` | 753 |
| extraction shift `4` (out1) | `context.ibitWorkArea >> 4 & MASK_6BITS` | 754 |
| extraction shift `2` (out2, `<<`) | `context.ibitWorkArea << 2 & MASK_6BITS` | 755 |
| 1 pad char `'='` | `if (encodeTable == STANDARD_ENCODE_TABLE) { buffer[..] = pad; }` | 757-758 |

### 1-byte tail (`case 1`, 8 bits = 6+2)

| Emitted bv-expr piece | Vendor source | Line |
|---|---|---|
| accumulation `work = (work << 8) + b` (run 1x over b0) | `context.ibitWorkArea = (context.ibitWorkArea << 8) + b;` | 778 |
| extraction shift `2` (out0) | `context.ibitWorkArea >> 2 & MASK_6BITS` | 742 |
| extraction shift `4` (out1, `<<`) | `context.ibitWorkArea << 4 & MASK_6BITS` | 744 |
| 2 pad chars `'='` | `if (encodeTable == STANDARD_ENCODE_TABLE) { buffer[..]=pad; buffer[..]=pad; }` | 746-748 |

### Common

| Emitted bv-expr piece | Vendor source | Line |
|---|---|---|
| 6-bit mask `0x3f` (`MASK_6BITS`) | `private static final int MASK_6BITS = 0x3f;` (field-resolved) | 129 |
| operators `<<`/`>>`/`&`/`+` → `bvshl`/`bvlshr`/`bvand`/`bvadd` | `BinaryTree` node kinds | 742-755 |
| **pad codepoint `61` (`'='`)** | `pad` field ← ctor param ← `super(...,PAD_DEFAULT,...)` ← `PAD_DEFAULT='='` (resolved through the SAME chain the weak tier uses; NEVER typed) | BaseNCodec.java:179 |
| 64 table codepoints | `STANDARD_ENCODE_TABLE` / `URL_SAFE_ENCODE_TABLE` `NewArrayTree` literals (resolved via the existing G1 selector) | 75 / 99 |

The pad WRITE is table-specific: the vendor guards it with
`if (encodeTable == STANDARD_ENCODE_TABLE)`. The walker reads that guard, so
the URL-SAFE table emits NO pad (verified by kit TEST 71). The pad COUNT (1 for
a 2-byte tail, 2 for a 1-byte tail) is the literal input's length mod 3 -- a
structural fact of the literal, not a vendor constant.

## Inputs chosen and their encodings

- `"ba"` (UTF-8, 2 bytes = a 2-byte tail). Standard base64: `YmE=`
  python3: `base64.b64encode(b'ba').decode() == 'YmE='`
- `"f"` (UTF-8, 1 byte = a 1-byte tail). Standard base64: `Zg==`
  python3: `base64.b64encode(b'f').decode() == 'Zg=='`

The bad twin asserts `"YmX="` for `encode("ba")`. The correct value is `"YmE="`.
`"YmX="` is ALPHABET-VALID: `Y`, `m`, `X` are all members of the standard table,
and `'='` is the sworn pad char (the weak universe includes it for the standard
table, walked from the vendor's own pad guard). So the WEAK tier
(`str.chars-in-set`) discharges it -- the lie lives inside the alphabet. Only
the tail equations refute it: `out2 = table[(work << 2) & 0x3f] = table[4] = 'E'`,
not `'X' = table[23]`. The conjunction (sworn `"YmX="` ∧ tail-equation `"YmE="`)
is UNSAT.
