# 2026-05-30 Callee resolution tiers (per-kit, language-native)

Status: NORMATIVE (lift-plugin protocol extension)

## Motivation

The implication lifter bridges a call site to the contract that call must
discharge against. It currently matches by the callee's BARE name (the last
path segment, or the method ident). Bare names are ambiguous in two ways that
make a bridge unsound:

1. Cross-crate. A consumer's `json_to_cvalue` and a vendored dependency's
   `json_to_cvalue` are different contracts with the same leaf. Bare matching
   cannot tell them apart, so same-named dependency contracts had to be dropped
   at mint and the verifier logged duplicate-name collisions.
2. Receiver-typed dispatch. `x.unwrap()` is `Option::unwrap` or `Result::unwrap`
   depending on the type of `x`; `x.get(k)` depends on the receiver. The leaf
   `unwrap` alone names neither contract.

This ambiguity exists in EVERY kit (the implication lifter is per-language). A
fix that is specific to one language's tooling (e.g. wiring rust-analyzer into
the Rust kit) does not generalize and leaks language knowledge into a layer
that must stay blind. This spec defines callee resolution as a uniform,
per-kit, language-native capability with conformance tiers.

## §1 The resolution obligation

The lift-plugin protocol already hands each kit's implication lifter the
consumer's contract bindings as opaque records carrying at least
`{ leaf, library, contract_cid, target_proof_cid, body_bearing }`. The
substrate never inspects source; the kit owns all AST and type knowledge.

- §1.R1 (Qualified match). A kit MUST resolve each call site to at most one
  binding by a QUALIFIED identity, never by bare leaf. The minimum qualified
  key is `(library, leaf)`, where `library` is the crate / module / package the
  callee is defined in (kit-derived, the source of truth being the build
  manifest, not a sugar/concept label) and `leaf` is the callee's simple name.
  A kit MAY use a richer canonical symbol internally; the substrate only
  consumes the resulting `targetContractCid` + `targetProofCid`.

- §1.R2 (Refuse floor, supra omnia rectum). If a kit cannot determine the
  callee's resolved identity, it MUST emit a `lift-gap` for that call site and
  MUST NOT bridge to a guessed contract. An unresolved call is a recorded loss,
  never a wrong edge. This floor holds at every tier.

- §1.R3 (Pin composition). Resolution selects the consequent CONTRACT; the
  bundle it is allowed to come from is pinned separately by `targetProofCid`
  (see `2026-04-30-ir-formal-grammar.md`, Bridge target pinning). A kit MUST
  carry the resolved binding's `target_proof_cid` onto the bridge so
  `ConsequentBundlePinned` is enforced. Qualified identity answers "which
  contract"; the proof CID answers "from which bundle". Both are required.

## §2 Conformance tiers

A kit declares the highest tier it implements. Higher tiers resolve strictly
more call sites; every tier obeys §1.R2 (refuse the rest).

- §2.T1 (Syntactic, REQUIRED). Import-graph plus path qualification, no
  compiler. Build a per-file map of imported leaf to defining
  module/package/crate root (the language's `use` / `import` / `@import`
  forms), and resolve fully-qualified and import-resolved free-function calls
  to `(library, leaf)`. A `crate`/`self`/`super`-rooted (or equivalent
  same-unit) path resolves to the current unit. Every kit MUST reach T1; it is
  the universal floor and needs no toolchain.

- §2.T2a (Local type-flow, RECOMMENDED). Within a function body, track the
  types of local bindings from annotations and from calls whose return type is
  locally known, then resolve a receiver-typed call (`x.method()`) to the crate
  of the receiver's type. No compiler. Refuse when the receiver type is not
  locally determinable.

- §2.T2b (Native semantic oracle, OPTIONAL). Delegate to the language's own
  semantic analyzer for full dispatch: overload resolution, trait / interface
  dispatch, generic instantiation, re-export transparency. The kit invokes its
  analyzer as a subprocess (the same shape it already uses to spawn its lifter)
  and parses the canonical resolved callee. T2b changes NO substrate code; it
  upgrades only the kit's resolver behind the §1 obligation.

## §3 Per-language profile

The oracle and the expected coverage differ by language. A kit SHOULD document
its profile and record what it refuses as loss, not as failure.

| kit    | T2b oracle             | static dispatch | expected coverage |
|--------|------------------------|-----------------|-------------------|
| go     | `go/types`             | full, simple    | T1 covers most; oracle is cheap |
| zig    | the zig compiler       | full, explicit  | T1 covers most; comptime resolved by oracle |
| rust   | rust-analyzer / rustc  | full, with traits + generics | needs T2b for trait/method dispatch |
| java   | `JavacTask` / JDT      | full, overloaded | needs T2b for overload + interface dispatch |
| python | pyright / mypy, best-effort | dynamic | T1 only is sound; method resolution is often statically impossible |

- §3.R1 (Honest dynamism). A dynamically-typed kit (Python) is NOT required to
  reach T2a/T2b soundly. Where static resolution is impossible it MUST refuse
  (§1.R2) and SHOULD emit a loss-record naming the unresolved call, so the gap
  is visible rather than papered over with a bare-name guess.

## §4 The canonical truth

The qualified symbol and `(library, leaf)` key are kit-internal resolution
HANDLES. The language-neutral identity of the consequent is its contract CID.
Resolution is the act of computing which contract CID a call site targets;
names are sugar (`2026-... function-names-are-sugar`). Two kits that resolve
the "same" call to the same contract MUST agree on the contract CID, not on the
spelling of the symbol. The substrate compares CIDs; it never compares names.

## §5 Conformance checklist

A kit claiming a tier MUST:

- T1: build the import graph and resolve qualified + import-resolved free
  calls; key bridges by `(library, leaf)`; stamp `library` on the contracts it
  lifts (the defining unit's manifest name); carry `target_proof_cid` (§1.R3);
  refuse the rest (§1.R2).
- T2a: additionally track local binding types and resolve receiver-typed calls
  that are locally determinable.
- T2b: additionally delegate to its native analyzer for dispatch its syntactic
  tiers cannot decide, as a subprocess, with no substrate dependency on the
  analyzer.

A kit MUST NOT emit a bridge whose target it resolved by bare leaf alone. The
floor everywhere is: resolve what the tier can, refuse the rest, never guess.
