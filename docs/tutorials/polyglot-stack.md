# Tutorial: Polyglot stack

> **Status:** stub. This is the load-bearing tutorial for the thesis. Until it ships as a runnable end-to-end demo, the cross-domain claim remains prose. Contributions especially welcome.

A walkthrough demonstrating ProvekIt's central claim: **a proof minted in one language transfers to another for free, by walking content-addressed bridges anchored to shared reference contracts.**

## What this tutorial proves

A polyglot service stack:

- **TypeScript** frontend with `zod` validation
- **Python** ML service with `pydantic` schemas
- **Rust** backend with `proptest` properties
- **Go** API gateway with `validate:` struct tags

Each kit lifts its language-native annotations into IR. Where two implementations describe semantically-equivalent behavior, both bridge to the same **reference contract** (e.g., `ref-parseInt-v1`, `ref-email-format-v1`). The handshake at every cross-language call site discharges at **Tier 1** (one CPU instruction) because the bridge endpoints share a CID.

## 1. What you'll have at the end

A monorepo with four services in four languages, each shipping a `.proof`. A consumer of any service in any language gets the same Tier-1 discharge fraction as a same-language consumer would. The discharge breakdown is the proof of the claim.

## 2. The load-bearing primitive: reference contracts

The bridge mechanism only works if multiple implementations bridge to the **same** target CID. That target is a curated reference contract — a canonical anchor like:

- `ref-parseInt-v1` (ECMA-262 `parseInt` semantics)
- `ref-email-format-v1` (RFC 5322 email validation)
- `ref-uint32-arithmetic-v1` (32-bit unsigned arithmetic)

These live in [docs/reference-contracts/](../reference-contracts/). They are the bridge anchors the per-language adapters target.

A TypeScript `z.string().email()` adapter emits a bridge:
```json
{
  "kind": "bridge",
  "sourceContractCid": "bafy...zod-string-email-v1",
  "targetContractCid": "bafy...ref-email-format-v1",
  "targetProofCid": "bafy...ref-contracts-v1-proof"
}
```

A Python `EmailStr` pydantic adapter emits the same shape:
```json
{
  "kind": "bridge",
  "sourceContractCid": "bafy...pydantic-emailstr-v1",
  "targetContractCid": "bafy...ref-email-format-v1",
  "targetProofCid": "bafy...ref-contracts-v1-proof"
}
```

Both bridge to `ref-email-format-v1`. The verifier walks `zod-string-email-v1 → ref-email-format-v1 ← pydantic-emailstr-v1` and discharges the cross-language call site without invoking Z3.

## 3. The walkthrough (TODO)

This section needs to be written as a runnable monorepo demo. Proposed structure:

```
examples/polyglot-stack/
├── frontend-ts/        # zod-validated form submission
├── ml-python/          # pydantic-schemed feature inference
├── backend-rust/       # proptest-asserted invariants
├── gateway-go/         # validate-tagged request shapes
├── reference-contracts/
│   └── ref-email-format-v1.proof
├── Makefile            # `make proof` runs all kits, then `make verify` runs the cross-stack handshake
└── README.md
```

Each service's `.proof` is built independently. The verifier walks the union and reports per-service and cross-service discharge fractions.

## 4. Expected output

```
provekit prove --polyglot

per-service breakdown:
  frontend-ts:   18 contracts, 14 bridges, 16/18 discharged at Tier 1 (89%)
  ml-python:     22 contracts, 19 bridges, 19/22 discharged at Tier 1 (86%)
  backend-rust:  31 contracts, 27 bridges, 28/31 discharged at Tier 1 (90%)
  gateway-go:    12 contracts, 11 bridges, 11/12 discharged at Tier 1 (92%)

cross-service handshake:
  frontend-ts → gateway-go:    7 call sites, 7 Tier-1, 0 Tier-2, 0 Tier-3
  gateway-go → backend-rust:   12 call sites, 11 Tier-1, 1 Tier-2, 0 Tier-3
  backend-rust → ml-python:    4 call sites, 4 Tier-1, 0 Tier-2, 0 Tier-3
  total: 23 cross-language call sites, 22 Tier-1 (96%)

hash-discharge fraction (cross-language): 0.96
```

The 96% fraction is the proof of the claim. Tier 1 means `memcmp` returned zero on the publisher's post-hash and the consumer's pre-hash — one CPU instruction per discharge. No solver invocation, no signature check, no symbol-name lookup. Just hash equality across language boundaries.

## What this validates

The thesis says: "the verification problem at supply-chain scale has the same shape as currency, source history, content distribution, and the addressable web." This tutorial is the empirical demonstration of that shape. If it ships as a runnable demo with the breakdown above, the polyglot claim is no longer prose. Until it ships, the claim is a roadmap, not a result.

## What's next

- [docs/explanation/thesis.md](../explanation/thesis.md) — the full claim.
- [docs/explanation/cross-domain-verification.md](../explanation/cross-domain-verification.md) — the mechanism in depth.
- [docs/reference-contracts/](../reference-contracts/) — the curated anchor set this demo depends on.
- [docs/contributing/porting-to-a-new-language.md](../contributing/porting-to-a-new-language.md) — to add another service in your language.

---

*This tutorial is a stub describing the demo to be built. Building the runnable demo (under `examples/polyglot-stack/`) is the highest-leverage Tier 1 work in the docs IA: it converts the thesis from claim to demonstration. Contributions especially welcome.*
