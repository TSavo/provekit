# Base64 generalization prototype — the test selects, the body generalizes, the solver gates

**Status:** research prototype, parallel exploratory work. Does not touch the Java
lifter kit, the verifier engine, the Makefile, or any showcase.
**Branch:** `kit/research-base64-generalization`
**Artifacts:** `menagerie/base64-generalization/` (walker, emitter, `run.sh`,
vendored source + `PROVENANCE.md`).

## The thesis under test

> "Here's your point sample from the vendor test: `encode("xyz") = Y`. Now go walk
> the AST of the encode implementation, and report the constraints on Y. Whatever
> constraints you found? That's a generalization. Now let the solver solve for Y."

The vendor test swears a contract exists at the callsite and gives one sample. The
implementation body, walked through its own grammar, yields the constraints. The
solver closes the loop: constraints + input must re-derive the vendor's own sample.
That re-derivation is the soundness gate — the vendor verifies our generalization
of the vendor.

## The point sample (real vendor vector)

`vendor/Base64Test.java`, line 878 (RFC 4648 §10 vector "foo"):

```java
assertEquals("Zm9v", Base64.encodeBase64String(StringUtils.getBytesUtf8("foo")));
```

So the sworn point is `encodeBase64String([0x66,0x6f,0x6f]) = "Zm9v"`.

## The law obeyed

Every constraint below is traceable to an AST node of the vendored source, walked
with `com.sun.source` (`walker/Base64Walker.java`, `javac --release 21`,
`-proc:none`). No regex/string-scan of Java drives a constraint; no base64 fact is
hand-authored. The SMT emitter (`emit_smt.js`) reads **only** the walker's JSON
output — it never opens the Java source. The one structural model the emitter
fixes (how `ibitWorkArea` accumulates input bytes) is itself a walked statement,
called out explicitly in the residue section.

## What the walk found (tree-derived facts)

The walker emits a JSON fact document. Each fact carries the source file + line of
the AST node it came from. Verbatim (`walker.json`):

### The alphabet — `STANDARD_ENCODE_TABLE`

- Node: `VariableTree` named `STANDARD_ENCODE_TABLE`, `Base64.java:75`.
- Modifiers walked off `node.getModifiers().getFlags()`: **`private, static, final`**.
  The `static final` is the immutability axiom that makes a universal over the
  table sound — the array is a compile-time constant, so a quantifier over "every
  output byte is a table member" cannot be invalidated by mutation.
- Initializer is a `NewArrayTree` of 64 `LiteralTree` char literals. Walked byte
  values (in order): `[65..90, 97..122, 48..57, 43, 47]` = `A–Z a–z 0–9 + /`.
  Length 64, confirmed structurally (not assumed).

### The pad and the constants

- `PAD_DEFAULT = '='` → byte `61`, `BaseNCodec.java:179`.
- `MASK_6BITS = 0x3f` → `63`, `Base64.java:129`.
- `BITS_PER_ENCODED_BYTE = 6` (`:63`), `BYTES_PER_UNENCODED_BLOCK = 3` (`:64`),
  `BYTES_PER_ENCODED_BLOCK = 4` (`:65`). All walked as `LiteralTree` initializers.

### The bit arithmetic — `encode(byte[],int,int,Context)`

The walker classifies every `buffer[context.pos++] = encodeTable[ <idx> & MASK ];`
assignment by the structural region it sits in (full-block `if (0==modulus)` vs the
tail `switch (modulus)` cases), reading the top operator of `<idx>` (a `BinaryTree`
of kind `AND`, whose left operand is a `>>`/`<<` `BinaryTree` or the bare work
area). Walked records:

| Region | Emit order | op | amount | source node | line |
|--------|-----------|----|--------|-------------|------|
| full 3-byte block (`modulus==0`) | y0 | `>>` | 18 | `context.ibitWorkArea` | 780 |
| | y1 | `>>` | 12 | | 781 |
| | y2 | `>>` | 6 | | 782 |
| | y3 | bare `& MASK` | 0 | | 783 |
| tail `modulus==1` (8 bits) | c0 | `>>` | 2 | | 742 |
| | c1 | `<<` | 4 | | 744 |
| | + 2 pad bytes | | | (guarded `encodeTable==STANDARD_ENCODE_TABLE`) | 746–749 |
| tail `modulus==2` (16 bits) | c0 | `>>` | 10 | | 753 |
| | c1 | `>>` | 4 | | 754 |
| | c2 | `<<` | 2 | | 755 |
| | + 1 pad byte | | | (guarded `encodeTable==STANDARD_ENCODE_TABLE`) | 757–759 |

