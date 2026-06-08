# libsugar Rust Surface Audit v2

## Summary

Audit scope was the fixed D1 surface inventory: `implementations/rust/libsugar/src` plus direct sibling crates `sugar-canonicalizer`, `sugar-proof-envelope`, and `sugar-ir-types`. Function rows were re-run through the post-D5 `sugar-walk-emit term` path. Non-function rows reflect the post-D5 type-declaration surface, where the current mementos carry the item without a typed refusal.

Total items audited: 1109

- handles-fully: 497
- handles-partially-with-loss-record: 261
- refuses-with-typed-reason: 351

## Per-crate breakdown

### libsugar

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 248 | 199 | 244 |

### sugar-canonicalizer

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 10 | 11 | 17 |

### sugar-ir-types

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 226 | 45 | 63 |

### sugar-proof-envelope

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 13 | 6 | 27 |

## Gap classes (grouped by refusal reason)

### unsupported-literal (160 items)


### unsupported-value-closure (46 items)

- `libsugar::canonical::serializable_jcs`
- `libsugar::compose::blocking_effects_for_steps`
- `libsugar::compose::build_value`
- `libsugar::compose::compose_chain_contracts`
- `libsugar::compose::compose_function_contracts_checked`

### unsupported-stmt-method-call (34 items)


### unsupported-let-pattern (27 items)

- `libsugar::canonical::is_blake3_512_cid`
- `libsugar::lift_plugin::impl LiftPluginKit::parse_session`
- `libsugar::primitives::dropper`
- `libsugar::primitives::verify_sig`
- `libsugar::types::impl Cid::parse`

### unsupported-stmt-call (25 items)

- `libsugar::desugar::impl DesugaringSet::non_core_ops`

### block-without-tail (17 items)

- `libsugar::canonical::json_to_cvalue`
- `libsugar::compose::find_namespaced_result`
- `libsugar::compose::find_result_equation`
- `libsugar::stubs::impl Domain for FunctionContractDomain::discharge`
- `libsugar::stubs::term_formals`

### unsupported-value-if (13 items)

- `libsugar::compose::compose_chain_contracts_internal`
- `libsugar::compose::impl EffectSet::check_opacity_effects`
- `libsugar::compose::impl FunctionContractMemento::check_aliasing_discharged`
- `libsugar::types::impl Path::ordered_steps`
- `libsugar::types::json_to_cvalue`

### function-not-found (11 items)

- `libsugar::traits::Canonical::canonical_bytes`
- `libsugar::traits::Domain::discharge`
- `libsugar::traits::Domain::name`
- `libsugar::traits::Domain::project`
- `libsugar::traits::Kit::dialect`

### unsupported-value-return (4 items)

- `libsugar::lift_plugin::impl LiftPluginKit::claim_from_response_term`
- `libsugar::ffi::impl AliasingMementoDto::into_memento`
- `sugar-proof-envelope::sign::ed25519_verify_bytes`
- `sugar-proof-envelope::sign::ed25519_verify_string`

### unsupported-return-type (4 items)

- `libsugar::desugar::impl DesugaringSet::rules`
- `libsugar::witness_registry::impl WitnessRegistry::get`
- `libsugar::wp::aggregate_conjunction`
- `libsugar::wp::handle_formula_binder`

### unsupported-value-cast (2 items)

- `libsugar::compose::promote_fcm_to_compound`
- `sugar-proof-envelope::cbor::cbor_append_head`

### unsupported-value-for-loop (1 items)

- `libsugar::desugar::certify_confluence`

### unsupported-value-range (1 items)

- `libsugar::desugar::desugar`

### unsupported-stmt-while (1 items)

- `libsugar::desugar::graph_reaches`

### unsupported-boolean-if (1 items)

- `libsugar::desugar::has_cycle`

### unsupported-value-unsafe (1 items)

- `libsugar::ffi::read_jcs_input`

### unsupported-stmt-binary (1 items)

- `libsugar::promotion_decision_registry::impl PromotionDecisionRegistry::admit_many`

### unsupported-value-loop (1 items)

- `libsugar::wp::fresh_name`

### residual-term-emitter (1 items)

- `sugar-ir-types::src::require_schema`

## Partial-handle classes (grouped by loss-record dimension)

### procedural-macro (249 items)


### ffi-call-unresolved-effect (204 items)

