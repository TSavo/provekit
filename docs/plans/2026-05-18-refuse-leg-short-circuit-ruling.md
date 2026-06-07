# Refuse-Leg Short-Circuit Ruling

Date: 2026-05-18
Status: Active. Implemented in provekit-cli/src/cmd_bind_migrate.rs via PR #1204.

## Ruling

When any callsite in a migrate is uncharacterizable (substrate cannot characterize the divergence; routes to refuse leg of the trichotomy), the entire receipt is a pure refusal:

- All `OpCoverageVerdict::Uncharacterizable` callsites emit `RefusalMemento` entries into `refusal_mementos`.
- `aggregate_summary.refused` counts those refusals.
- `loss_records` is suppressed entirely. ZERO `LossRecordMemento` entries emitted, even for callsites that were independently characterized as `Divergent`.
- `aggregate_summary.lossy = 0` under refuse.

## Why this shape

The trichotomy is **exact / loudly-bounded-lossy / refuse**. Three legitimate outcomes; they are mutually exclusive at the substrate level. A receipt cannot be both "loudly-bounded-lossy" AND "refuse" because the loudly-bounded-lossy claim REQUIRES that every divergence is characterized; refuse is the explicit admission that at least one is not.

Emitting partial loss-records alongside refusal mementos would conflate the two outcomes:
- Caller might infer: "lossy=3, refused=1, so I have 3 characterized losses plus 1 unknown."
- Substrate truth: when ANY callsite is uncharacterizable, the substrate cannot make ANY loudly-bounded-lossy claim about ANY callsite in this migrate. The presence of uncharacterizable evidence taints the receipt's "I've characterized everything within this scope" property.

This is "Supra omnia, rectum" applied to compound claims. The receipt's claim is about the migrate as a whole, not piecewise. If any leg refuses, the whole receipt refuses.

## P0 origin

PR #1201 shipped the production wiring with this bug: `cmd_bind_migrate.rs` collected `uncharacterizable_callsites` into the `PlatformSemanticChangeSet` but `build_receipt` never consumed them. The substrate primitive correctly returned `Uncharacterizable`; the consumer silently dropped it. Caught empirically when typescript-better-sqlite3 to python-sqlite3 migration emitted `refused=0, lossy=0, refusal_mementos=[]` despite python-sqlite3 having no binding-kit declaration for `concept:insert-and-get-id`.

PR #1204 fixed by wiring uncharacterizable_callsites into refusal_mementos AND applying the short-circuit (suppress loss_records when any uncharacterizable is present).

## Discipline

- ALL substrate consumers of `OpCoverageVerdict::Uncharacterizable` MUST route through a refusal memento. Silent drops are bugs.
- ANY new receipt-builder that handles propagation decisions MUST short-circuit lossy emission when uncharacterizable callsites are present. The check is `if uncharacterizable_callsites.is_empty() { emit lossy }`.
- Tests asserting on the refuse leg MUST assert BOTH the refusal memento's presence AND the absence of loss records (the cross-assertion that distinguishes refuse from mixed-leg receipt).
- `aggregate_summary.refused >= 1 AND aggregate_summary.lossy = 0` is the invariant; tests must pin both halves.

## Future work

- Same short-circuit logic must apply if other refuse-class verdicts are added (e.g., async-effect refuse, contract-forbid refuse). Adding a new refuse-class without short-circuiting lossy is a trichotomy violation.

## Cross-references

- PR #1204: implementation + Stage 5 CI gate (fixture 5 specifically pins the short-circuit invariant)
- [[2026-05-18-op-coverage-verdict-trichotomy-ruling]] (the verdict that triggers this short-circuit)
- Sugar first principle: "Supra omnia, rectum, never claim more than you can prove"
- `project_provekit_first_principle` memory (trichotomy: exact / loudly-bounded-lossy / refuse)
