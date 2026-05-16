# Trinity Path B blocker: bind-result wrapper not consumable by LowerKit

**Status:** STOPPED, awaiting architect ruling.
**Issue:** #1068 (Trinity exhibit Path B), parent #1024.
**Branch:** `1068-trinity-exhibit-path-b`.
**Date:** 2026-05-16.

## Summary

Path B's seven-step algebra `[lift, bind, lower, relift, rebind, lower-back, prove]`
cannot be composed from kits currently on main. The `lower` step refuses at
runtime because BindKit (after A3 / #1065) wraps its output in a
`Term::Op { op_cid: concept:bind-result, args: [original_term, named_form_binding] }`,
and LowerKit's spec-extraction path on `Input::Claim` does not know how to
descend through that wrapper. The Python realize plugin then refuses with
`missing body-template entry` for the synthesized operation kind
`bind::default::bind-result-op-tree`.

This is a real substrate integration gap, surfaced for the first time by the
attempt to compose lift -> bind -> lower end-to-end through the kit registry
on a real Rust source.

Per the Path B brief's HARD REQUIREMENT clause, this PR does not invent a fix.
The brief explicitly preserves the antibody pattern: surface the blocker with
file:line citations and options, do not assume substrate behavior that does
not exist on main.

## Empirical probe (uncommitted, deleted before commit)

A scratch probe under `provekit-cli/tests/trinity_probe.rs` ran the first
three steps of the algebra against real toolchains:

- `provekit-walk-rpc` for Rust lift.
- `BindKit::default()` for bind.
- `LowerKit::new(..., "python", ..., DispatchRealizeTransport)` invoking the
  real `provekit-realize-python-core` PEP 1.7.0 plugin under `PYTHONPATH`
  pointing at `implementations/python/...src/`.

Results:

- **Rust lift succeeded.** Output: `ir-document` with one
  `bind-lift-entry` for `pub fn id(x: i64) -> i64`.
- **Bind succeeded.** Post-bind term is
  `Term::Op { op_cid: concept:bind-result, args: [...] }` per A3 / #1065.
  Post-bind CID:
  `blake3-512:c26b6757393f6eef52064e0c80f5b3363ed13b8713642aecaab588e20c1c0c32d38654f5d106689da7161b01fa704e60aed1758f4887159c4c0ee85b5563639e`.
- **Determinism sentinel: PASSED.** Two distinct `BindKit::transform` calls
  on the same lifted Term produced byte-identical post-bind CIDs. BindKit is
  invocation-deterministic.
- **Lower failed.** Refusal payload (verbatim from the probe log):

  ```
  LOWER FAILED: kit transform failed: realize plugin transport:
    kit-plugin-unavailable: no realize plugin for language `python`
    (realize kit error: {"code":-32100,"message":"missing body-template entry",
     "data":[{"operation_kind":"bind::default::bind-result-op-tree",
              "args_shape":["LiftPluginResponse"],
              "function":"bind::default::bind-result-op-tree",
              "term_position":"body"}]})
  ```

Because step 3 (`lower`) does not complete, steps 4-7 cannot be executed and
the federation byte-identity assertion (assertion 4 of the six) is not
empirically tested. The probe was intentionally minimal and is not committed.

## Mechanism (file:line citations)

`implementations/rust/libprovekit/src/core/bind.rs:317-328` defines
`bind_result_payload`:

```rust
pub fn bind_result_payload(
    original_term: Term,
    named: &NamedTermDocument,
) -> Result<Term, BindError> {
    let catalog = ConceptOpCatalog::load()?;
    let named_form_binding = named_term_document_op_tree(named, &catalog)?;
    Ok(Term::Op {
        op_cid: concept_bind_result_cid(),
        name: CONCEPT_BIND_RESULT.to_string(),
        args: vec![original_term, named_form_binding],
    })
}
```

`implementations/rust/libprovekit/src/core/bind.rs:357-384` defines
`bind_response_contract`, which sets `fn_name =
"bind::default::bind-result-op-tree"`. The bind claim's
`contract.fn_name` therefore carries that synthetic name.

`implementations/rust/libprovekit/src/core/lower_plugin.rs:210-228` defines
`claim_spec_value`:

```rust
fn claim_spec_value(claim: &DomainClaim) -> Result<Value, String> {
    if let Some(Term::Const { value, .. }) = &claim.payload {
        return Ok(value.clone());
    }
    let param_types = ...;
    Ok(json!({
        "function": claim.contract.fn_name, // -> "bind::default::bind-result-op-tree"
        ...
        "conceptName": claim.contract.concept_hint.clone()
                        .unwrap_or_else(|| claim.contract.fn_name.clone()),
        ...
    }))
}
```

The fast path matches only `Term::Const`. The new `Term::Op { ... }` shape
A3 introduced takes the fallback branch and synthesizes a RealizeRequest with
`function = bind::default::bind-result-op-tree`. No realize plugin has a
body-template for that name; the plugin refuses correctly with
`missing body-template entry`.

A3 (#1065) updated `bind_result_payload` but did not update
`claim_spec_value` to decompose the wrapper. The wire-compat preservation in
A3 (`parse_named_or_bind_payload`) covers the read side but not the
"feed bind output to lower" composition.

## Options for the architect

No recommendation is made here. Path B is paused pending ruling.

### (a) Tiny substrate fix in `claim_spec_value`

When `claim.payload` is
`Some(Term::Op { op_cid: concept:bind-result, args })`, extract `args[1]`
(the `named_form_binding` op tree per A3's contract) and either feed it
into the existing JSON-extraction shape OR descend into the underlying
NamedTermDocument and synthesize the same `RealizeRequest` that
`cmd_lower` builds today from `--named-terms-json`.

Estimated diff: ~30-60 LOC in `lower_plugin.rs`, plus a unit test in
`bind_kit_path_integration.rs` style.

Frame: a missed-integration fix on A3, NOT a Path B scope expansion.
Mechanically it is the smallest possible change. Violates Path B's HARD
REQUIREMENT clause ("does NOT modify kits") in letter; arguably honors it
in spirit (closes a regression A3 introduced rather than adding new kit
behavior).

### (b) Add an explicit `decompose` step to the seven-step algebra

`[lift, bind, decompose, lower, relift, rebind, decompose, lower-back, prove]`.
A new kit translates `Term::Op { concept:bind-result }` into the
RealizeRequest shape `LowerKit` already understands.

New kit. Out of Path B's scope (HARD REQUIREMENT). Re-opens the algebra,
which #1024's editorial framing locks at seven steps.

### (c) New prereq A7: ship `claim_spec_value` decomposition first

Split (a) into its own prereq PR (call it A7), land it on main, then
re-dispatch Path B as a clean composition over existing kits. Cleanest
fit with the prereq pattern (A1-A6 + Path A pattern). Adds one
dispatch round-trip; the Trinity exhibit ships unchanged from the
brief once A7 lands.

### (d) Tighten Path B's algebra around per-NamedTerm lowering

Lower each `NamedTerm` inside the bind payload individually rather than
the whole document; compare federation byte-identity on a per-NamedTerm
basis instead of on the whole bind-result wrapper.

Changes the empirical claim from "whole concept-tier IR is byte-identical
across the cycle" to "each NamedTerm's concept-tier projection is
byte-identical". Weaker but possibly the right factoring; this is an
editorial call about what #1024's federation claim is actually saying.

## What did empirically prove out

- **Real Rust lift via `provekit-walk-rpc`** produces canonical bind-IR.
- **BindKit is invocation-deterministic** on real input. The determinism
  sentinel (the new pre-assertion-4 check architected in the
  2026-05-16 ruling) passed: two distinct `BindKit::transform` calls on
  the same lifted Term produced byte-identical post-bind CIDs.
- **The `KitRegistry` + `execute_path` shape composes the first two
  steps cleanly.** No shell-out, one `execute_path` call, no
  `cmd_*::run()`.

These results survive whichever option the architect picks. The
architecture from A1-A6 plus Path A holds; the blocker is one
integration point in `claim_spec_value`.

## Branch state

- HEAD before this commit: `917fa6a6a` (Path B architect ruling locked).
- No code changes in this PR; only this plans document.
- Six prereqs (#1064, #1066, #1065, #1061, #1062, #1063) and Path A
  (#1067) all remain landed on main and unchanged.

When the architect rules:

- (a) -> dispatch a small substrate fix PR, then re-dispatch Path B.
- (b) -> dispatch a new-kit prereq, then re-dispatch with the eight-step
  algebra.
- (c) -> dispatch A7 as a one-line bug-surface PR; Path B then composes
  the brief as written.
- (d) -> re-dispatch Path B with a tightened federation claim and an
  algebra that pushes the lower step inside a per-NamedTerm loop.
