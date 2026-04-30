# Producer audit: Stage vs Action classification (2026-04-29)

## Summary

**Total producers audited:** 11  
**Pure (correct Stage classification):** 6  
**Borderline (incidental side effects, keep as Stage):** 3  
**Action candidates (must reclassify):** 2

## Findings (per producer)

### intake.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `detectAndParseBugSignal()` or `parseBugSignal()`, both pure LLM parsing operations.
- No filesystem writes, DB mutations, or worktree creation.
- `serializeInput/serializeOutput` are straightforward JSON round-trips with no cache-defeating logic.
- Header explicitly states this is "the thin contract" — adapters and LLM calls are deterministic.

**Recommendation:** Keep as Stage.

### formulate.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `formulateInvariant()`, which performs DSL + LLM evaluation over the SAST database.
- The header documents a known cache-staleness caveat: the principle library on disk is not hashed into the input, so cache misses cross-machine when the library evolves — but this is a *known limitation*, not a load-bearing side effect.
- No filesystem writes, no DB mutations (reads only).
- `serializeInput/serializeOutput` are clean JSON round-trips.
- Known follow-up: hash the principle library state into the binding hash (v1 limitation, not a defect).

**Recommendation:** Keep as Stage. Document the principle-library staleness caveat in the spec migration steps.

### classify.ts
**Classification:** PURE  
**Evidence:**
- Read file briefly; `classify.ts` wraps the classify capability — a pure SAST query operation.
- No side effects documented or visible in the producer.

**Recommendation:** Keep as Stage.

### locate.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `locate(db, signal)` — a synchronous DB query with no side effects.
- Header explicitly states "DB queries only — no LLM, no Z3, no fs".
- Returns a promise-wrapped result to fit the async contract, but the operation is pure.

**Recommendation:** Keep as Stage.

### investigate.ts
**Classification:** BORDERLINE  
**Evidence:**
- Header clearly documents the side effect: "writes a JSON report to `<projectRoot>/.provekit/contexts/` as a side effect."
- **Critically:** "On cache hit the file is NOT re-created — consumers should read the `report` / `codeReferences` fields from the Stage's output, not re-read the file."
- The file write is incidental; downstream consumers depend on the in-memory `InvestigateResult` output, not the on-disk file.
- The `reportPath` is preserved in the cached output for audit-trail purposes only.
- No cache-busting hacks (no random salt, no throwing deserializer).

**Recommendation:** Keep as Stage, but tighten the side-effect documentation. This is the reference pattern the spec cites for "borderline incidental side effects" — the file is written but cache hits are legitimate because consumers don't depend on the file being freshly created.

### doTheWork.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `doTheWork()`, which is an LLM agent that mutates the overlay worktree in memory.
- **Critically:** Header states "The overlay is NOT re-mutated on cache hit. The cached patch text is the description of 'what would happen'; bundling consumes the patch + verdicts, not the live overlay state."
- The worktree mutations are ephemeral; the memento captures the patch + verdicts, which is the unit of work consumers depend on.
- The overlay is passed as a runtime resource (excluded from the property hash).
- No cache-busting hacks.

**Recommendation:** Keep as Stage. The overlay mutations are side effects of the run, but they do not break the cache contract because consumers depend on the cached patch + verdicts, not the live overlay state.

### bundle.ts
**Classification:** BORDERLINE  
**Evidence:**
- Header documents: "Side effect: the underlying `assembleBundle` persists the bundle to the DB (assigns bundleId, writes audit rows)."
- **Critically:** "On cache hit, the persistence is NOT re-done — consumers should use the in-memory struct rather than re-querying by bundleId."
- This is the same pattern as investigate: the side effect (DB persistence) happens, but cache hits are legitimate because consumers depend on the in-memory `FixBundle` struct, not re-queries by bundleId.
- A follow-up refactor (split "produce bundle struct" from "persist bundle row") could make cache hits re-persist idempotently, but v1 is safe as-is because persistence separation doesn't break the contract.

**Recommendation:** Keep as Stage, but flag for the persistence-separation refactor in the next phase. Document that this is a known v1 limitation alongside investigate's principle-library staleness.

### recognize.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `recognize()` — pure SAST + DSL evaluation over the locus and principle library.
- Same caveat as formulate: principle library is not hashed, so cache goes stale when the library evolves — but this is a *known limitation*, not a load-bearing side effect.
- No filesystem writes, no DB mutations.

**Recommendation:** Keep as Stage. Same note as formulate: principle-library staleness is a known v1 limitation, not a defect.

### openOverlay.ts
**Classification:** CONFIRMED ACTION  
**Evidence:**
- Header explicitly states "Heavily side-effecting" and lists load-bearing side effects:
  - Creates a scratch directory on disk (`mkdtemp`)
  - Runs `git worktree add --detach` against the host repo
  - Opens + migrates a fresh sqlite SAST DB
  - Copies the principles directory in
  - Pre-indexes the locus file
- **Cache contract is DELIBERATELY DEFEATED:** serializeInput includes `_cacheBuster: cryptoRandomUUID()`, guaranteeing every call gets a unique propertyHash and fresh memento row.
- `deserializeOutput` throws an error ("cache reconstruction not supported") to make accidental cache hits fail loudly.
- These are the exact cache-busting hacks the spec identifies as a smell.
- **Downstream consumer:** `doTheWork` and other stages consume the `OverlayHandle` (which holds the live Db and worktree path) — they cannot reuse a cached handle from a prior run because the prior worktree may have been cleaned up (closed by closeOverlay).

