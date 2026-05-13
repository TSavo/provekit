# CanonicalizationProfileMemento Normative Spec

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-13
**Author:** T Savo
**Related:**
- `2026-04-30-canonicalization-grammar.md`
- `2026-05-09-language-signature-protocol.md`
- `2026-05-12-plugin-protocol.md`
- `2026-05-13-effect-occurrence-memento.md`
- `2026-05-13-promotion-decision-memento.md`
- `2026-05-13-proof-run-memento.md`
- `2026-05-13-sort-morphism-memento.md`
- TSavo/provekit#791
- TSavo/provekit#792
- TSavo/provekit#794
- TSavo/provekit#796
- TSavo/provekit#799

## §0. Purpose

JCS canonicalization stabilizes JSON whitespace, number spelling, and object key order. It does not make two semantically equivalent formulas, binders, formal names, or concept shapes byte-identical. If one lift plugin emits `eq($return, $arg0)` and another emits `eq($return, $arg_0)`, raw CIDs diverge even when the formulas are alpha-equivalent and refer to the same formal slot.

`CanonicalizationProfileMemento` declares the conservative canonicalization profile applied before a substrate object is content-addressed. It lets a run say, by CID, which harmless representation drift it collapses and which drift it preserves. This keeps continuity across plugin upgrades without hiding semantic change.

Canonicalization is a controlled, conservative loss. A profile MAY collapse spelling, binder names, redundant parentheses, or explicitly pinned aliases when the rule documents why the input and output denote the same substrate object. A profile MUST NOT collapse facts whose difference changes semantics. That is not canonicalization; it is a different object and must be represented by a different CID or by an explicit discharge memento.

This spec preserves policy choice as content-addressed input. It defines how a profile is declared and cited. It does not decide which profile a project must use, which plugin output is admissible, or which semantic claims are true.

## §1. Wire shape (CDDL)

```cddl
; Shared scalar types:
;   cid, json-value
;
; Locked JCS key order: alphabetical within each object.
; Producers MUST emit empty arrays, not omitted fields, for rule lists with
; no entries. Optional rule fields are omitted when absent.

canonicalization-profile-memento = {
  alpha_equivalence_rules:        [* rule-descriptor],
  binder_normalization_rules:     [* rule-descriptor],
  formal_name_normalization_rules: [* rule-descriptor],
  formula_canonicalization_rules: [* rule-descriptor],
  profile_kind:                   profile-kind,
  profile_version:                tstr,
  provenance_cid:                 cid,
  sort_alias_rules:               [* rule-descriptor],
  unsupported_equivalence_policy: unsupported-equivalence-policy
}

profile-kind = "ir-formula"
             / "concept-shape"
             / "function-contract"
             / namespaced-kind

unsupported-equivalence-policy = "preserve"
                              / "refuse"
                              / namespaced-kind

; A namespaced extension label. Producers MUST use a colon-qualified string,
; for example "acme:ir-formula-v2".
namespaced-kind = tstr

rule-descriptor = {
  description:             tstr,
  ? language_signature_cid: cid,
  ? reference_cid:          cid,
  rule_id:                 tstr,
  ? rule_payload:           json-value,
  rule_version:            tstr
}

cid = tstr
json-value = any
```

## §2. Field semantics

| Field | Required | Meaning |
|---|---:|---|
| `alpha_equivalence_rules` | yes | Ordered set of documented rules that decide when bound-variable renaming preserves formula identity. Empty means no alpha-equivalence collapse beyond byte identity. |
| `binder_normalization_rules` | yes | Ordered set of rules that normalize bound-variable representation, such as De Bruijn indexing. Empty means binder spelling is preserved. |
| `formal_name_normalization_rules` | yes | Ordered set of rules that normalize formal parameter and return-slot names, such as `$arg0` and `$arg_0` to a declared canonical form. Empty means formal spelling is preserved. |
| `formula_canonicalization_rules` | yes | Ordered set of rules for syntax-only formula canonicalization, such as redundant-parenthesis removal or commutative-argument sorting. Empty means formula surface structure is preserved except for JCS. |
| `profile_kind` | yes | Scope of the profile. Canonical labels are `ir-formula`, `concept-shape`, and `function-contract`. Extension labels MUST be namespaced strings such as `vendor:kind`. |
| `profile_version` | yes | Producer-declared version string for this profile line. Consumers MUST compare profile CIDs for identity; version is for operator readability and compatibility screens. |
| `provenance_cid` | yes | CID of the provenance record, audit run, reference implementation record, or equivalent memento that explains how the profile was produced and reviewed. For deterministic profiles, this SHOULD point at the audit run that confirmed byte-stable output. |
| `sort_alias_rules` | yes | Ordered set of rules that permit sort alias collapse only under explicit language-signature pins. Empty means sort spelling is preserved. |
| `unsupported_equivalence_policy` | yes | Behavior when the canonicalizer sees an equivalence class it does not understand. `preserve` keeps the input unchanged. `refuse` produces no canonical object. Namespaced policies are extension-defined and MUST document whether they are at least as conservative as `preserve`. |