That is the full RFC base64 group arithmetic — six shift amounts, one bare mask,
and the mod-3 padding structure — and not one number of it was typed from memory.
It fell out of the tree.

## The constraints on Y (two tiers)

Both tiers are emitted into SMT-LIB2 over bitvectors (`QF_ABV`). The table is an
SMT array `(_ BitVec 6) -> (_ BitVec 8)`, built by 64 nested `store`s directly from
the walked literal (not typed). Input bytes are `(_ BitVec 8)`.

### Weak tier — alphabet membership + length

`inAlphabet(c)` is the disjunction "`c` equals one of the 64 walked table bytes, or
the walked pad byte." This is the universal the `static final` modifier licenses:
every output byte of a standard-table encode lies in this set. (The length relation
`|Y| = 4·ceil(|x|/3)` is implied by the walked block structure — 4 chars per
3-byte block, pad-filled — and is reported but not the load-bearing part of the
weak refutation.) Verbatim head of `weak_alphabet.smt2`:

```smt2
(define-fun inAlphabet ((c (_ BitVec 8))) Bool (or (= c (_ bv65 8)) ... (= c (_ bv47 8)) (= c (_ bv61 8))))
(declare-const yk (_ BitVec 8))
(assert (inAlphabet yk))
(assert (= yk (_ bv45 8)))   ; consumer claims this byte is '-' (url-safe)
(check-sat)
```

### Strong tier — the full per-group bit equations

For the 3-byte block, `ibitWorkArea` is the 24-bit word `(b0<<16)|(b1<<8)|b2`, and
each output char is `T[(w OP amount) & MASK]` with `OP`/`amount` from the walked
records. Verbatim core of `strong_derive.smt2`:

```smt2
(define-fun w () (_ BitVec 24)
  (bvor (bvor (bvshl ((_ zero_extend 16) b0) (_ bv16 24))
              (bvshl ((_ zero_extend 16) b1) (_ bv8 24)))
        ((_ zero_extend 16) b2)))
(assert (= y0 (select T ((_ extract 5 0) (bvand (bvlshr w (_ bv18 24)) (_ bv63 24))))))
(assert (= y1 (select T ((_ extract 5 0) (bvand (bvlshr w (_ bv12 24)) (_ bv63 24))))))
(assert (= y2 (select T ((_ extract 5 0) (bvand (bvlshr w (_ bv6  24)) (_ bv63 24))))))
(assert (= y3 (select T ((_ extract 5 0) (bvand            w           (_ bv63 24))))))
(assert (= b0 (_ bv102 8)))   ; 'f'
(assert (= b1 (_ bv111 8)))   ; 'o'
(assert (= b2 (_ bv111 8)))   ; 'o'
```

Note `(_ bv18 24)`, `(_ bv12 24)`, `(_ bv6 24)`, the bare-mask last char, and
`(_ bv63 24)` are the walked shift amounts and `MASK_6BITS` — emitted, not authored.

## The z3 results (verbatim, all four checks)

Run via `menagerie/base64-generalization/run.sh` (z3 4.15.4, OpenJDK 25 building
`--release 21`). The driver parses the model bytes and compares them to the vendor
vector; honest exit code.

**A. `strong_derive` — derive Y for x="foo" (expect sat; Y == vendor):**

```
sat
((y0 #x5a)
 (y1 #x6d)
 (y2 #x39)
 (y3 #x76))
PASS A: z3 derived Y = 90,109,57,118 == vendor 90,109,57,118 ("Zm9v")
```

`0x5a 0x6d 0x39 0x76` = `Z m 9 v` = **"Zm9v"**, byte-for-byte the vendor's sworn
output. The solver, given only the walked constraints and the input, re-derived the
vendor's own sample. **∀ ⊨ sample.**

**B. `strong_unique` — x pinned, assert Y ≠ "Zm9v" (expect unsat):**

```
unsat
PASS B: Y pinned uniquely
```

Under the walked constraints there is no other Y for this input — the
generalization pins Y uniquely, it is not merely consistent with the sample.

**C. `refute_alphabet` — free input, assert some output char is '-' or '_' (expect unsat):**

```
unsat
PASS C: no standard-table output is '-'/'_'
```

This is the url-safe-confusion refutation, and it is the payoff: it holds for
**any** 3-byte input, not just the vendor's vector. We can now reject "this standard
encoder emitted a url-safe character" without a vector collision — the
generalization made an infinite family of claims checkable from one sample.

**D. `weak_alphabet` — a byte claimed in-alphabet and equal to '-' (expect unsat):**

```
unsat
PASS D: out-of-alphabet claim refuted
```