- `libsugar::canonical::json_cid`
- `libsugar::canonical::json_jcs`
- `libsugar::canonical::serializable_cid`

### return-type-user-defined (131 items)

- `libsugar::compose::build_memento_value`
- `libsugar::compose::cid_of_value`

### trait-path-truncated (106 items)

- `libsugar::compose::domain_claim_fcm_tests::bare_fcm_error_is_deterministic`

### return-type-result (65 items)

- `libsugar::canonical::json_cid`
- `libsugar::canonical::json_jcs`
- `libsugar::canonical::serializable_cid`

### Expr::Macro (25 items)

- `libsugar::compose::effect_args_json`

### vec-macro-desugared-to-array (17 items)

- `libsugar::compose::fcm_auto_promote_tests::trivial_formula`
- `libsugar::compose::impl EffectSet::empty`
- `libsugar::primitives::composed_to_contract`
- `libsugar::types::memento_from_parts`
- `libsugar::desugar::refusal_from_error`

### return-type-option (16 items)

- `libsugar::compose::OpacityMementoLookup::lookup_pin_invariant`
- `libsugar::compose::impl OpacityMementoLookup for EmptyOpacityPool::lookup_pin_invariant`
- `libsugar::primitives::resolve`
- `libsugar::traits::Catalog::get`
- `libsugar::traits::InputCatalog::get_input`

### return-type-byte-vec (13 items)

- `libsugar::compose::jcs_bytes_of_value`
- `libsugar::types::impl Canonical for & str::canonical_bytes`
- `libsugar::types::impl Canonical for Cid::canonical_bytes`
- `libsugar::types::impl Canonical for DomainClaim::canonical_bytes`
- `libsugar::types::impl Canonical for FunctionContractMemento::canonical_bytes`

### impl-associated-type-not-lowered (12 items)

- `libsugar::compose::impl TryFrom<&FunctionContractMemento> for sugar_ir_types :: DomainClaim::try_from`
- `libsugar::types::impl TryFrom<&str> for Cid::try_from`
- `libsugar::types::impl TryFrom<DomainClaim> for Refutation::try_from`
- `libsugar::types::impl TryFrom<DomainClaim> for Truth::try_from`
- `libsugar::types::impl TryFrom<String> for Cid::try_from`

### type-inference-assumed-bool (11 items)

- `libsugar::compose::impl Locus::is_unknown`
- `libsugar::desugar::match_lhs`
- `libsugar::ffi::pk_composition_result_body_jcs`

### type-inference-assumed-int (10 items)

- `libsugar::compose::impl Locus::is_unknown`
- `libsugar::types::slot_evaluation_is_default`
- `libsugar::types::slot_sort_is_default`

### statement-macro (10 items)

- `libsugar::compose::domain_claim_fcm_tests::bare_fcm_error_is_deterministic`
- `libsugar::compose::domain_claim_fcm_tests::bare_fcm_returns_unbound_contract_error`
- `libsugar::compose::domain_claim_fcm_tests::unbound_contract_display_is_informative`
- `libsugar::compose::domain_claim_fcm_tests::unbound_contract_error_variant_matches`
- `libsugar::types::impl Cid::from_hash_output`

### Expr::Let (7 items)

- `sugar-ir-types::src::impl TryFrom<&ConceptSiteMemento> for DomainClaim::try_from`
- `sugar-ir-types::src::impl TryFrom<NamespacedExtensionPolicyMementoWire> for NamespacedExtensionPolicyMemento::try_from`
- `sugar-ir-types::src::impl TryFrom<String> for CanonicalizationProfileKind::try_from`
- `sugar-ir-types::src::impl TryFrom<String> for CatalogKind::try_from`
- `sugar-ir-types::src::impl TryFrom<String> for OccurrenceKind::try_from`

### abi-attribute-not-carried (5 items)

- `libsugar::ffi::pk_compose_chain_contracts`
- `libsugar::ffi::pk_composition_result_body_jcs`
- `libsugar::ffi::pk_composition_result_cid`
- `libsugar::ffi::pk_composition_result_error`
- `libsugar::ffi::pk_composition_result_free`

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
- Direct dependency crates are included only because `libsugar` composes them through its manifest. Other workspace consumers remain outside this surface pass.
- Build scripts, benches, external `tests/`, and third-party dependency sources remain excluded.
- `sugar-walk-emit term` accepts a simple function name, so same-file duplicate method names are constrained by that existing CLI dispatch surface.