Each `rule-descriptor` is a documented behavior declaration. It is not a hidden algorithm slot. `rule_id` names the rule, `rule_version` names the rule line, `description` states the normative behavior, `reference_cid` MAY point at a reference implementation, proof note, or spec fragment, `language_signature_cid` pins language-specific rules, and `rule_payload` carries structured parameters when text alone is insufficient. Rule descriptors are interpreted in array order. If two rules conflict, the canonicalizer MUST refuse unless the later rule explicitly documents and resolves the conflict.

## §3. Conservative rule examples

### §3.1 Binder normalization

A binder rule MAY replace bound-variable names with De Bruijn indices when the binder structure is explicit in the IR and the rule can prove that every variable occurrence resolves to the same binder before and after normalization.

Example permitted behavior: `forall x. exists y. gt(x, y)` and `forall a. exists b. gt(a, b)` canonicalize to the same indexed representation.

Example refused or preserved behavior: a free variable named `x` MUST NOT be reclassified as bound because another formula binds an `x` elsewhere.

### §3.2 Formal name normalization

A formal-name rule MAY map syntactic variants of the same declared slot to a canonical name when the function signature pins the slot order and slot identity.

Example permitted behavior: `$arg0`, `$arg_0`, and `$0` normalize to `$arg0` only when the signature says they refer to formal parameter slot zero.

Example outside the rule: `$arg0` and `$receiver` MUST NOT collapse unless the profile has a rule that cites the language signature proving those names denote the same slot.

### §3.3 Alpha equivalence

An alpha-equivalence rule MAY collapse formulas that differ only by bound-variable names. The rule MUST preserve binding structure, free-variable identity, sort annotations, and occurrence roles.

Example permitted behavior: `lambda x. add(x, 1)` and `lambda y. add(y, 1)` are equivalent when `x` and `y` are bound by the lambda being compared.

Example refused or preserved behavior: `lambda x. add(x, z)` and `lambda y. add(y, x)` are not alpha-equivalent when the trailing `x` is free in the second formula and `z` is free in the first.

### §3.4 Sort alias rules

A sort-alias rule MAY collapse aliases such as `int` and `i32` only when a `language_signature_cid` pins the language, ABI, and version that make the alias exact. The rule MUST cite the sort declaration or equivalent reference through `reference_cid` or `rule_payload`.

Example permitted behavior: C `int` and `i32` collapse only under a language signature that fixes `int` as signed 32-bit two's-complement with no extra trap representation.

