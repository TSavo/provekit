# Smoke Test: End-to-End Vision Demo

The eight-verb pipeline (Lift, Cluster, Name, Scope, Cluster, Identify, Realize, Witness) walking around a 200-line Rust fixture. Every contract in the rewritten output traces to one of three substrate sources: a lifted annotation, a lifted test assertion, or a wp_rule applied to a recognized term-shape. None of the contracts shown below were authored by the smoke-test driver author.

Fixture root: `/Users/tsavo/provekit-worktrees/pk-695-unstub/menagerie/smoke-test-e2e`

## 1. The fixture

Three idioms are deliberately mixed so the smoke test can prove each substrate-source pathway separately:

- `src/option_handling.rs` carries `#[requires]` / `#[ensures]` annotations on every fn. These exercise the ANNOTATION-LIFT path.
- `src/validate_then_commit.rs` carries no contract annotations on either fn. A unit test in `tests/properties.rs` asserts a post-condition over `commit_balance_change` only. This exercises the TEST-LIFT path and (combined with the clustering result) the PROPAGATION path onto `commit_inventory_change`.
- `src/retry_with_backoff.rs` carries neither annotations nor test assertions. Both functions have a structural shape the term algebra recognizes as `retry-with-bounded-attempts`. This exercises the ALGEBRA-SYNTHESIS path AND the COMPRESSION event (two visibly different functions sharing one concept-CID).

The fixture itself is plain Rust 2021. Attributes are kept inert under `rustc` via `#[cfg_attr(any(), requires(...))]` so `cargo build` and `cargo test` succeed unchanged while `syn::parse_file` still observes the attribute literals.

## 2. The lift

Pass 1 lifted 9 sites across 4 files. For each site, the driver constructed a canonical term shape, minted a `contract` memento (via `provekit-claim-envelope::mint_contract`) when a contract was recoverable, and assigned a stable BLAKE3-512 shape-CID for clustering.

| Site | Shape-CID (prefix) | Mint Envelope CID (prefix) | Contract content-CID (prefix) |
|---|---|---|---|
| `src/clamps.rs::clamp_score` | `blake3-512:8949291d51ae4f73…` | `(empty)` | `(empty)` |
| `src/clamps.rs::clamp_pressure` | `blake3-512:8949291d51ae4f73…` | `(empty)` | `(empty)` |
| `src/option_handling.rs::first_or_default` | `blake3-512:4899179abf6709d6…` | `blake3-512:e84f8b1866740718…` | `blake3-512:482c45397f0ed2f1…` |
| `src/option_handling.rs::safe_index` | `blake3-512:f103ab16bf168497…` | `blake3-512:e3a27191078d666a…` | `blake3-512:9de5f093cfe3f6e1…` |
| `src/retry_with_backoff.rs::try_send_v1` | `blake3-512:4b48c5b59fab37a3…` | `blake3-512:88c92b7c05ab0997…` | `blake3-512:a1b4c58ef06c6ef2…` |
| `src/retry_with_backoff.rs::try_send_v2` | `blake3-512:987bda3963eeab3b…` | `blake3-512:bc6ceef4548ebbd9…` | `blake3-512:18ca4f604d7a6901…` |
| `src/retry_with_backoff.rs::attempt_succeeds` | `blake3-512:9bb145c78148efe7…` | `(empty)` | `(empty)` |
| `src/validate_then_commit.rs::commit_balance_change` | `blake3-512:520961a119b28bf8…` | `blake3-512:4d94e0456ff4bb90…` | `blake3-512:64ee7465bd7a2678…` |
| `src/validate_then_commit.rs::commit_inventory_change` | `blake3-512:520961a119b28bf8…` | `blake3-512:9e270257e1258a3a…` | `blake3-512:244b669c64291946…` |

## 3. The clustering

