# Platform semantics captured via LossRecord at port boundaries

**Date:** 2026-05-16
**Status:** DRAFT for architect review. Substrate-level ruling. Captures Reading 2 unification.
**Authority:** T Savo (architect), via the 2026-05-16 deliberation that surfaced the trichotomy-already-handles-this insight.

## TL;DR

Algebra catalog operations are PURE STRUCTURE. Binary ops are 2-formal (`concept:add(lhs, rhs)`); no mode operands. Per-platform semantic behavior (overflow mode, integer-division rounding, shift mode) is declared at kit registration time. Behavioral divergence across cross-platform port is captured automatically via the substrate's existing `LossRecord` / `LossRecordMemento` family: the loudly-bounded-lossy leg of the substrate's first-principle trichotomy applied at the operation-semantic boundary.

No new memento family. No new trichotomy. The first-principle machinery already in place generalizes naturally because the question shape is identical: a transformation that is lossless within bounds, with characterizable loss outside bounds.

## The architectural insight

Earlier deliberation framed the question as "should concept:add carry a mode operand?" with two readings:
- Reading 1: mode in the algebra (3-formal `concept:add(lhs, rhs, mode)`), federation distinguishes behaviorally-different algebras at the bind-CID layer
- Reading 2: mode in the platform, federation is purely structural at the algebra layer, behavior captured elsewhere

The decisive observation that unified both readings: when source platform's `+` is Wrapping and target platform's `+` is Trap, that IS a loudly-bounded-lossy transformation. The substrate's existing trichotomy (exact / loudly-bounded-lossy / refuse) from `project_sugar_first_principle` already handles it. Concrete: lowering from i64 source to i32 target IS the loudly-bounded-lossy case the trichotomy was built for. The substrate had this architecture before; the mode-in-algebra question was asking for a NEW mechanism for something the existing mechanism handles natively.

Reading 2 + LossRecord unification:
- The colimit's universal property is preserved at the algebra layer (structural federation)
- Behavioral semantics live at the platform/kit-declaration layer (where behavior actually exists)
- Divergence across port = LossRecord at the operation-semantic boundary (existing artifact, new application)

### Rejected alternative for posterity

Reading 1 (mode-in-algebra) was a COHERENT alternative, not an obvious error. Its argument: if concept:add(a, b, Wrapping) and concept:add(a, b, Trap) are different algebras with different behaviors, their bind CIDs should differ; federation correctly distinguishes them; users get behavioral equivalence within-federation as a structural guarantee. The cost: every algebra op grows a mode operand, every lifter must map its language's semantic to canonical mode values for every op, every realize plugin renders every (op, mode) pair. Coverage grows from M (lifters per language) to M×K (lifter handles K modes per op). Paper 16's M+N hub claim collapses to M×N at the application layer.

Reading 2 wins because the substrate already has the architecture (LossRecord + the trichotomy) for capturing behavioral divergence at the right boundary. The mode-in-algebra approach would have required minting new mechanism for a question the existing mechanism handles. Architectural parsimony plus the M+N preservation made the call.

## The three substrate layers and what each captures

| Substrate fact | Captured in | Why |
|---|---|---|
| Operation identity | Algebra catalog (op_cid) | Structural, language-agnostic |
| Operation's composition shape | Algebra catalog (formals, sorts) | Structural |
| Platform's semantic for an op | Kit's `PlatformSemanticsDeclaration` at registration | Per-platform truth, content-addressed |
| Behavioral divergence on cross-platform port | LossRecord chained to bind claim's premises | Per-(source, target, op) tuple, signed |
| Loss characterization | LossRecord.loss_dimensions | Explicit named failure set, existing format |

## Concrete: i64-source → i32-target port under this architecture

