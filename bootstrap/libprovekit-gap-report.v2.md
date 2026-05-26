# libprovekit Rust Surface Audit v2

## Summary

Audit scope was the fixed D1 surface inventory: `implementations/rust/libprovekit/src` plus direct sibling crates `provekit-canonicalizer`, `provekit-proof-envelope`, and `provekit-ir-types`. Function rows were re-run through the post-D5 `provekit-walk-emit term` path. Non-function rows reflect the post-D5 type-declaration surface, where the current mementos carry the item without a typed refusal.

Total items audited: 1109

- handles-fully: 497
- handles-partially-with-loss-record: 261
- refuses-with-typed-reason: 351

## Per-crate breakdown

### libprovekit

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 248 | 199 | 244 |

### provekit-canonicalizer

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 10 | 11 | 17 |

### provekit-ir-types

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 226 | 45 | 63 |

### provekit-proof-envelope

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 13 | 6 | 27 |

## Gap classes (grouped by refusal reason)

### unsupported-literal (160 items)


### unsupported-value-closure (46 items)

- `libprovekit::canonical::serializable_jcs`
- `libprovekit::compose::blocking_effects_for_steps`
- `libprovekit::compose::build_value`
- `libprovekit::compose::compose_chain_contracts`
- `libprovekit::compose::compose_function_contracts_checked`

### unsupported-stmt-method-call (34 items)


### unsupported-let-pattern (27 items)

- `libprovekit::canonical::is_blake3_512_cid`
- `libprovekit::lift_plugin::impl LiftPluginKit::parse_session`
- `libprovekit::primitives::dropper`
- `libprovekit::primitives::verify_sig`
- `libprovekit::types::impl Cid::parse`

### unsupported-stmt-call (25 items)

- `libprovekit::desugar::impl DesugaringSet::non_core_ops`

### block-without-tail (17 items)

- `libprovekit::canonical::json_to_cvalue`
- `libprovekit::compose::find_namespaced_result`
- `libprovekit::compose::find_result_equation`
- `libprovekit::stubs::impl Domain for FunctionContractDomain::discharge`
- `libprovekit::stubs::term_formals`

### unsupported-value-if (13 items)

- `libprovekit::compose::compose_chain_contracts_internal`
- `libprovekit::compose::impl EffectSet::check_opacity_effects`
- `libprovekit::compose::impl FunctionContractMemento::check_aliasing_discharged`
- `libprovekit::types::impl Path::ordered_steps`
- `libprovekit::types::json_to_cvalue`

### function-not-found (11 items)

- `libprovekit::traits::Canonical::canonical_bytes`
- `libprovekit::traits::Domain::discharge`
- `libprovekit::traits::Domain::name`
- `libprovekit::traits::Domain::project`
- `libprovekit::traits::Kit::dialect`

### unsupported-value-return (4 items)

- `libprovekit::lift_plugin::impl LiftPluginKit::claim_from_response_term`
- `libprovekit::ffi::impl AliasingMementoDto::into_memento`
- `provekit-proof-envelope::sign::ed25519_verify_bytes`
- `provekit-proof-envelope::sign::ed25519_verify_string`

### unsupported-return-type (4 items)

- `libprovekit::desugar::impl DesugaringSet::rules`
- `libprovekit::witness_registry::impl WitnessRegistry::get`
- `libprovekit::wp::aggregate_conjunction`
- `libprovekit::wp::handle_formula_binder`

### unsupported-value-cast (2 items)

- `libprovekit::compose::promote_fcm_to_compound`
- `provekit-proof-envelope::cbor::cbor_append_head`

### unsupported-value-for-loop (1 items)

- `libprovekit::desugar::certify_confluence`

### unsupported-value-range (1 items)

- `libprovekit::desugar::desugar`

### unsupported-stmt-while (1 items)

- `libprovekit::desugar::graph_reaches`

### unsupported-boolean-if (1 items)

- `libprovekit::desugar::has_cycle`

### unsupported-value-unsafe (1 items)

- `libprovekit::ffi::read_jcs_input`

### unsupported-stmt-binary (1 items)

- `libprovekit::promotion_decision_registry::impl PromotionDecisionRegistry::admit_many`

### unsupported-value-loop (1 items)

- `libprovekit::wp::fresh_name`

### residual-term-emitter (1 items)

- `provekit-ir-types::src::require_schema`

## Partial-handle classes (grouped by loss-record dimension)

### procedural-macro (249 items)


### ffi-call-unresolved-effect (204 items)

- `libprovekit::canonical::json_cid`
- `libprovekit::canonical::json_jcs`
- `libprovekit::canonical::serializable_cid`

### return-type-user-defined (131 items)

- `libprovekit::compose::build_memento_value`
- `libprovekit::compose::cid_of_value`

### trait-path-truncated (106 items)

- `libprovekit::compose::domain_claim_fcm_tests::bare_fcm_error_is_deterministic`

### return-type-result (65 items)

- `libprovekit::canonical::json_cid`
- `libprovekit::canonical::json_jcs`
- `libprovekit::canonical::serializable_cid`

### Expr::Macro (25 items)