Shape-CIDs were grouped via exact equality (the algebra's address). Each cluster received a name via the priority order: human annotation (pass 2 only) -> seed catalog -> `UNNAMED-CONCEPT-N`.

| Concept | Shape-CID (prefix) | Source of Name | # sites | Catalog id |
|---|---|---|---|---|
| `UNNAMED-CONCEPT-1` | `blake3-512:8949291d51ae4f73…` | auto (UNNAMED-CONCEPT-N) | 2 | (none) |
| `UNNAMED-CONCEPT-2` | `blake3-512:4899179abf6709d6…` | auto (UNNAMED-CONCEPT-N) | 1 | (none) |
| `UNNAMED-CONCEPT-3` | `blake3-512:f103ab16bf168497…` | auto (UNNAMED-CONCEPT-N) | 1 | (none) |
| `concept:retry-with-bounded-attempts` | `blake3-512:4b48c5b59fab37a3…` | seed catalog | 2 | shape:retry-with-bounded-attempts |
| `UNNAMED-CONCEPT-4` | `blake3-512:9bb145c78148efe7…` | auto (UNNAMED-CONCEPT-N) | 1 | (none) |
| `concept:guard-then-commit` | `blake3-512:520961a119b28bf8…` | seed catalog | 2 | shape:guard-then-commit |

4 unnamed cluster(s) surfaced in pass 1. Each carries a stable shape-CID. The naming round-trip in section 10 closes one of them.

## 4. The bindings

Each binding is a `(code-site -> concept -> discharge verdict)` triple. The discharge verdict is computed by a structural oracle that respects the substrate's loss model:

- `exact` if both shape and formula are losslessly transported.
- `loudly-bounded-lossy(...)` when the shape transports exactly but the formula uses the smoke-test's single-atom encoding (see section 8).
- `refuse(...)` if no contract was recovered.

| Site | Concept | Origin | Discharge Verdict |
|---|---|---|---|
| `src/clamps.rs::clamp_score` | `UNNAMED-CONCEPT-1` | `empty` | `refuse(no contract recovered)` |
| `src/clamps.rs::clamp_pressure` | `UNNAMED-CONCEPT-1` | `empty` | `refuse(no contract recovered)` |
| `src/option_handling.rs::first_or_default` | `UNNAMED-CONCEPT-2` | `annotation-lift` | `loudly-bounded-lossy(formula-string-transport (smoke-test single-atom encoding))` |
| `src/option_handling.rs::safe_index` | `UNNAMED-CONCEPT-3` | `annotation-lift` | `loudly-bounded-lossy(formula-string-transport (smoke-test single-atom encoding))` |
| `src/retry_with_backoff.rs::try_send_v1` | `concept:retry-with-bounded-attempts` | `algebra-synthesis[wp_rule.retry-with-bounded-attempts.v0]` | `exact` |
| `src/retry_with_backoff.rs::try_send_v2` | `concept:retry-with-bounded-attempts` | `algebra-synthesis[wp_rule.retry-with-bounded-attempts.v0]` | `exact` |
| `src/retry_with_backoff.rs::attempt_succeeds` | `UNNAMED-CONCEPT-4` | `empty` | `refuse(no contract recovered)` |
| `src/validate_then_commit.rs::commit_balance_change` | `concept:guard-then-commit` | `test-lift` | `loudly-bounded-lossy(formula-string-transport (smoke-test single-atom encoding))` |
| `src/validate_then_commit.rs::commit_inventory_change` | `concept:guard-then-commit` | `algebra-synthesis[wp_rule.guard-then-commit.v0]` | `exact` |

## 5. The propagation event

One unit-test assertion lifted into the substrate as a `WitnessMemento` and attached to a concept (not a single site). Every other binding to the same concept-CID inherits the witness obligation. Before-and-after shown explicitly below.

### Witness event

- Source: `properties.rs:assert`
- Concept shape-CID: `blake3-512:520961a119b28bf8…`
- Formula (substrate-pretty): `out >= 0`

**Before the witness fires**: the sites bound to this concept have no inherited obligation. The driver's per-site contract origins are recorded individually.

| Site | Pre-witness Origin | Pre-witness Verdict |
|---|---|---|
| `src/validate_then_commit.rs::commit_balance_change` | `test-lift` | `loudly-bounded-lossy(formula-string-transport (smoke-test single-atom encoding))` |
| `src/validate_then_commit.rs::commit_inventory_change` | `algebra-synthesis[wp_rule.guard-then-commit.v0]` | `exact` |

**After the witness fires**: the concept-CID carries the obligation. Every binding to that shape-CID inherits a `#[cfg_attr(any(), witness(...))]` marker in the rewritten output. The propagated-to set is exactly the concept's site list.

| Site | Inherited Witness |
|---|---|
| `src/validate_then_commit.rs::commit_balance_change` | `out >= 0` |
| `src/validate_then_commit.rs::commit_inventory_change` | `out >= 0` |

The propagation is the magic: one assertion against one function attaches a property to N sites by going through the concept abstraction. The witness travels with the algebra, not with the code-site.

## 6. The compression event

When the algebra's address is the cluster key, syntactic variants of the same idea fall into one bucket. The fixture has two such clusters; both compress automatically.

### Cluster: `UNNAMED-CONCEPT-1`

- Primary shape-CID: `blake3-512:8949291d51ae4f7309ee5f546bb121a7c4bde9a3683a66341e38646ba502d6f9d46ad29e66b46c1e3aae58dcc4abe46a20695c9afb7baf18b9496d2d8b901860`
- Sites:
  - `src/clamps.rs::clamp_score` (line 17) realizes `UNNAMED-CONCEPT-1` (primary shape)
  - `src/clamps.rs::clamp_pressure` (line 27) realizes `UNNAMED-CONCEPT-1` (primary shape)

Every site in this cluster has the same canonical shape-CID. The compression is byte-identical at the algebra level.

### Cluster: `concept:retry-with-bounded-attempts`

- Primary shape-CID: `blake3-512:4b48c5b59fab37a3c25ef0ca2f1e8ffc83826059b2bdac97ae8878a02fe40683fbdc25337d6ced61e28b62715aa8a663915a6f07291b2cee7d20d0b3ac48755d`
- Alias shape-CIDs (compressed into the same cluster):
  - `blake3-512:987bda3963eeab3b379029c7d3063e0c1cb5e1d8f7d010c1a08825f5a5398c236412469e5e85591b33897bb6f5218aa26c35172369f6a2fc303868db05e8ef95`
- Sites:
  - `src/retry_with_backoff.rs::try_send_v1` (line 21) realizes `concept:retry-with-bounded-attempts` (primary shape)
  - `src/retry_with_backoff.rs::try_send_v2` (line 33) realizes `concept:retry-with-bounded-attempts` (alias shape `blake3-512:987bda3963eeab3b…`)

The driver did not need to be told these functions are related. They were written with different syntax (different loop forms, different mutation patterns) and the canonical term-shape classifier collapses them into one cluster across `2` distinct shape-CIDs. That is the algebra compression event for this cluster: structurally different term-shapes recognized by the catalog's classification rule and merged under one named concept.

### Cluster: `concept:guard-then-commit`

- Primary shape-CID: `blake3-512:520961a119b28bf8de5e4ab8fb72047bff157694277fc37ca9ca0fda8854c13ca6f738ca2398a0a007f989ef4a1be1d985054a356d9ad3fe82177caa85e762b8`
- Sites:
  - `src/validate_then_commit.rs::commit_balance_change` (line 18) realizes `concept:guard-then-commit` (primary shape)
  - `src/validate_then_commit.rs::commit_inventory_change` (line 28) realizes `concept:guard-then-commit` (primary shape)

Every site in this cluster has the same canonical shape-CID. The compression is byte-identical at the algebra level.

## 7. The round-trip

Every fixture `.rs` was re-emitted into `rewritten/src/<basename>.rs` with a substrate-attributed header block above each fn:

```rust
// concept: <name-or-UNNAMED-CONCEPT-N>
// substrate-origin: <annotation-lift | test-lift | algebra-synthesis[rule-id]>
// memento-cid: <blake3-512:...>
#[cfg_attr(any(), requires(<pre>))]   // if pre present
#[cfg_attr(any(), ensures(<post>))]   // if post present
#[cfg_attr(any(), witness(<formula>))] // if inherited via propagation
// witness-inherited-from: <source-of-witness>
pub fn the_function(...) { ... }
```

The rewritten tree under `rewritten/` is itself a buildable Rust crate (`smoke-test-e2e-rewritten`). It is the input to pass 2.

### Round-trip diff inventory

- `src/option_handling.rs` -> `rewritten/src/option_handling.rs`
- `src/retry_with_backoff.rs` -> `rewritten/src/retry_with_backoff.rs`
- `src/validate_then_commit.rs` -> `rewritten/src/validate_then_commit.rs`

Run `diff -u /Users/tsavo/provekit-worktrees/pk-695-unstub/menagerie/smoke-test-e2e/src/option_handling.rs /Users/tsavo/provekit-worktrees/pk-695-unstub/menagerie/smoke-test-e2e/rewritten/src/option_handling.rs` (etc) to see every line the substrate added.

## 8. Zero-authoring receipt

Every contract in the rewritten output, with its substrate source traced back to its origin:

| Site | Pre | Post | Origin |
|---|---|---|---|
| `src/clamps.rs::clamp_score` | `(none)` | `(none)` | (no contract recovered) |
| `src/clamps.rs::clamp_pressure` | `(none)` | `(none)` | (no contract recovered) |
| `src/option_handling.rs::first_or_default` | `items_len >= 0` | `out == if items_len == 0 { 0 } else { 1 }` | annotation-lift (lifted from `#[requires]`/`#[ensures]` on the source) |
| `src/option_handling.rs::safe_index` | `idx >= 0` | `out >= 0` | annotation-lift (lifted from `#[requires]`/`#[ensures]` on the source) |
| `src/retry_with_backoff.rs::try_send_v1` | `max_attempts >= 0` | `(out == true) || (out == false)` | algebra-synthesis (wp_rule `wp_rule.retry-with-bounded-attempts.v0` fired structurally on the cluster) |
| `src/retry_with_backoff.rs::try_send_v2` | `max_attempts >= 0` | `(out == true) || (out == false)` | algebra-synthesis (wp_rule `wp_rule.retry-with-bounded-attempts.v0` fired structurally on the cluster) |
| `src/retry_with_backoff.rs::attempt_succeeds` | `(none)` | `(none)` | (no contract recovered) |
| `src/validate_then_commit.rs::commit_balance_change` | `(none)` | `out >= 0` | test-lift (lifted from `assert!` in `tests/properties.rs`) |
| `src/validate_then_commit.rs::commit_inventory_change` | `(none)` | `(out >= 0) || (out == before_state)` | algebra-synthesis (wp_rule `wp_rule.guard-then-commit.v0` fired structurally on the cluster) |

### Known transport losses

- The driver's `formula_text_to_value` encoding wraps each pretty-printed contract string in a single-atom IR formula (`{kind:atomic, name:"<text>", args:[]}`). The mint envelope is signed correctly and the BLAKE3-512 CID is deterministic, but the formula is opaque to the kit's parse/serialize round-trip. Discharge verdict for these mints is `loudly-bounded-lossy(formula-string-transport)`. Closing this gap requires the upstream `provekit-ir-symbolic::parse` to accept the rewritten predicate strings; until then the smoke test's transport is signed-but-opaque.
- **[CLOSED — Stub 2]** The smoke-test driver now invokes the live `libprovekit::wp` evaluator for algebra-synthesis discharge. A `SmokeTestResolver` carries authored `wp_rule`s for `retry-loop` (`max_attempts >= 0 ∧ Q`) and `guard-then-commit` (`Q`). The evaluator reduces the rule against the postcondition placeholder and returns a ground formula; verdict is `exact`. The previously blocking condition (wp-as-formula PR series — PRs #619, #633, #663) is fully merged on main; this stub is now closed. Remaining loss on algebra-synthesis sites: the synthesized contract formula is still a single-atom shim (Stub 1 — `provekit-ir-symbolic::parse` round-trip not yet wired). The discharge ITSELF is exact; the formula encoding is the remaining open gap.
- ConceptSiteMemento and ConceptAbstractionMemento are emitted with `schemaVersion: "stub-0"`. The canonical layered schema is being drafted (Opus agent `acd66a6b322284a3a` at the time of writing). The stub schema's keys (`siteFile`, `siteFn`, `conceptShapeCid`, `contractCid`, `dischargeVerdict`) match the emerging proposal so the migration to the canonical layered envelope is a renaming pass once the PR lands.

## 9. Open Karlton work

4 cluster(s) surfaced without a name. The substrate places each one's stable shape-CID alongside a `// concept: UNNAMED-CONCEPT-N` annotation in the rewritten output, inviting a human to write the name where it surfaced. Section 10 demonstrates the closure path.

| Unnamed | Shape-CID | First site |
|---|---|---|
| `UNNAMED-CONCEPT-1` | `blake3-512:8949291d51ae4f7309ee5f546bb121a7c4bde9a3683a66341e38646ba502d6f9d46ad29e66b46c1e3aae58dcc4abe46a20695c9afb7baf18b9496d2d8b901860` | `src/clamps.rs::clamp_score` |
| `UNNAMED-CONCEPT-2` | `blake3-512:4899179abf6709d682348431b4b27db120c7975dc1476ab668fd037e0a3033ff8d46211010ea81d9f2fd5ab3604bf345dfe31f07e984a9e13bda70d948f0238e` | `src/option_handling.rs::first_or_default` |
| `UNNAMED-CONCEPT-3` | `blake3-512:f103ab16bf16849782e2406e1ffb45c3c28a90f2ef0555b1f0082c53661a3d9cbae08b36eee522dbcdae797c8fa98f4a5129f7fb56c2c762d844b1fc53b43ec1` | `src/option_handling.rs::safe_index` |
| `UNNAMED-CONCEPT-4` | `blake3-512:9bb145c78148efe7769966e1a1e3bdb9b4e2cc77fa5a6ee09b1b0c6231880a29b7b205f6545412e0f52b09205402b66a5fec29b67b5cfc352bb9693bd1d208b6` | `src/retry_with_backoff.rs::attempt_succeeds` |

## 10. The naming round-trip: how Karlton's second hard thing closes inline

Karlton: there are two hard things in computer science, cache invalidation and naming things. The substrate handles the first via content-addressed mementos. This section handles the second.

When the substrate emits rewritten code with `// concept: UNNAMED-CONCEPT-N` above a cluster it could not name, that annotation IS substrate input on the next lift. A human reads the rewritten code in their editor, replaces `UNNAMED-CONCEPT-N` with a real name, and the next pass picks it up. The concept's shape-CID is stable; only the name changes.

### Annotation format

```rust
// concept: <name>
[other substrate-emitted attributes...]
pub fn the_function(...) { ... }
```

The `<name>` is a bare identifier (no `concept:` prefix). The lifter scans upward from each `fn` declaration across attribute lines for the first `// concept: <name>` and uses it. Whitespace is trimmed. Anything that is not a `#[...]` line breaks the upward scan.

### Pass 1 -> Pass 1.5 -> Pass 2

Pass 1 surfaced an unnamed cluster at shape-CID `blake3-512:8949291d51ae4f73…`. The driver wrote `// concept: UNNAMED-CONCEPT-N` above one of the cluster's sites in `rewritten/src/...`.

Pass 1.5 (simulated human edit, executed by `naming_roundtrip::apply_human_naming`) replaces that annotation with `// concept: saturating-clamp`. The change is exactly what a human editor would commit. The substrate did not generate the name; the substrate ASKED for it, the human SUPPLIED it, the substrate now LEARNS it.

Pass 2 re-lifts the same source tree (now with the human-edited annotation) and binds the same shape-CID to `concept:saturating-clamp`. Every binding to that shape-CID inherits the new name without any further human action.

Before pass 2:

- concept at shape-CID `blake3-512:8949291d51ae4f73…` had name `UNNAMED-CONCEPT-N`
  - binding at `src/clamps.rs::clamp_score` reported under `UNNAMED-CONCEPT-N`
  - binding at `src/clamps.rs::clamp_pressure` reported under `UNNAMED-CONCEPT-N`

After pass 2:

- concept at shape-CID `blake3-512:8949291d51ae4f73…` has name `concept:saturating-clamp`
  - binding at `src/clamps.rs::clamp_score` reported under `concept:saturating-clamp`
  - binding at `src/clamps.rs::clamp_pressure` reported under `concept:saturating-clamp`

The shape-CID did not move. The name attached. Every other binding inherited the new name through the concept-CID, not through a code change.

Karlton's second hard thing closes inline. No catalog editor, no separate naming workflow. The unnamed thing surfaces where the code is. The name lands where the unnamed thing surfaced. The substrate learns on the next pass.


---
Generated by `cargo run -p smoke-test-e2e-driver` against the fixture at `menagerie/smoke-test-e2e/`.
All hashes are BLAKE3-512 self-identifying strings. All `mint_contract` envelopes are signed with the shared dev seed `[0x42; 32]` per the kit default; they are reproducible across machines.
