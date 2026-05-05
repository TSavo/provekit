# Baseline catalog rubric

What does a per-language baseline catalog need to contain before it ships at v1.0.0? This rubric gives a precise answer.

## The principle

**A baseline catalog captures the hidden predicates of a language's standard library at a level real programs can build on.** Not the most fundamental predicates. Not the most documented. The ones that programs in that language _actually call_, with the contracts those programs _actually rely on_, encoded explicitly so a verifier can reason about them.

A foundation-baseline catalog is not authoritative. It's the worked example showing the substrate carries weight. The authoritative signer for a language's contracts is the language steward (the people who wrote the language). The baseline exists so users have something to verify against until the steward signs.

## What ships at v1.0.0

For each of the 12 supported languages (rust, go, cpp, ts, csharp, swift, java, python, ruby, zig, c, php) the foundation signs one baseline catalog. The catalog's metadata explicitly says:

- _signed by_: foundation v0 ed25519 key
- _signer role_: `foundation-baseline`
- _authoritative_: false
- _disclaimer_: "starting point; for authoritative contracts ask the language steward"

A consumer who pins this catalog gets the foundation's claim that these contracts hold for the named language version. They do not get a guarantee from the language vendor. They can layer the language vendor's signature on top when it's available, or fork the catalog and sign their own.

## The four floor decisions

### 1. Builtin coverage floor

**Target: ~50 builtins per language.** Exception: PHP at ~100 (the language has 5000+ builtins; the 80% case is wider).

**"Top builtin" means most-called in real programs**, not most documented or most fundamental. Sources of truth, ranked:

1. **The kit's existing test suite**: what does the lifter already exercise? If a builtin appears in the kit's test fixtures, it's already shown up in real code somewhere.
2. **GitHub corpus frequency**: sample a few hundred public repos in the language, grep for builtin calls, take the top 50 by call frequency.
3. **Language steward's "essentials" documentation**: when available (e.g., MDN's Web APIs, Python's `dir(builtins)`, Rust's `std` prelude). Treat as a hint, not a list to copy verbatim.

Do not target "comprehensive coverage of the standard library." That's a different artifact and it's not what ships at v1.0.0.

### 2. Predicate density per builtin

**Floor: 2 predicates per builtin.** At minimum:

- **Type signature.** Input types, output type. Encoded via `forall` + `eq(ctor("type_of", ...), strConst("..."))`.
- **Determinism.** Same input, same output. Encoded via `forall` + `eq(ctor("f", x), ctor("f", x))`. (Some builtins are non-deterministic: `time()`, `random()`, file IO. Those get an explicit `non_deterministic` predicate instead.)

**Aspiration: 4-5 predicates** when the builtin has obvious additional structure:

- **Length / size bounds**: `len(s) >= 0`, `len(strlen(s)) == strlen(s) + 1` for null-terminated, etc.
- **Totality vs. partiality**: does it always return, or can it throw / return null on bad input?
- **Self-identifying prefix**: outputs that carry their own type tag (e.g., `blake3-512:...`, `ed25519:...`).
- **Side effects**: file IO, mutation of arguments, global state.
- **Edge cases**: behavior on empty input, on the largest representable value, on null.

**Out of scope for the baseline**: predicates the current formula DSL can't express (see #256 DSL extension survey). Ship without them; the language steward's signature can add them later.

### 3. Advisory metadata shape

Every baseline catalog carries advisory metadata at the **envelope level** (not per-contract; all contracts in a baseline share advisory status). The metadata is part of the catalog's signed bytes.

```json
{
  "signer": "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
  "signer_role": "foundation-baseline",
  "baseline": {
    "version": 1,
    "language": "rust",
    "language_version": "1.81.0",
    "kit_version": "0.1.0",
    "disclaimer_cid": "blake3-512:..."
  }
}
```

Field semantics:

- `signer_role`: one of `foundation-baseline`, `language-steward`, `community`. The verifier exposes this so consumers can apply trust policy by role.
- `baseline.version`: schema version of the baseline metadata block. Starts at 1.
- `baseline.language`: the language identifier (matches the kit alias: `rust`, `go`, `cpp`, ...).
- `baseline.language_version`: the language version this baseline was authored against. When the language ships a new release, a new baseline is minted; old baselines remain pinnable by CID.
- `baseline.kit_version`: the kit's version at authoring time. If the kit changes how it lifts, a new baseline gets a new kit_version even if the language hasn't moved.
- `baseline.disclaimer_cid`: content-CID of the disclaimer text. The disclaimer text travels with the catalog as a member; this field pins its content.

### 4. Disclaimer text

Verbatim base text + per-language addendum. The base ensures consumers learn one disclaimer; the addendum names the absent steward so the gap is visible.