- Source kit (i64-platform lifter) registered with `PlatformSemantics { concept:add → ArithmeticOverflowMode::Wrapping }`
- Target kit (i32-platform realizer) registered with `PlatformSemantics { concept:add → ArithmeticOverflowMode::Trap }`
- User invokes substrate's port operation: lift i64 source, emit i32 source
- Substrate emits: ported i32 source code + signed LossRecord characterizing "behavior diverges at inputs where lhs+rhs overflows i32; source wrapped; target traps at hardware level"
- User reads the loss record, decides: accept the divergence (port is good enough for inputs known to be bounded), refuse the port (semantics matter), or specify mitigation (add explicit overflow handling)

The substrate has not shipped silent breakage. Federation byte-identity at the algebra layer + explicit loss capture at the platform boundary + user decisional authority. All three legs of the first-principle trichotomy.

Note on tag distinctions (relevant to the example above): `Trap` means the hardware causes UB or a processor exception (e.g., ARM signed integer overflow on Trap-on-Overflow mode, x86 INTO instruction). `Checked` means the substrate refuses with a failure memento BEFORE invoking the operation (compile-time or substrate-layer check). They are distinct facts: Trap is a runtime hardware behavior; Checked is a substrate-layer policy. The substrate captures both via the same LossRecord mechanism but with different loss-dimensions encoded.

## What the algebra DOES NOT carry

- No mode operands on binary ops. `concept:add(lhs, rhs)` stays 2-formal.
- No surface-syntax fields (this was the γ ruling at the term_shape layer; the same principle applies upward).
- No platform-specific behavior tags inline in the algebra.

The algebra is the portable thing because operations and their compositions are language-agnostic shapes. Behavior at edge cases lives in implementations; implementations declare their semantics via kit registration.

## What the algebra DOES carry

- Operation identity (op_cid)
- Compositional structure (args)
- Concept-tier sorts (when polymorphism demands it; deferred per the γ ruling)

This is the canonical form locked in `docs/plans/2026-05-16-canonical-term-shape-form.md`. This ruling is consistent with that ruling.

## The PlatformSemanticTag sort family

Mint a `PlatformSemanticTag` sort with canonical values for the kinds of platform behavior that need explicit declaration:

- `ArithmeticOverflowMode`: Wrapping, Saturating, Checked, Trap, ArbitraryPrecision
- `IntegerDivisionRoundingMode`: Truncate (Rust), Floor (Python `//`), Euclidean
- `ShiftMode`: Arithmetic, Logical
- `FloatingPointMode`: IEEE754_R2008, IEEE754_R1985, Strict, Relaxed (deferred; floats are out of current scope)

Each value is a small content-addressed memento. Extensible: new platform behaviors mint new tags as needed; existing tags remain canonical.

## Kit registration extension

Extend `ConformanceDeclaration::Carrier` (or add a sibling `PlatformSemanticsDeclaration`) to carry per-target-op semantic tags:

```rust
struct PlatformSemanticsDeclaration {
    op_semantics: HashMap<Cid, PlatformSemanticTag>,
}
```

Each kit registers once with its full per-op semantic table. Substrate has the per-platform truth as content-addressed declarations.

## execute_path / port pipeline extension

When the substrate composes a source kit and a target kit:
1. Walk each concept op in the chain
2. Look up source kit's declared semantic for the op
3. Look up target kit's declared semantic for the op
4. If they differ: compute a LossRecord characterizing the divergence (existing `sugar-ir-types::LossRecord` machinery)
5. Chain the LossRecord to the bind claim's premises as a signed memento
6. If the divergence is uncharacterizable (no LossRecord shape captures it): refuse the port (refuse leg of the trichotomy)

CI gate: cross-platform composition test asserts that when source's semantic ≠ target's semantic for an op in the path, a LossRecord-bearing memento is emitted. Refuse silent emission.

## What the substrate claims under this architecture

Federation byte-identity at the algebra layer: same algebra produces same bind CID across any source platform. Empirically validated 2026-05-16 by `seam4_federation_rust_vs_python_lift_bind_byte_identity` for `add(x, y) = x + y`.