The weak tier alone already refutes the url-safe confusion at the alphabet level —
cheaper than the strong tier, and it does not need the bit equations.

## Honest accounting — what did NOT reduce structurally

The unwalkable residue is a finding, not a failure.

1. **The byte-accumulation model is the seam.** The walker records that the work
   area is built by the per-byte statement `ibitWorkArea = (ibitWorkArea << 8) + b`
   (`Base64.java`, in the `else` loop) and that the full block fires when
   `0 == modulus` after three bytes. But the emitter, not the walker, turns that
   into the closed-form 24-bit word `(b0<<16)|(b1<<8)|b2`. A production lifter would
   have to symbolically execute / unroll the accumulation loop to derive that word
   from the tree, rather than pattern-matching the known shape. This is the single
   place the prototype hand-waved the structural→symbolic step, and it is named
   here plainly.

2. **The tail branches (mod-3 = 1, 2) are walked but not solved end-to-end.** The
   walker fully extracts their shift records and pad counts (table above), so the
   strong constraints for 1- and 2-byte tails are derivable; the prototype solves
   the 3-byte full block end-to-end (the "foo" vector) and leaves the tail SMT
   generation as the obvious next emit. The facts are in hand; only the emitter
   arithmetic for the partial work areas is unwritten.

3. **The pad guard `if (encodeTable == STANDARD_ENCODE_TABLE)` is walked but its
   reference-equality semantics are not modeled.** The walker sees the pads sit
   under that guard (and counts 2 pads for mod-1, 1 for mod-2), which is exactly
   why the standard alphabet pads and the url-safe one does not. Turning "this `==`
   is reference identity against the standard table field" into a sound model
   requires reasoning the prototype does not attempt.

4. **Constructor parameter flow (`urlSafe` → `encodeTable`) is unwalked.** Line 612,
   `this.encodeTable = urlSafe ? URL_SAFE_ENCODE_TABLE : STANDARD_ENCODE_TABLE;`,
   is the data-flow that decides *which* table the instance uses. The
   `encodeBase64String` path constructs a non-url-safe `Base64`, so the standard
   table is the right pin — but the prototype asserts that by reading the call
   chain by eye (`encodeBase64String` → `encodeBase64(data,false)` →
   `new Base64(0)` → standard table), not by walking the ternary + the constructor
   argument flow. A production lifter must follow that flow to know which alphabet
   the contract is over.

5. **Chunking / line-wrapping config (`lineLength`, `lineSeparator`, CRLF
   insertion) is unmodeled.** `encodeBase64String` uses the non-chunking path
   (`lineLength == 0`), and the prototype simply does not emit the line-separator
   branch. For a chunked encoder the output-length relation and interleaved CRLFs
   would all need walking.

6. **Streaming state in `BaseNCodec` (the `Context` object, `eof`, `pos`, buffer
   resizing) is purity residue.** The prototype treats the output as a pure
   function of the input bytes. The walk shows `Context` is per-call scratch state,
   which supports that read, but the prototype does not *establish* purity
   structurally (no aliasing / escape analysis). Reported as an assumption, not a
   proven fact.

## What this implies for the post-P5 generalization campaign

The thesis holds on a real vendor library with a real RFC vector: **the test
selected the callsite and sample, the body (walked) generalized it, and the solver
gated the generalization against the vendor's own sworn output and against an
infinite family of confusion claims.** Concretely, the design it points at:

- **test-selects:** the lifter harvests `(input, output)` point samples from vendor
  `assertEquals` callsites — already the "vendor tests ARE the spec" principle, now
  with the sample as the *gate input* rather than the *whole contract*.
- **body-generalizes:** an AST walk of the called implementation emits the
  constraint set (alphabet membership from `static final` table literals; bit
  equations from the shift/mask tree). The weak tier (membership + length) is cheap
  and falls out of any table-driven codec; the strong tier needs the loop→symbolic
  step (residue item 1).
- **sample-gates:** the solver must re-derive every harvested sample from the
  constraints (soundness, check A), and the constraints must pin the output
  uniquely (check B). A generalization that cannot reproduce the vendor's own
  vectors is rejected before it can ever be trusted — the vendor verifies the
  generalization of the vendor.

The production lifter's real work, beyond what this prototype hand-waved, is items
1–4 of the residue: loop unrolling / symbolic execution of the bit accumulation,
the mod-3 tail emitters, reference-identity modeling of the pad guard, and
constructor data-flow to fix the alphabet. The alphabet-membership weak tier is
shippable today and already buys debunk-grade refutation (check C/D) of the
standard-vs-url-safe confusion across all inputs — the cross-library seam that the
point-wise vendor assertions alone could never reach.
