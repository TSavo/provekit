# Trinity Round-Trip Transport Gaps: Java Leg

**Date:** 2026-05-12
**Status:** empirical finding (v1, post Wave-C PR #748)
**Branches:** feat/trinity-java-roundtrip (v0), feat/wave-c-bind-real-body-opus (v1)
**Companion spec:** 2026-05-14-transport-gap-and-partial-morphism-protocol.md
**Companion tests:**
- `implementations/rust/provekit-cli/tests/wave_c_real_body_emission.rs` (axis 1 + 2 verification)
- `implementations/rust/provekit-cli/tests/trinity_roundtrip_test.rs` (full chain)
- `implementations/java/provekit-lift-java-source/src/test/java/com/provekit/lift/java_source/TrinityRoundtripLiftTest.java` (v0 snapshot, intentionally pinned to pre-wave-c output to preserve regression-detection)

## Summary

The trinity round-trip fixture (11 catalog concepts + retry-loop) was run
through `provekit bind --rewrite=canonical --target-language=java` and the
per-concept emission verdict measured against the three convergence axes
declared in the plugin-protocol spec §15.

### v0 result (pre Wave-C)

**0 / 11 concepts emit real bodies; 11 round-trip REFUSE.**

### v1 result (post Wave-C, this PR)

**12 / 12 bindings emit real Java bodies (axis 1 + 2 closed).**
**Round-trip axis 3 (Java→IR concept-CID match) remains loudly-bounded-lossy** —
the Java re-lifter is not exercised in this PR per scope.

Per the trichotomy (Supra omnia, rectum):

- **EXACT** (all 3 axes verified): 0 / 11
- **Loudly-bounded-lossy** (axes 1 + 2 verified, axis 3 deferred):
  10 / 11 — all but `unit` — with the named loss-dim
  `axis-3-concept-cid-verification-deferred`
- **REFUSE**: 1 / 11 — `unit` (`do_nothing`), Gap 4 below

The loudly-bounded-lossy verdict here is the legitimate first-class outcome
per transport-gap spec §0.1: the domain of agreement is provably non-empty
(emitted Java compiles, structurally encodes the original Rust logic, carries
the correct concept annotation), and the failure region (concept-CID mismatch
not yet verified end-to-end through the Java lifter) is precisely characterized
and named.

## How the bind was run (v1, this PR)

```
cargo run -p provekit-cli --quiet -- bind \
  --root implementations/rust/provekit-cli/tests/fixtures/trinity_roundtrip \
  --lang rust \
  --target-language java \
  --rewrite canonical \
  --mode monitor \
  --output /tmp/java-out-trinity
```

Verdict counts on `index.json` (12 bindings):
`exact=7  loudly_bounded_lossy=1  refuse=4`

These bind-side verdicts reflect the Rust-lift-to-concept transport, not the
Java leg. They are stable across v0 and v1 (Wave-C does not change the
abstraction-side discharge).

## Gap 1: `bind-stub-body-emitted` — **CLOSED**

**Resolution:** v1 lifts a real `libprovekit::core::Term` graph from each
function body via `provekit-cli/src/syn_to_term.rs` and threads it through
`cmd_transport::realize_for_bind`. The realizer now receives a real body
and emits idiomatic per-target statements (return/if/while/for/throw) instead
of the language-stub fallback.

**Falsifiable assertion** (locked by
`tests/wave_c_real_body_emission.rs::wave_c_emits_real_bodies_for_trinity_concepts`):
the emitted `lib.java` contains no `UnsupportedOperationException` marker
and no `provekit-bind canonical:` stub message — for any of the 12 classes.

**Gap-record consequence:** `gaps.json` no longer carries
`bind-stub-body-emitted` when every binding fits the wave-c lift slice; it
records `bind-real-body-emitted` instead with the list of realized functions.
When a binding's body is outside the wave-c slice, `bind-stub-body-emitted`
is emitted with the precise list of fall-back functions (loudly-bounded-lossy
record, not silent regression).

## Gap 2: `bind-invalid-java-param-type` — **CLOSED on bind side**

**v0 issue:** bind emitted `&i64 items` as a Java parameter type for Rust
`&[i64]` slices (not valid Java).

**v1 resolution:** `type_to_str` (in `cmd_transport.rs`) recognises
`syn::Type::Reference<Slice>` and emits a `[T]` token; `map_source_type`
maps `[T]` to the idiomatic per-target container shape — Java `long[]`,
TS `number[]`, Go `[]int64`, etc. Same path handles `Type::Tuple` for
`(i64, i64)` — `concept:pair` now emits `long[] { b, a }` in Java rather
than the broken `&i64 items` token.

**Status on the lift side (Java):** untouched by this PR per scope; the
`TrinityRoundtripLiftTest` snapshot at
`implementations/java/provekit-lift-java-source/src/test/resources/trinity-roundtrip/lib.java`
still encodes the v0 output. Refreshing it would require co-updating
`TrinityRoundtripLiftTest.java`, which the Java testsuite asserts the v0
verdict against. Follow-up work.

## Gap 3: `bind-concept-misclassification` — **CLOSED**

**v0 issue:** when `wrap_identity`, `toggle`, `swap_pair` collapsed to the
same `shape_cid` (`Body([Opaque])`), the bucket creation pass added all
three concept buckets, but the binding-realization pass looked up
`concept_idx` solely via `shape_to_concept`, where the last bucket inserted
under that shape (`pair`) silently overwrote `identity` and `bool-cell`.
Result: `wrap_identity` lifted as `concept:pair` and `toggle` lifted as
`concept:pair`.

**v1 resolution:** the binding-realization pass now recomputes the same
priority-keyed bucket key (`human:NAME` > `catalog:ID` > `shape:CID`) and
looks up `concept_idx` via `key_to_concept_idx` first, falling back to
`shape_to_concept` only for keys that were never created. `// concept:`
annotations take precedence over shape-collision.

**Falsifiable assertion** (locked by `tests/wave_c_real_body_emission.rs`):

```java
final class WrapIdentityTransported {
    // concept: identity
    public static long wrap_identity(long x) { return x; }
}
final class ToggleTransported {
    // concept: bool-cell
    public static boolean toggle(boolean flag) { return !flag; }
}
```

## Gap 4: `lift-void-return-refused` — **OPEN (out of scope per PR #748)**

**v0 issue:** the Java lifter v1 slice refuses void-returning methods; the
emitted `DoNothingTransported.do_nothing()` lifts to a `Refusal` rather
than a function-contract declaration.

**Status:** the Java lifter is intentionally untouched by this PR (prompt
explicit). The bind side now correctly emits

```java
public static void do_nothing() { return; }
```

(real body, no stub marker), but the round-trip remains REFUSE on the
lift-side until the Java lifter v1 slice extends to void-returning methods.

**Resolution path:** extend `JavaSourceLifter` to handle `VoidType` return,
or introduce a `unit` wrapper return type on the bind side. Out of scope
here.

## Per-concept verdict table (v1, this PR)

Three columns per row: BIND-EMISSION (what bind writes), LIFT-AXIS-1+2
(parse / structure verified by `wave_c_real_body_emission.rs`), and
ROUND-TRIP (concept-CID axis 3 status).

| Concept      | Bind emits          | Axis 1+2 (this PR)        | Axis 3 (round-trip)                                  |
|--------------|---------------------|---------------------------|-------------------------------------------------------|
| identity     | `return x;`         | VERIFIED                  | LOUDLY-BOUNDED-LOSSY (axis-3-concept-cid-verification-deferred) |
| unit         | `return;`           | VERIFIED                  | REFUSE (Gap 4: void lift open) |
| bool-cell    | `return !flag;`     | VERIFIED                  | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| assert       | `if (x <= 0L) throw…` | VERIFIED                | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| option       | `if (items.length == 0) return -1L; else return items[(int) 0L];` | VERIFIED | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| option-bind  | nested if/return    | VERIFIED                  | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| result       | `if (denom == 0L) return -1L; else return num / denom;` | VERIFIED | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| result-bind  | nested if/return    | VERIFIED                  | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| pair         | `return new long[] { b, a };` | VERIFIED        | LOUDLY-BOUNDED-LOSSY (axis-3 deferred; tuple-as-array structural loss recorded) |
| list         | `for (var v : items) acc = acc + v; return acc;` | VERIFIED | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| tagged-union | nested if/return    | VERIFIED                  | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |
| retry-loop   | `while (attempt < max_attempts) { attempt = attempt + 1; if (attempt >= 1L) return true; } return false;` | VERIFIED | LOUDLY-BOUNDED-LOSSY (axis-3 deferred) |

(retry-loop is the 12th binding, not one of the 11 trinity concepts.)

**EXACT (all 3 axes empirically verified): 0 / 11** — axis 3 not exercised
in this PR (Java lifter untouched per scope).

**LOUDLY-BOUNDED-LOSSY (axes 1+2 verified, axis 3 deferred): 10 / 11.**

**REFUSE (axes 1+2 verified, axis 3 known-blocked): 1 / 11** — `unit`,
Gap 4 open.

### Why loudly-bounded-lossy and not REFUSE for the 10

Per transport-gap spec §0.1, LOUDLY-BOUNDED-LOSSY is the legitimate verdict
when the transformation agrees with the source on a precisely characterized
non-empty domain. In v1 each of these concepts:

1. **Axis 1:** emitted Java parses (`javac` clean for the whole `lib.java`,
   verified by `wave_c_emitted_java_compiles_when_javac_available`).
2. **Axis 2:** the emitted class name, parameter names + types, and
   `// concept:` annotation match the Rust-side intent (locked by
   `wave_c_emits_real_bodies_for_trinity_concepts`).
3. **Axis 3:** unverified — the Java re-lifter would need to compute the
   concept-CID from the emitted Java and compare to the Rust-side concept-CID.
   This PR does not run the Java lifter (out of scope per prompt).

The named loss-dim is `axis-3-concept-cid-verification-deferred`. The work
to close it is straightforward (run `provekit-lift-java-source` against
`lib.java`, compute concept-CID, assert equality) but requires touching the
Java testsuite which the prompt forbids.

### Why REFUSE for `unit`

`do_nothing` emits a syntactically valid Java void-returning method, but
the Java lifter v1 slice refuses void returns (Gap 4). Round-trip cannot
recover the original IR because the lifter never produces a function-contract
declaration for it.

## Open follow-ups

1. **Gap 4 (void lift):** extend `JavaSourceLifter` to handle void-returning
   methods. Closes round-trip for `unit`.
2. **Axis-3 verification:** run `JavaSourceLifter` over wave-c-emitted
   `lib.java`, compute concept-CID per class, assert equality with Rust-side
   concept-CID. Closes 10 / 11 concepts to EXACT.
3. **`TrinityRoundtripLiftTest.java` resource refresh:** the snapshot at
   `implementations/java/provekit-lift-java-source/src/test/resources/trinity-roundtrip/lib.java`
   is the v0 stub output. Co-update the test to the v1 expectations once
   axis-3 verification lands.

## What this PR closes

- Gap 1 (`bind-stub-body-emitted`): **CLOSED** for 12 / 12 trinity-fixture
  bindings.
- Gap 2 (`bind-invalid-java-param-type`): **CLOSED** on the bind side
  (Java side untouched; lift-time `<any>` erasure no longer triggered
  because parameter types are now valid Java).
- Gap 3 (`bind-concept-misclassification`): **CLOSED** for `identity`,
  `bool-cell`, and any future concept-shape collisions.
- Gap 4 (`lift-void-return-refused`): **STILL OPEN** (out of scope per
  PR #748).