**Base (verbatim across catalogs):**

```
Foundation baseline catalog: advisory only.

This catalog asserts hidden predicates about the named language's
standard library. It is signed by the ProvekIt foundation key as a
starting point for users who want to verify proofs about code in
this language.

It is NOT authoritative.

The authoritative signer for this language's contracts is the
language steward (named below). If they sign their own catalog,
prefer it over this one. If they have not, fork this catalog and
sign your own; see docs/contributing/signing-your-own-catalog.md.
```

**Per-language addendum:**

```
Language: <language>
Steward: <e.g., the rust-lang team / Microsoft / Python core developers /
          PHP RFC group / etc.>
Steward signature available: <yes / no / partial>
Authored against: <language version>
```

If the steward has signed their own catalog, the addendum points at it. If not, the addendum says "no authoritative signer for this version."

The disclaimer ships as a `members` entry in the proof envelope, content-addressed via `disclaimer_cid`. Modifying the disclaimer changes the CID, which forces a new envelope, which forces a new signature. The disclaimer is a first-class part of the signed artifact, not metadata that can drift.

## Compliance checklist

Any agent's baseline catalog output is checked against this list. The kit-level CI gate runs these as automated assertions where possible.

- [ ] Builtin count ≥ 50 (≥ 100 for PHP)
- [ ] Each builtin has ≥ 2 ContractDecls (type signature + determinism / non_deterministic)
- [ ] Envelope metadata has `signer_role: "foundation-baseline"`
- [ ] Envelope metadata has `baseline.version`, `baseline.language`, `baseline.language_version`, `baseline.kit_version`, `baseline.disclaimer_cid`
- [ ] Disclaimer text in catalog matches base template verbatim
- [ ] Per-language addendum names the steward
- [ ] Companion doc at `docs/baselines/<lang>.md` exists with disclaimer + change log
- [ ] All ContractDecls verify under `provekit verify --baseline=<lang>`
- [ ] Catalog signs with foundation v0 ed25519 key
- [ ] Catalog filename matches `baselines/<lang>-baseline-v<N>.proof` convention
- [ ] Companion `<lang>-baseline-v<N>.proof.json` (the public attestation) lists the catalog CID + signer + roles

## Authoring workflow

For each language:

1. Mine top-N builtins from the three sources above.
2. For each builtin, author 2-5 ContractDecls in the kit's slab (mirroring the per-kit Side A pattern).
3. Run through the kit's lifter to mint the catalog.
4. Verify the metadata block matches the schema in section 3.
5. Verify the disclaimer text matches the template in section 4.
6. Sign with foundation v0.
7. Commit the catalog at `.provekit/baselines/<lang>-baseline-v1.proof`.
8. Write `docs/baselines/<lang>.md` with the disclaimer + change log.
9. Run `provekit verify --baseline=<lang>` to confirm everything verifies.
10. Open PR. CI gate runs the compliance checklist as assertions.

The rust pilot (#257) walks this workflow end-to-end on the lowest-risk kit, validates the rubric, and feeds any rough edges back to this doc.

## Versioning

Baselines are versioned per-language, not globally. `<lang>-baseline-v1` and `<lang>-baseline-v2` can coexist; consumers pin whichever version they want. A new minor release of a language doesn't force a new baseline version unless the kit's authoring changes; minor releases that add builtins without changing existing semantics extend an existing baseline (members appended, contractSetCid changes, signed envelope re-emitted as `<lang>-baseline-v1.<minor>.proof` if needed).

A new major language version (Python 4, Java 22, etc.) starts a new baseline major version. Old baselines remain pinnable by CID indefinitely.

## What this rubric is NOT

- It is not a quality gate for the steward's eventual signature. The steward can sign at whatever density they want; the foundation baseline is the floor.
- It is not a target. Authoring more than 50 builtins is fine; this is the ship-it floor.
- It is not authoritative on what predicates the language _has_. It's the floor of what we encode and verify.

## Open questions

- **Builtin selection automation.** The "GitHub corpus frequency" approach should be scriptable. Until automated, agents pick top-N by inspection. Filed as a follow-up.
- **Steward outreach.** The protocol's value comes from stewards eventually signing. The launch ships with foundation-only signatures and a documented federation mechanism; stewards signing is post-launch growth, not v1.0.0 scope.

## See also

- #253 launch v1.0.0 epic
- #255 federation mechanism (how stewards sign their own)
- #256 DSL extension survey (which predicates can/can't be expressed today)
- #257 rust pilot (first-mover validation of this rubric)
- `docs/contributing/adapter-coverage-rubric.md` (sister rubric for lift adapters)
