# libprovekit Rust Surface Audit

## Summary

Audit scope was `implementations/rust/libprovekit/src` plus the direct sibling crates named in `libprovekit/Cargo.toml`: `provekit-canonicalizer`, `provekit-proof-envelope`, and `provekit-ir-types`. The CLI `provekit lift` in this branch dispatches configured plugins and does not expose a `--language rust` flag, so classification used the current direct Rust lifter surface in `provekit-walk` (`rust_function_term_json`, `type_decl`, and bind-lift behavior).

Total items audited: 1109

- handles-fully: 217
- handles-partially-with-loss-record: 309
- refuses-with-typed-reason: 583

## Per-crate breakdown

### libprovekit

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 150 | 127 | 414 |

### provekit-canonicalizer

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 7 | 3 | 28 |

### provekit-ir-types

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 51 | 175 | 108 |

### provekit-proof-envelope

| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |
| ---: | ---: | ---: |
| 9 | 4 | 33 |

## Gap classes (grouped by refusal reason)

### unsupported-return-type (267 items)

- `libprovekit::canonical::json_to_cvalue`
- `libprovekit::ci::ci_job_result_name`
- `libprovekit::ci::cid_set`
- `libprovekit::ci::impl CIBlastRadius::cid`
- `libprovekit::ci::impl CIImpactBodyClaim::cid`

### let-binding (175 items)

- `libprovekit::canonical::is_blake3_512_cid`
- `libprovekit::canonical::json_cid`
- `libprovekit::canonical::json_jcs`
- `libprovekit::ci::check_ci_body`
- `libprovekit::ci::impl CIBlastRadiusInput::build`

### term-emitter-unsupported (64 items)

- `libprovekit::ci::admit_identical_reuse`
- `libprovekit::ci::impl CIBlastRadius::validate`
- `libprovekit::ci::impl CIImpactBodyClaim::validate`
- `libprovekit::ci::impl CIJobResultBodyClaim::validate`
- `libprovekit::ci::impl CIReuseBodyClaim::validate`

### ffi-call (44 items)

- `libprovekit::ci::insert_many`
- `libprovekit::ci::insert_required`
- `libprovekit::ci::require_cid_field`
- `libprovekit::ci::require_nonempty`
- `libprovekit::ci::require_one_of`

### complex-generic (25 items)

- `libprovekit::canonical::serializable_cid`
- `libprovekit::canonical::serializable_jcs`
- `libprovekit::core::primitives::address`
- `libprovekit::core::traits::impl HashMapCatalog::insert`
- `libprovekit::core::types::impl ArityShape::named`

### statement-macro (5 items)

- `libprovekit::compose::impl AliasingMemento::to_jcs_value`
- `libprovekit::core::types::impl Cid::from_hash_output`
- `provekit-canonicalizer::hash::tests::distinct_inputs_distinct_hashes`
- `provekit-canonicalizer::jcs::tests::empty_object_and_array`
- `provekit-proof-envelope::sign::tests::verify_rejects_malformed`

### nested-item (3 items)

- `provekit-ir-types::lib::migration_json_to_canonical`
- `provekit-ir-types::lib::proof_run_json_to_canonical`
- `provekit-ir-types::lib::serde_json_to_canonical_value`

## Partial-handle classes (grouped by loss-record dimension)

### procedural-macro (254 items)

- `libprovekit::ci::CIJobResult`
- `libprovekit::ci::CINondeterminismMode`
- `libprovekit::ci::CIReuseReason`
- `libprovekit::ci::CIBlastRadius`
- `libprovekit::ci::CIBlastRadiusInput`

### trait-path-truncated (33 items)

- `libprovekit::compose::impl std::fmt::Display for OpacityError`
- `libprovekit::core::types::impl fmt::Display for Cid`
- `libprovekit::desugar::impl fmt::Display for Refusal`
- `libprovekit::desugar::impl std::error::Error for Refusal`
- `provekit-ir-types::lib::impl std::error::Error for CanonicalizationExtensionError`

### impl-associated-type-not-lowered (12 items)

- `libprovekit::compose::impl TryFrom<&FunctionContractMemento> for provekit_ir_types :: DomainClaim::Error`
- `libprovekit::core::types::impl TryFrom<&str> for Cid::Error`
- `libprovekit::core::types::impl TryFrom<DomainClaim> for Refutation::Error`
- `libprovekit::core::types::impl TryFrom<DomainClaim> for Truth::Error`
- `libprovekit::core::types::impl TryFrom<String> for Cid::Error`

### abi-attribute-not-carried (5 items)

- `libprovekit::ffi::pk_compose_chain_contracts`
- `libprovekit::ffi::pk_composition_result_body_jcs`
- `libprovekit::ffi::pk_composition_result_cid`
- `libprovekit::ffi::pk_composition_result_error`
- `libprovekit::ffi::pk_composition_result_free`

### impl-generics-not-carried (2 items)

- `libprovekit::core::types::impl Deserialize<'de> for Cid`
- `provekit-ir-types::lib::impl Deserialize<'de> for PolicyMemento`

### complex-generic (1 items)

- `libprovekit::core::traits::Domain::discharge`

### impl-associated-const-not-lowered (1 items)

- `provekit-ir-types::lib::impl DomainClaim::KIND`

### type-sort-opaque (1 items)

- `libprovekit::core::traits::DischargeMode`

## Recommended D2 sub-issues

- fix `unsupported-return-type` (267 items): extend return sort support beyond Unit, Bool, and integer terms.
- fix `procedural-macro` (254 items): accept named loss for bootstrap unless the attribute changes semantic identity, then add attribute mementos.
- fix `let-binding` (175 items): extend the term emitter to lower let bindings into the existing `let` operation instead of refusing Stmt::Local.
- fix `term-emitter-unsupported` (64 items): extend the Rust lifter for this syntax class or file an explicit non-goal if it is not needed for bootstrap.
- fix `ffi-call` (44 items): extend the lifter with call/method call terms and route unresolved effects into typed call mementos.
- fix `trait-path-truncated` (33 items): record this as accepted named loss unless D2 needs the missing detail for soundness.
- fix `complex-generic` (25 items): extend type and impl mementos to carry generics and where predicates before treating them as full lifts.
- fix `impl-associated-type-not-lowered` (12 items): record this as accepted named loss unless D2 needs the missing detail for soundness.
- fix `abi-attribute-not-carried` (5 items): record this as accepted named loss unless D2 needs the missing detail for soundness.
- fix `statement-macro` (5 items): treat macro definitions and macro call bodies as explicit loss unless expanded HIR becomes available.
- fix `nested-item` (3 items): extend the Rust lifter for this syntax class or file an explicit non-goal if it is not needed for bootstrap.

## Out-of-scope and known-noisy

- `#[cfg(test)]` and unit-test helper items under audited `src/` files are included because they are Rust items in the source surface, but they should not drive bootstrap production leaf priority unless the same gap appears in production code.
- Direct dependency crates are included only because `libprovekit` composes them through its manifest. Other workspace consumers of `libprovekit`, such as `provekit-mint-amp`, are outside this D1 surface pass.
- Build scripts, benches, external `tests/`, and third-party dependency sources are excluded.
- `derive`, `serde`, `repr`, `no_mangle`, and `cfg_attr` entries are treated as current lifter loss because the type/function mementos do not expand or encode those attributes.
