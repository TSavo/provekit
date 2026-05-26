# libprovekit Rust Surface Audit v3

## Summary

Audit scope was the fixed D1 surface inventory: `implementations/rust/libprovekit/src` plus direct sibling crates `provekit-canonicalizer`, `provekit-proof-envelope`, and `provekit-ir-types`. Function rows were re-run through the post-961 `provekit-walk-emit term` path. Non-function rows reflect the post-961 type-declaration surface, where the current mementos carry the item without a typed refusal.

Total items audited: 1109

- handles-fully: 507
- handles-partially-with-loss-record: 324
- refuses-with-typed-reason: 278

## Per-crate breakdown

### libprovekit

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 258 | 231 | 202 |

### provekit-canonicalizer

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 10 | 14 | 14 |

### provekit-ir-types

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 226 | 63 | 45 |

### provekit-proof-envelope

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 13 | 16 | 17 |

## Gap classes (grouped by refusal reason)

### unsupported-literal (163 items)


### unsupported-value-if (32 items)

- `libprovekit::compose::compose_chain_contracts_internal`
- `libprovekit::compose::composition_error_to_refusal`
- `libprovekit::compose::impl EffectSet::check_opacity_effects`
- `libprovekit::compose::impl FunctionContractMemento::check_aliasing_discharged`

### block-without-tail (21 items)

- `libprovekit::canonical::json_to_cvalue`
- `libprovekit::compose::evidence_cid`
- `libprovekit::compose::find_namespaced_result`
- `libprovekit::compose::find_result_equation`
- `libprovekit::stubs::impl Domain for FunctionContractDomain::discharge`

### unsupported-let-pattern (19 items)

- `libprovekit::canonical::is_blake3_512_cid`
- `libprovekit::lift_plugin::impl LiftPluginKit::parse_session`
- `libprovekit::primitives::verify_sig`
- `libprovekit::types::impl Cid::parse`
- `libprovekit::verbs::link`

### function-not-found (12 items)

- `libprovekit::traits::Canonical::canonical_bytes`
- `libprovekit::traits::Domain::discharge`
- `libprovekit::traits::Domain::name`
- `libprovekit::traits::Domain::project`
- `libprovekit::traits::Kit::dialect`

### residual-term-emitter (11 items)

- `libprovekit::compose::blocking_effects_for_steps`
- `libprovekit::compose::effect_occurrences_for_steps`
- `libprovekit::compose::impl EffectSet::add`
- `libprovekit::primitives::compose`
- `libprovekit::primitives::compose_function_contract_claims`

### unsupported-value-cast (5 items)

- `libprovekit::compose::promote_fcm_to_compound`
- `provekit-proof-envelope::cbor::cbor_append_head`
- `provekit-proof-envelope::cbor::cbor_encode_bstr`
- `provekit-proof-envelope::cbor::cbor_encode_tstr`
- `provekit-proof-envelope::proof::emit_sorted_map`

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

## Partial-handle classes (grouped by loss-record dimension)

### ffi-call-unresolved-effect (277 items)

- `libprovekit::canonical::json_cid`
- `libprovekit::canonical::json_jcs`
- `libprovekit::canonical::serializable_cid`
- `libprovekit::canonical::serializable_jcs`

### return-type-user-defined (150 items)

- `libprovekit::compose::aliasing_mementos_to_value`
- `libprovekit::compose::build_memento_value`

### return-type-result (99 items)

- `libprovekit::canonical::json_cid`
- `libprovekit::canonical::json_jcs`
- `libprovekit::canonical::serializable_cid`
- `libprovekit::canonical::serializable_jcs`

### macro-not-expanded (58 items)

- `libprovekit::canonical::serializable_jcs`

### trait-path-truncated (42 items)

- `libprovekit::compose::domain_claim_fcm_tests::unbound_contract_display_is_informative`
- `libprovekit::compose::impl TryFrom<&FunctionContractMemento> for provekit_ir_types :: DomainClaim::try_from`
- `libprovekit::primitives::compose_verdict`

### let-binding-mutability (28 items)

- `libprovekit::compose::aliasing_mementos_to_value`
- `libprovekit::lift_plugin::impl Kit for LiftPluginKit::transform`
- `libprovekit::types::impl DomainClaim::unsigned`
- `libprovekit::types::impl PathDocument::from_path_and_inputs`
- `libprovekit::types::impl PathDocument::materialized_inputs`

### closure-captures-environment (23 items)

- `libprovekit::canonical::serializable_jcs`
- `libprovekit::compose::aliasing_mementos_to_value`
- `libprovekit::compose::compose_chain_contracts`
- `libprovekit::lift_plugin::impl Kit for LiftPluginKit::transform`
- `libprovekit::stubs::prove_stub_claim`

### type-inference-assumed-int (21 items)

- `libprovekit::compose::impl Locus::is_unknown`
- `libprovekit::types::impl Path::step`
- `libprovekit::types::impl PathDocument::materialized_inputs`

### return-type-option (21 items)

- `libprovekit::compose::OpacityMementoLookup::lookup_pin_invariant`
- `libprovekit::compose::impl OpacityMementoLookup for EmptyOpacityPool::lookup_pin_invariant`
- `libprovekit::primitives::resolve`
- `libprovekit::traits::Catalog::get`
- `libprovekit::traits::InputCatalog::get_input`

### type-inference-assumed-bool (17 items)

- `libprovekit::compose::impl Locus::is_unknown`

### return-type-byte-vec (14 items)

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

### Expr::Let (8 items)

- `libprovekit::desugar::impl EquationTerm::collect_op_names`
- `provekit-ir-types::src::impl TryFrom<&ConceptSiteMemento> for DomainClaim::try_from`
- `provekit-ir-types::src::impl TryFrom<NamespacedExtensionPolicyMementoWire> for NamespacedExtensionPolicyMemento::try_from`
- `provekit-ir-types::src::impl TryFrom<String> for CanonicalizationProfileKind::try_from`
- `provekit-ir-types::src::impl TryFrom<String> for CatalogKind::try_from`

### abi-attribute-not-carried (5 items)

- `libprovekit::ffi::pk_compose_chain_contracts`
- `libprovekit::ffi::pk_composition_result_body_jcs`
- `libprovekit::ffi::pk_composition_result_cid`
- `libprovekit::ffi::pk_composition_result_error`
- `libprovekit::ffi::pk_composition_result_free`

### return-type-vec (2 items)

- `libprovekit::promotion_decision_registry::payload_string_array`

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
- Direct dependency crates are included only because `libprovekit` composes them through its manifest. Other workspace consumers remain outside this surface pass.
- Build scripts, benches, external `tests/`, and third-party dependency sources remain excluded.
- `provekit-walk-emit term` accepts a simple function name, so same-file duplicate method names are constrained by that existing CLI dispatch surface.
