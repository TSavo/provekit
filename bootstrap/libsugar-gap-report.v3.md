# libsugar Rust Surface Audit v3

## Summary

Audit scope was the fixed D1 surface inventory: `implementations/rust/libsugar/src` plus direct sibling crates `sugar-canonicalizer`, `sugar-proof-envelope`, and `sugar-ir-types`. Function rows were re-run through the post-961 `sugar-walk-emit term` path. Non-function rows reflect the post-961 type-declaration surface, where the current mementos carry the item without a typed refusal.

Total items audited: 1109

- handles-fully: 507
- handles-partially-with-loss-record: 324
- refuses-with-typed-reason: 278

## Per-crate breakdown

### libsugar

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 258 | 231 | 202 |

### sugar-canonicalizer

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 10 | 14 | 14 |

### sugar-ir-types

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 226 | 63 | 45 |

### sugar-proof-envelope

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 13 | 16 | 17 |

## Gap classes (grouped by refusal reason)

### unsupported-literal (163 items)


### unsupported-value-if (32 items)

- `libsugar::compose::compose_chain_contracts_internal`
- `libsugar::compose::composition_error_to_refusal`
- `libsugar::compose::impl EffectSet::check_opacity_effects`
- `libsugar::compose::impl FunctionContractMemento::check_aliasing_discharged`

### block-without-tail (21 items)

- `libsugar::canonical::json_to_cvalue`
- `libsugar::compose::evidence_cid`
- `libsugar::compose::find_namespaced_result`
- `libsugar::compose::find_result_equation`
- `libsugar::stubs::impl Domain for FunctionContractDomain::discharge`

### unsupported-let-pattern (19 items)

- `libsugar::canonical::is_blake3_512_cid`
- `libsugar::lift_plugin::impl LiftPluginKit::parse_session`
- `libsugar::primitives::verify_sig`
- `libsugar::types::impl Cid::parse`
- `libsugar::verbs::link`

### function-not-found (12 items)

- `libsugar::traits::Canonical::canonical_bytes`
- `libsugar::traits::Domain::discharge`
- `libsugar::traits::Domain::name`
- `libsugar::traits::Domain::project`
- `libsugar::traits::Kit::dialect`

### residual-term-emitter (11 items)

- `libsugar::compose::blocking_effects_for_steps`
- `libsugar::compose::effect_occurrences_for_steps`
- `libsugar::compose::impl EffectSet::add`
- `libsugar::primitives::compose`
- `libsugar::primitives::compose_function_contract_claims`

### unsupported-value-cast (5 items)

- `libsugar::compose::promote_fcm_to_compound`
- `sugar-proof-envelope::cbor::cbor_append_head`
- `sugar-proof-envelope::cbor::cbor_encode_bstr`
- `sugar-proof-envelope::cbor::cbor_encode_tstr`
- `sugar-proof-envelope::proof::emit_sorted_map`

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

## Partial-handle classes (grouped by loss-record dimension)

### ffi-call-unresolved-effect (277 items)

- `libsugar::canonical::json_cid`
- `libsugar::canonical::json_jcs`
- `libsugar::canonical::serializable_cid`
- `libsugar::canonical::serializable_jcs`

### return-type-user-defined (150 items)

- `libsugar::compose::aliasing_mementos_to_value`
- `libsugar::compose::build_memento_value`

### return-type-result (99 items)

- `libsugar::canonical::json_cid`
- `libsugar::canonical::json_jcs`
- `libsugar::canonical::serializable_cid`
- `libsugar::canonical::serializable_jcs`

### macro-not-expanded (58 items)

- `libsugar::canonical::serializable_jcs`

### trait-path-truncated (42 items)

- `libsugar::compose::domain_claim_fcm_tests::unbound_contract_display_is_informative`
- `libsugar::compose::impl TryFrom<&FunctionContractMemento> for sugar_ir_types :: DomainClaim::try_from`
- `libsugar::primitives::compose_verdict`

### let-binding-mutability (28 items)

- `libsugar::compose::aliasing_mementos_to_value`
- `libsugar::lift_plugin::impl Kit for LiftPluginKit::transform`
- `libsugar::types::impl DomainClaim::unsigned`
- `libsugar::types::impl PathDocument::from_path_and_inputs`
- `libsugar::types::impl PathDocument::materialized_inputs`

### closure-captures-environment (23 items)

- `libsugar::canonical::serializable_jcs`
- `libsugar::compose::aliasing_mementos_to_value`
- `libsugar::compose::compose_chain_contracts`
- `libsugar::lift_plugin::impl Kit for LiftPluginKit::transform`
- `libsugar::stubs::prove_stub_claim`

### type-inference-assumed-int (21 items)

- `libsugar::compose::impl Locus::is_unknown`
- `libsugar::types::impl Path::step`
- `libsugar::types::impl PathDocument::materialized_inputs`

### return-type-option (21 items)

- `libsugar::compose::OpacityMementoLookup::lookup_pin_invariant`
- `libsugar::compose::impl OpacityMementoLookup for EmptyOpacityPool::lookup_pin_invariant`
- `libsugar::primitives::resolve`
- `libsugar::traits::Catalog::get`
- `libsugar::traits::InputCatalog::get_input`

### type-inference-assumed-bool (17 items)

- `libsugar::compose::impl Locus::is_unknown`

### return-type-byte-vec (14 items)

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

### Expr::Let (8 items)

- `libsugar::desugar::impl EquationTerm::collect_op_names`
- `sugar-ir-types::src::impl TryFrom<&ConceptSiteMemento> for DomainClaim::try_from`
- `sugar-ir-types::src::impl TryFrom<NamespacedExtensionPolicyMementoWire> for NamespacedExtensionPolicyMemento::try_from`
- `sugar-ir-types::src::impl TryFrom<String> for CanonicalizationProfileKind::try_from`
- `sugar-ir-types::src::impl TryFrom<String> for CatalogKind::try_from`

### abi-attribute-not-carried (5 items)

- `libsugar::ffi::pk_compose_chain_contracts`
- `libsugar::ffi::pk_composition_result_body_jcs`
- `libsugar::ffi::pk_composition_result_cid`
- `libsugar::ffi::pk_composition_result_error`
- `libsugar::ffi::pk_composition_result_free`

### return-type-vec (2 items)

- `libsugar::promotion_decision_registry::payload_string_array`

## Recommended residual sub-issues

- triage `unsupported-literal` (163 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-if` (32 items): residual post-961 term-emitter surface class.
- triage `block-without-tail` (21 items): residual post-961 term-emitter surface class.
- triage `unsupported-let-pattern` (19 items): residual post-961 term-emitter surface class.
- triage `function-not-found` (12 items): residual post-961 term-emitter surface class.
- triage `residual-term-emitter` (11 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-cast` (5 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-return` (4 items): residual post-961 term-emitter surface class.
- triage `unsupported-return-type` (4 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-for-loop` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-range` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-stmt-while` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-boolean-if` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-unsafe` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-stmt-binary` (1 items): residual post-961 term-emitter surface class.
- triage `unsupported-value-loop` (1 items): residual post-961 term-emitter surface class.

## Out-of-scope and known-noisy

- `#[cfg(test)]` and unit-test helper items under audited `src/` files remain included because they are Rust items in the fixed surface inventory.
- Direct dependency crates are included only because `libsugar` composes them through its manifest. Other workspace consumers remain outside this surface pass.
- Build scripts, benches, external `tests/`, and third-party dependency sources remain excluded.
- `sugar-walk-emit term` accepts a simple function name, so same-file duplicate method names are constrained by that existing CLI dispatch surface.