- `libprovekit::compose::effect_args_json`

### vec-macro-desugared-to-array (17 items)

- `libprovekit::compose::fcm_auto_promote_tests::trivial_formula`
- `libprovekit::compose::impl EffectSet::empty`
- `libprovekit::primitives::composed_to_contract`
- `libprovekit::types::memento_from_parts`
- `libprovekit::desugar::refusal_from_error`

### return-type-option (16 items)

- `libprovekit::compose::OpacityMementoLookup::lookup_pin_invariant`
- `libprovekit::compose::impl OpacityMementoLookup for EmptyOpacityPool::lookup_pin_invariant`
- `libprovekit::primitives::resolve`
- `libprovekit::traits::Catalog::get`
- `libprovekit::traits::InputCatalog::get_input`

### return-type-byte-vec (13 items)

- `libprovekit::compose::jcs_bytes_of_value`
- `libprovekit::types::impl Canonical for & str::canonical_bytes`
- `libprovekit::types::impl Canonical for Cid::canonical_bytes`
- `libprovekit::types::impl Canonical for DomainClaim::canonical_bytes`
- `libprovekit::types::impl Canonical for FunctionContractMemento::canonical_bytes`

### impl-associated-type-not-lowered (12 items)

- `libprovekit::compose::impl TryFrom<&FunctionContractMemento> for provekit_ir_types :: DomainClaim::try_from`
- `libprovekit::types::impl TryFrom<&str> for Cid::try_from`
- `libprovekit::types::impl TryFrom<DomainClaim> for Refutation::try_from`
- `libprovekit::types::impl TryFrom<DomainClaim> for Truth::try_from`
- `libprovekit::types::impl TryFrom<String> for Cid::try_from`

### type-inference-assumed-bool (11 items)

- `libprovekit::compose::impl Locus::is_unknown`
- `libprovekit::desugar::match_lhs`
- `libprovekit::ffi::pk_composition_result_body_jcs`

### type-inference-assumed-int (10 items)

- `libprovekit::compose::impl Locus::is_unknown`
- `libprovekit::types::slot_evaluation_is_default`
- `libprovekit::types::slot_sort_is_default`

### statement-macro (10 items)

- `libprovekit::compose::domain_claim_fcm_tests::bare_fcm_error_is_deterministic`
- `libprovekit::compose::domain_claim_fcm_tests::bare_fcm_returns_unbound_contract_error`
- `libprovekit::compose::domain_claim_fcm_tests::unbound_contract_display_is_informative`
- `libprovekit::compose::domain_claim_fcm_tests::unbound_contract_error_variant_matches`
- `libprovekit::types::impl Cid::from_hash_output`

### Expr::Let (7 items)

- `provekit-ir-types::src::impl TryFrom<&ConceptSiteMemento> for DomainClaim::try_from`
- `provekit-ir-types::src::impl TryFrom<NamespacedExtensionPolicyMementoWire> for NamespacedExtensionPolicyMemento::try_from`
- `provekit-ir-types::src::impl TryFrom<String> for CanonicalizationProfileKind::try_from`
- `provekit-ir-types::src::impl TryFrom<String> for CatalogKind::try_from`
- `provekit-ir-types::src::impl TryFrom<String> for OccurrenceKind::try_from`

### abi-attribute-not-carried (5 items)

- `libprovekit::ffi::pk_compose_chain_contracts`
- `libprovekit::ffi::pk_composition_result_body_jcs`
- `libprovekit::ffi::pk_composition_result_cid`
- `libprovekit::ffi::pk_composition_result_error`
- `libprovekit::ffi::pk_composition_result_free`

### return-type-vec (1 items)


## Recommended residual sub-issues

- triage `unsupported-literal` (160 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-closure` (46 items): residual post-D5 term-emitter surface class.
- triage `unsupported-stmt-method-call` (34 items): residual post-D5 term-emitter surface class.
- triage `unsupported-let-pattern` (27 items): residual post-D5 term-emitter surface class.
- triage `unsupported-stmt-call` (25 items): residual post-D5 term-emitter surface class.
- triage `block-without-tail` (17 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-if` (13 items): residual post-D5 term-emitter surface class.
- triage `function-not-found` (11 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-return` (4 items): residual post-D5 term-emitter surface class.
- triage `unsupported-return-type` (4 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-cast` (2 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-for-loop` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-range` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-stmt-while` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-boolean-if` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-unsafe` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-stmt-binary` (1 items): residual post-D5 term-emitter surface class.
- triage `unsupported-value-loop` (1 items): residual post-D5 term-emitter surface class.
- triage `residual-term-emitter` (1 items): residual post-D5 term-emitter surface class.

## Out-of-scope and known-noisy

- `#[cfg(test)]` and unit-test helper items under audited `src/` files remain included because they are Rust items in the fixed surface inventory.
- Direct dependency crates are included only because `libprovekit` composes them through its manifest. Other workspace consumers remain outside this surface pass.
- Build scripts, benches, external `tests/`, and third-party dependency sources remain excluded.
- `provekit-walk-emit term` accepts a simple function name, so same-file duplicate method names are constrained by that existing CLI dispatch surface.
