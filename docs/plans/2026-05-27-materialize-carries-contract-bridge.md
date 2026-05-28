# Materialize carries the vendor's contract to the boundary (as a bridge)

**Branch:** `feat/materialize-carries-contract-bridge` (stacked on `chore/catalog-starts-empty` / PR #1565, which holds the hardened bridge-discharge machinery this relies on).

## The thesis (why this is the capstone)

`materialize` writes a vendor's **sugar** (body) into a **boundary** in the user's
source. The vendor ships a `.proof` carrying the sugar **and the contract you
incur by using it**. The act of materializing **creates new implications** where
none existed: a stub is a signature around a hole (a leaf, no edges); splicing
the body in gives it a call structure (a subgraph of `post → pre` edges).

Those new implications must carry the vendor's **real** contract, or adoption is
all cost and no ratchet. When they do, every contract a user adopts **shrinks
their valid-program space** — more wrong programs rejected, change gets cheaper,
**software ages backwards**. Cross-platform falls out because the contract is
platform-evacuated (canonical `Int`, see PR #1565): one `.proof`, every language's
`materialize`, the same obligation.

## The gap (grounded, exact locations)

The vendor's contract pointer is **in hand and dropped**:

- **Origin:** `RealizeRequest.contract: Option<RealizeContractPayload>`
  (`lower_plugin.rs:61`), parsed from the carrier payload at
  `lower_plugin.rs:1331` (`non_null_field(spec, &["contract"])`).
  `RealizeContractPayload.local_contract_cid` (`:494`) is the CID of the vendor's
  contract memento — a **pointer**, never inlined formulas (confirmed: the struct
  is all CIDs + verdict + witnesses). Carrying a *copy* of the obligation would
  fork its content-addressed identity — death in a content-addressed system.

- **Drop site 1:** `claim_from_realized` (`lower_plugin.rs:1437`) mints the
  lower-realize claim contract via `memento_from_parts(..., formula_true(),
  formula_true(), ...)` — vacuous — with `invocation.request.contract` ignored
  in scope.

- **Drop site 2:** `claim_from_receipt` (`source_transform_kit.rs:219`) mints the
  source-transform claim contract `true → true` likewise ("trivial pre/post").

- **Why it can't carry today:** `RealizedSource` (`lower_plugin.rs:519`) has no
  contract field, so even though `MaterializeKit::transform_site`
  (`cmd_materialize.rs:1834`) has the realized source, the pointer is already
  gone by then. `binding_cid` it records = `emitted_artifact_cid` (the *body's*
  CID), not the contract.

## The fix (the bridge — "you get the rest for free")

Emit a **bridge** at the boundary write. The materialized boundary becomes a
callsite the verifier already chews: `enumerate_callsites` finds the bridged
symbol, `resolve_target` resolves the vendor contract **by CID**, and the
hardened discharge from PR #1565 runs — **zero new enforcement code**. Producer
callsites and library boundaries collapse into one mechanism.

Touch-points:

1. **`RealizedSource`** (`lower_plugin.rs:519`): add
   `#[serde(default, skip_serializing_if = "Option::is_none")] pub contract_cid: Option<String>`.
2. **`claim_from_realized`** (`:1398`): `mut realized`; set
   `realized.contract_cid = invocation.request.contract.as_ref().map(|c| c.local_contract_cid.clone())`
   before `realized_source_term(&realized)`, so it round-trips through the claim
   payload and `realized_source_from_claim` (`:613`) deserializes it back.
3. **`SiteOutcome::Materialize` / `LoudlyLossy`** (`source_transform_kit.rs`):
   carry the contract CID alongside `binding_cid`.
4. **The boundary write** (`MaterializeKit::transform_site` /
   `claim_from_receipt`): when `contract_cid` is `Some`, emit a
   `BridgeHeaderV14` (`sourceSymbol` = boundary site, `sourceContractCid` =
   `local_contract_cid`, target = vendor contract memento) **instead of** the
   `true → true` mint. `None` (vendor declared no contract) legitimately carries
   nothing.
5. **verify**: free — the bridge flows through the existing discharge.

## Done = (grounding + e2e)

- **Grounding (Task #75):** materializing a boundary emits a bridge whose
  `sourceContractCid` = the vendor's `local_contract_cid`, NOT a fresh
  `true → true`. Red on current code, green after.
- **E2E (Task #78), the boundary-side analog of `rust-missing-edge`:** a vendor
  sugar with a real contract, materialized into a user boundary whose surrounding
  use violates it → `provekit verify`/`prove` **refuses** (the squiggly). Today it
  false-greens. This is "software ages backwards," demonstrated.

## Invariants (do not violate)

- Carry the **CID**, never the formulas. Inlining forks identity = death.
- The verifier stays language-blind; the bridge references a canonical contract
  CID. No platform intrinsic enters the substrate (see
  `project_provekit_platform_in_sidecar`).