Behavioral divergence at cross-platform port: explicitly captured in signed LossRecord mementos that flow with every port event. The substrate ships HONEST federation with explicit-delta-capture; it does not ship silent platform-behavior loss.

Refusal on uncharacterizable loss: preserved as the third leg of the trichotomy. If a divergence shape isn't expressible as a LossRecord, the port refuses.

## What this means for A18 (and prior audit findings)

The 2026-05-16 post-merge audit flagged A18 (concept:div Python `//` floor-toward-negative-infinity vs Rust `i64 /` truncate-toward-zero) as an URGENT BLOCKING Supra-omnia-rectum violation. Under this ruling, A18 reclassifies:

- The substrate is STRUCTURALLY correct: same algebra, same CID
- The current gap is: kit registrations don't yet carry platform semantics, so cross-platform divergence isn't being detected, so LossRecord isn't being emitted, so the substrate isn't being EXPLICIT about the divergence
- Fix is implementation (PlatformSemanticsDeclaration machinery), not architectural redesign

A18 is no longer a correctness violation. It is the work needed to operationalize this ruling.

Audit findings A19, A20, A21, A22 are unaffected by this ruling; they remain as filed and dispatched.

## Implementation work surface

1. Mint `PlatformSemanticTag` sort + canonical values. Small, mintable, extensible. Architectural-judgment-required for the initial value set.
2. Extend kit registration with `PlatformSemanticsDeclaration`. Either as an extension to `ConformanceDeclaration::Carrier` or as a sibling declaration filed at registration. Build-on-existing-kits clause applies.
3. Update each kit on main to declare its platform semantics. Rust kit (i64 platform), Python kit (int / arbitrary-precision), Java kit (int / long), C kit (int / long / size_t per ABI). Per-kit, per-op, declared once at registration.
4. Update `execute_path` (or a pre-emit step) to walk source-vs-target semantics per concept op in the path; emit `LossRecord`-bearing memento for each divergence; chain to the bind claim's premises.
5. CI gate: cross-platform composition test asserts that when source's semantic ≠ target's semantic for an op in the path, a `LossRecord`-bearing memento is emitted; refuse silent emission.

The work is bounded. Each step is mechanical-once-architected. The substrate stays structurally pure at the algebra layer; behavior is captured at the kit-declaration layer; divergence is captured at the per-port-event memento layer. Each fact lives where it belongs.

## Cleanup follow-up

The current catalog may contain a forward-looking 3-formal `concept:add` (and similar) that was an aspirational shape under Reading 1. Decide: revise back to 2-formal (cleanest, removes the aspirational shape) or retain as deferred-future (leaves a known catalog-vs-lifter inconsistency on main). Lean toward revise-back-to-2-formal under this ruling.

## Cross-references

- `docs/plans/2026-05-16-canonical-term-shape-form.md`: the γ ruling at the term_shape layer; consistent with this ruling at the platform-semantics layer
- `docs/papers/16-after-portability-the-universal-address-space.md`: paper 16's colimit argument; this ruling preserves it
- `project_sugar_first_principle` (agent memory): Supra omnia, rectum + the trichotomy (exact / loudly-bounded-lossy / refuse); this ruling applies the trichotomy at a new boundary
- Post-merge audit `docs/plans/2026-05-16-gamma-postmerge-audit.md`: A18-A22 findings; A18 reclassifies under this ruling

## Trinity claim under this ruling

The algebra is the portable thing. Structural federation holds at the algebra layer empirically. Behavioral divergence across platforms is explicitly captured in signed LossRecord mementos that flow with every cross-platform port. The substrate ships honest federation with explicit-delta-capture and refusal-on-uncharacterizable-loss. All three legs of the substrate's first-principle trichotomy operate at the operation-semantic boundary the same way they operate everywhere else.