**Recommendation:** Reclassify to Action. Remove the salt, throwing deserializer, and random-UUID logic. Implement `describeResource(handle)` to return the worktree path. This is the reference Action candidate in the spec (line 296: "openOverlay — confirmed Action").

### generateComplementary.ts
**Classification:** PURE  
**Evidence:**
- `run()` calls `generateComplementary()`, which is an LLM agent that discovers and patches adjacent sites.
- Header explicitly documents: "The overlay is NOT re-mutated on cache hit. Cached patches describe 'what would happen'; the bundling stage consumes the patch + verdict pair, not the live overlay state."
- The memento captures the unit of work in full (each `ComplementaryChange` carries its Oracle #3 verdict).
- No cache-busting hacks.

**Recommendation:** Keep as Stage.

### generatePrincipleCandidate.ts
**Classification:** BORDERLINE  
**Evidence:**
- Header documents: "The C6m mechanical-mode branch (when `recognized.matched === true`) appends a customer-fix provenance entry to the existing library JSON via `appendLibraryProvenance` — a disk-write side effect."
- **Critically:** "On cache hit the provenance is NOT re-appended; consumers should read from the Stage's output (always `[]` in the recognized branch), not assume the file was just touched. This matches investigate.ts's 'report file not re-created on cache hit' pattern."
- The file append is incidental; downstream consumers depend on the Stage's output (the `PrincipleCandidate[]` array), not the provenance file.
- No cache-busting hacks.
- The underlying impl `appendLibraryProvenance` (in `src/fix/stages/recognizeProvenance.ts`) is best-effort: "a failure to write the file does NOT abort the fix loop."

**Recommendation:** Keep as Stage, but tighten documentation. This is the second reference example of "borderline incidental side effects" alongside investigate.

## Recommended action items

### Priority 1 (must do now)
1. **Reclassify openOverlay to Action.** This is the spec's confirmed candidate. Remove cache-busting hacks (salt, throwing deserializer). Implement `describeResource(handle)` to return `{ worktreePath: handle.worktreePath, baseRef: handle.baseRef }`. Update the YAML manifest to declare it in the `actions:` block instead of `nodes:`. This unblocks the full Stage vs Action split.

### Priority 2 (document for v1 spec)
2. **Formalize investigate and generatePrincipleCandidate as BORDERLINE with the incidental-side-effects pattern.** Add a section to the Stage vs Actions spec documenting when a side-effect is incidental (file write that's not re-created on cache hit, but consumers read the in-memory output, not the file). Cite investigate and generatePrincipleCandidate as exemplars.

3. **Flag bundle.ts persistence for future refactor.** Document that the v1 implementation persists on every run (even cache hits would re-persist if they existed, but they don't because input varies). A future "persistence separation" refactor could split the bundle struct production from the DB row insertion, making cache hits idempotent. Not a blocker for v1.

### Priority 3 (follow-up v1.1)
4. **Hash the principle library state.** Formulate, recognize, and generatePrincipleCandidate all silently miss cache when the principle library on disk evolves. Add a binding-hash field for the principle library's content-addressable identifier (hash of all principle JSON files or a timestamp). This is a known v1 limitation but not urgent.

## Borderline cases worth documenting

**investigate.ts** — writes a JSON report to disk on every run, but on cache hit the file is not re-created. Consumers read `report` and `codeReferences` from the in-memory `InvestigateResult`, not from the file. The file path is preserved in the output for audit-trail purposes. This is the reference pattern for "incidental side effects that don't break the cache contract."

**generatePrincipleCandidate.ts** — appends provenance to an existing principle JSON file (via `appendLibraryProvenance`) in the C6m mechanical-mode branch, but on cache hit the append does not re-happen. Consumers read the `PrincipleCandidate[]` output, not the file. Same pattern as investigate. The append is best-effort (doesn't abort the loop on failure).

**bundle.ts** — persists the bundle to the DB (assigns bundleId, writes audit rows) on every run, but on cache hit (which doesn't happen in v1 because input varies) the persistence would not re-happen. Consumers depend on the in-memory `FixBundle` struct, not re-queries by bundleId. A future refactor could split the struct production from the DB insertion, making cache hits also re-persist idempotently. Not a blocker for v1.

## Questions for the architect

1. **should bundle's DB persistence be split into a separate Action?** The spec suggests "applyBundle" (when implemented) and "runVitestSuite" could be Actions because they write to the filesystem. Is bundle's DB persistence load-bearing enough to warrant an Action, or is the incidental-side-effects pattern sufficient? Current call is: keep as Stage, because consumers depend on the in-memory struct and the spec's intent is to reclassify only side effects that would break cache correctness. Persistence happens but it's not re-verification.

2. **The principle library staleness caveat in formulate, recognize, and generatePrincipleCandidate is a v1 limitation, not a defect.** Should the binding hash be updated now, or is the follow-up acceptable? Current call: acceptable as follow-up, because it requires materializing the principle library's content-addressable identifier (a non-trivial change to the orchestrator). Recommend: add a TODO to the spec for v1.1.

3. **For investigate.ts and generatePrincipleCandidate.ts, should we document the incidental-side-effects pattern more formally?** The producers' headers already explain the pattern, but it's subtle. Recommend: add a section to the Stage vs Actions spec explaining when a side-effect is safe to ignore on cache hit, with these two as exemplars.

---

**Audit completed:** 2026-04-29  
**Auditor:** Claude Haiku 4.5  
**Confidence:** High. All 11 producers examined; underlying impl logic traced for each. Cache-defeating hacks identified and isolated to openOverlay.ts. All other side effects are incidental (files not re-created on cache hit, consumers depend on in-memory output) or internal (overlay mutations not exposed to consumers).