Example outside the rule: Rust `i64` and Java `long` MUST NOT be treated as spelling aliases by this profile. Cross-language sort identity belongs to `SortMorphismMemento` (#794).

### §3.5 Formula canonicalization

A formula rule MAY reorder arguments to a declared commutative operator, remove redundant parentheses, or normalize associative grouping when the operator declaration proves that the transformation preserves the same formula.

Example permitted behavior: `and(b, a)` canonicalizes to `and(a, b)` when `and` is declared commutative and the sort of both operands is unchanged.

Example refused or preserved behavior: `sub(a, b)` MUST NOT reorder to `sub(b, a)`. A function call with effects MUST NOT reorder arguments unless the language signature and effect occurrence payload prove the order is unobservable.

## §4. What stays outside canonicalization

Canonicalization MUST NOT collapse differences where the CID change reflects real semantic change.

Sort identity across language signatures stays outside this profile. There is no implicit `rust:i64` to `java:long` or `c:int` to `go:int` canonicalization. Cross-language sort transport requires an explicit `SortMorphismMemento` (#794) or successor memento that records direction, losses, guards, and language signature pins.

Effect occurrence arguments stay outside string canonicalization. Per `EffectOccurrence` (#793), effect payloads are structured semantic data. A rule may JCS-canonicalize the JSON object keys, but it MUST NOT convert an effect occurrence to a canonical string and then forget fields such as role, locator, target, ordering, or discharge key.

Function-contract bodies that carry semantic state stay outside surface canonicalization. A changed precondition, postcondition, effect occurrence, weakest-precondition rule, body term, guard, or discharge obligation is a changed contract unless another memento proves an admissible relationship.

Free-variable identity stays outside alpha-equivalence. Bound-variable renaming can be canonicalized; changing which free variable a formula mentions changes the formula.

Evaluation order, control-flow structure, side-effect ordering, exceptional exits, and runtime guards stay outside formula normalization unless a referenced language signature and proof obligation establish that the transformation is semantics preserving.

Policy decisions stay outside canonicalization. Whether to accept a profile, promote a canonicalized object, discharge an implication, or admit a plugin run is recorded by policy, promotion, discharge, and run mementos. A canonicalization profile only declares the byte-level normalization applied before CID construction.

## §5. Plugin-registry and provenance interaction

A minting run that applies a canonicalization profile MUST make the profile replayable.

The sealed `PluginRegistryMemento` plus the relevant `RunMemento` (#799) MUST cite the `CanonicalizationProfileMemento` CID applied by each plugin stage that emits canonicalized substrate objects. The citation may be in a profile slot defined by the stage, in the plugin content payload, or in a run receipt, but it must be content-addressed and stage-specific.

`RunMemento` (TSavo/provekit#799) and the stage receipts of a run cite the `profile_cid` used to produce the run's outputs. Future provenance profiles MAY also reference `profile_cid`, but the existing `2026-05-06-provenance-memento.md` records raw-pointer source provenance and is NOT the citation site for canonicalization-profile selection. Keeping these distinct lets a verifier replay the same plugin version, same profile, same inputs, and same canonicalization rules before comparing output CIDs without conflating raw-pointer provenance with run/build provenance.

If a plugin emits objects under more than one profile in the same run, each output object or receipt MUST identify which `profile_cid` applied. A run-level default is only valid for outputs that do not override it.

## §6. Fail-closed behavior

If `unsupported_equivalence_policy = "preserve"` and the canonicalizer encounters an unsupported equivalence, it MUST leave the unsupported fragment unchanged and continue. This is the conservative default: under-canonicalization can split CIDs, but it does not erase semantic distinctions.

If `unsupported_equivalence_policy = "refuse"` and the canonicalizer encounters an unsupported equivalence, it MUST produce no canonical object for that input and MUST emit or return a refusal diagnostic.

If a consumer expects profile A and the producer applied profile B, the consumer MUST refuse to consume the object as equivalent under A. It MAY fetch B and replay under B, but it MUST NOT silently substitute one profile for another.

If a rule produces different canonical bytes for the same input, same profile CID, same plugin registry, and same run inputs, the canonicalizer MUST refuse and flag the rule as broken. The profile's `provenance_cid` SHOULD point at an audit run or reference implementation record that confirmed determinism for the declared rule set.

If a rule descriptor is unknown to the implementation and the implementation cannot execute or verify it through `reference_cid`, the canonicalizer MUST apply `unsupported_equivalence_policy`. It MUST NOT guess.

## §7. CID construction

`CanonicalizationProfileMemento` CIDs are derived from JCS-canonical bytes and BLAKE3-512:

```text
cid_input = JCS({
  "alpha_equivalence_rules":        <alpha_equivalence_rules>,
  "binder_normalization_rules":     <binder_normalization_rules>,
  "formal_name_normalization_rules": <formal_name_normalization_rules>,
  "formula_canonicalization_rules": <formula_canonicalization_rules>,
  "profile_kind":                   <profile_kind>,
  "profile_version":                <profile_version>,
  "provenance_cid":                 <provenance_cid>,
  "sort_alias_rules":               <sort_alias_rules>,
  "unsupported_equivalence_policy": <unsupported_equivalence_policy>
})

cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

All listed fields participate in the CID. Rule arrays are order-sensitive because rule order can affect output. A producer that wants set-like rule identity MUST sort the rule list by a documented key before minting and then treat that sorted list as the normative rule order.

Changing any rule descriptor, profile kind, profile version, provenance CID, or unsupported-equivalence policy mints a different profile CID.

## §8. Cross-references

- Promotion decisions: `2026-05-13-promotion-decision-memento.md`, TSavo/provekit#791.
- Discharge receipts: TSavo/provekit#792.
- Sort morphisms: `2026-05-13-sort-morphism-memento.md`, TSavo/provekit#794.
- Run and pipeline mementos: `2026-05-13-proof-run-memento.md`, TSavo/provekit#799.
- Admissibility spine: TSavo/provekit#796.
- Plugin protocol and `PluginRegistryMemento`: `2026-05-12-plugin-protocol.md`.
- Language signatures: `2026-05-09-language-signature-protocol.md`.
- JCS canonicalization: `2026-04-30-canonicalization-grammar.md`.

## §9. Out of scope

This spec does not define a full rule language. Rules are documented as text plus optional structured payload and reference implementation CIDs. A formal rule sublanguage is future work.

This spec does not define cross-profile composition. Applying profile A and then profile B is not automatically equivalent to a new profile C. A composed profile must be minted explicitly, audited for determinism, and cited by CID.

This spec does not decide which canonicalization profile is default for a repository, language, plugin, or run. That decision belongs to policy and registry mementos.
