// SPDX-License-Identifier: Apache-2.0

//! Phase H of the `#1334` epic: lift any [`SiteTransformKit`] into the
//! universal [`Kit`] trait so source-to-source transformations register in
//! [`KitRegistry`] and compose with `LiftKit` / `LowerKit` / `BindKit` /
//! `ProveKit` under [`execute_path`](super::path_executor::execute_path).
//!
//! The substrate's algebra composition engine was already in place before
//! Phase H: `KitRegistry` (path_executor.rs), the universal `Kit` trait
//! (traits.rs), the eight-verb pipeline. What was missing was the bridge for
//! the source-to-source kits shipped in Phases B-E (`MaterializeKit`,
//! `MigrateKit`, future per-language source transforms): those kits
//! implement [`SiteTransformKit`] (the per-site iteration contract that
//! emits `SiteOutcome` per carrier), not the universal `Kit` trait
//! (`Input -> DomainClaim`).
//!
//! [`SourceTransformAdapter`] is that bridge. Wrap any `SiteTransformKit`
//! into a `Kit`, register the adapter under a path-step selector, and the
//! adapter's `transform` will iterate every `provekit-concept:` carrier in
//! the input source, dispatch each one through the kit's `transform_site`,
//! and emit one `DomainClaim` whose payload is the rewritten source plus a
//! [`SourceTransformReceipt`] (the trichotomy audit from Phase E).
//!
//! Every substrate operation is a kit. Every kit is a path step. Every
//! transformation is a chain. The CLI is N projections over one engine.

use sugar_ir_types::{BridgeHeaderV14, BridgeTarget, Sort};
use serde_json::{json, Value};

use super::primitives::address;
use super::source_transform::{
    build_receipt, transform_source_text_collecting_refusals, SiteTransformKit,
    SourceTransformReceipt,
};
use super::traits::{Kit, KitError};
use super::types::{
    any_sort, formula_true, memento_from_parts, Cid, Contract, Dialect, DomainClaim, DomainKind,
    Input, Term, Verdict,
};

/// Wire-stable schema name advertised in the [`Term::Const`] payload's
/// `sort` field. Downstream consumers key off this string to recognize a
/// source-transform claim.
pub const SOURCE_TRANSFORM_PAYLOAD_SORT: &str = "SourceTransformResult";

/// Adapter that lifts any [`SiteTransformKit`] into the universal [`Kit`]
/// trait. `KitRegistry` accepts `Box<dyn Kit>`; this adapter is the bridge
/// that lets `MaterializeKit`, `MigrateKit`, and future source-to-source
/// kits register alongside `LiftKit`, `LowerKit`, `BindKit`, and `ProveKit`
/// under `execute_path`.
///
/// The wrapped kit owns the per-site decision logic; the adapter owns the
/// `Input -> DomainClaim` conversion. The `Input::Spec` payload shape is
///
/// ```json
/// { "source": "<source text>" }
/// ```
///
/// or, for callers that prefer to pass raw bytes through the path-document
/// catalog, `Input::Source { dialect, bytes }`. Either variant flows through
/// [`extract_source_from_input`]. The output `DomainClaim` carries the
/// rewritten source plus the per-run [`SourceTransformReceipt`] in the
/// `payload` field as a [`Term::Const`] whose `sort` is
/// [`SOURCE_TRANSFORM_PAYLOAD_SORT`].
pub struct SourceTransformAdapter<K: SiteTransformKit> {
    kit: K,
    /// Source language label this adapter exposes via [`Kit::dialect`].
    /// Distinct from the wrapped kit's `target_language()` (which names what
    /// the kit *emits*); for `MaterializeKit` they coincide (rust->rust),
    /// for `MigrateKit` they differ (the source library's host language is
    /// the same Rust source the kit emits, but the dialect label encodes
    /// the source->target arrow).
    source_lang: String,
    /// Optional source-library tag. `None` for materialize (N=1, no source
    /// binding to compare against); `Some` for migrate (N=2). Passed through
    /// to [`build_receipt`] for the `source_library` receipt field.
    source_library: Option<String>,
    /// Target-library tag the wrapped kit binds against. Passed through to
    /// [`build_receipt`] for the `target_library` receipt field.
    target_library: String,
}

impl<K: SiteTransformKit> SourceTransformAdapter<K> {
    /// Wrap a [`SiteTransformKit`] for universal-`Kit` dispatch under
    /// `execute_path`. `source_lang` names what the adapter accepts;
    /// `source_library` is `Some` only for cross-library kits like
    /// `MigrateKit`; `target_library` is the kit's binding target.
    pub fn new(
        kit: K,
        source_lang: impl Into<String>,
        source_library: Option<String>,
        target_library: impl Into<String>,
    ) -> Self {
        Self {
            kit,
            source_lang: source_lang.into(),
            source_library,
            target_library: target_library.into(),
        }
    }

    /// Borrow the wrapped kit. Tests and callers that need to inspect or
    /// configure the kit directly use this; the path-executor never does.
    pub fn kit(&self) -> &K {
        &self.kit
    }
}

impl<K: SiteTransformKit + 'static> Kit for SourceTransformAdapter<K> {
    fn dialect(&self) -> Dialect {
        Dialect::Other(format!(
            "source-transform:{}->{}",
            self.source_lang,
            self.kit.target_language()
        ))
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        let from_cid = address(input);
        let source = extract_source_from_input(input)?;
        let (rewritten, site_outcomes) =
            transform_source_text_collecting_refusals(&source, &self.kit)
                .map_err(KitError::Transformation)?;
        let receipt = build_receipt(
            &self.kit,
            &self.source_lang,
            self.source_library.as_deref(),
            &self.target_library,
            &site_outcomes,
        );
        Ok(claim_from_receipt(
            &self.kit,
            &self.source_lang,
            self.target_library.as_str(),
            rewritten,
            receipt,
            from_cid,
        ))
    }

    fn parse(&self, _input: &Input) -> Result<Term, KitError> {
        Err(KitError::NotSupported)
    }

    fn serialize(&self, _term: &Term) -> Result<Input, KitError> {
        Err(KitError::NotSupported)
    }
}

/// Decode the source-text payload from an [`Input`] variant. The adapter
/// accepts two shapes:
///
/// - `Input::Spec({"source": "<text>"})` for callers that drive the adapter
///   from a path-document catalog of spec inputs.
/// - `Input::Source { dialect: _, bytes }` for callers that pass raw bytes
///   directly. The `dialect` is ignored; the adapter's own [`Dialect`]
///   declaration is the canonical source of truth for what language the
///   wrapped kit accepts.
///
/// Any other [`Input`] variant returns [`KitError::UnsupportedInput`].
fn extract_source_from_input(input: &Input) -> Result<String, KitError> {
    match input {
        Input::Spec(value) => extract_source_from_spec(value),
        Input::Source { bytes, .. } => {
            String::from_utf8(bytes.clone()).map_err(|error| KitError::UnsupportedInput {
                dialect: Dialect::Other("source-transform".to_string()),
                message: format!("source-transform adapter requires UTF-8 source bytes: {error}"),
            })
        }
        other => Err(KitError::UnsupportedInput {
            dialect: Dialect::Other("source-transform".to_string()),
            message: format!(
                "source-transform adapter expects Input::Spec or Input::Source, got {:?}",
                std::mem::discriminant(other)
            ),
        }),
    }
}

fn extract_source_from_spec(value: &Value) -> Result<String, KitError> {
    let source =
        value
            .get("source")
            .and_then(Value::as_str)
            .ok_or_else(|| KitError::UnsupportedInput {
                dialect: Dialect::Other("source-transform".to_string()),
                message: "Input::Spec missing string field `source`".to_string(),
            })?;
    Ok(source.to_string())
}

/// Assemble a [`DomainClaim`] from a source-transform run.
///
/// Shape mirrors [`super::lower_plugin::claim_from_realized`]: payload is a
/// [`Term::Const`] of the structured result, `to` is the CID of that
/// payload term, `contract` is a minimal [`memento_from_parts`] entry with
/// trivial pre/post, `from` is the CID of the adapter's input, and
/// `domain` is `DomainKind::Other("source-transform")` so downstream
/// catalogs can filter by it.
fn claim_from_receipt<K: SiteTransformKit>(
    kit: &K,
    source_lang: &str,
    target_library: &str,
    rewritten: String,
    receipt: SourceTransformReceipt,
    from_cid: Cid,
) -> DomainClaim {
    let payload_value = json!({
        "rewritten": rewritten,
        "receipt": &receipt,
    });
    let payload = Term::Const {
        value: payload_value,
        sort: Sort::Primitive {
            name: SOURCE_TRANSFORM_PAYLOAD_SORT.to_string(),
        },
    };
    let to = address(&payload);
    let contract_name = format!(
        "source_transform:{source_lang}->{}:{target_library}",
        kit.target_language()
    );
    let contract = bridge_contract_from_receipt(
        &receipt,
        source_lang,
        &contract_name,
        kit.target_language(),
        target_library,
    )
    .unwrap_or_else(|| {
        memento_from_parts(
            contract_name,
            Vec::new(),
            Vec::new(),
            any_sort(),
            formula_true(),
            formula_true(),
            Some(to.to_string()),
        )
    });
    let mut artifacts = vec![to.clone()];
    if let Ok(contract_cid) = Cid::parse(contract.cid.clone()) {
        artifacts.push(contract_cid);
    }
    artifacts.extend(receipt.site_witnesses.iter().filter_map(|site| {
        site.contract_cid
            .as_deref()
            .and_then(|contract_cid| Cid::parse(contract_cid).ok())
    }));
    artifacts.sort();
    artifacts.dedup();
    DomainClaim {
        domain: DomainKind::Other("source-transform".to_string()),
        contract,
        artifacts,
        from: vec![from_cid],
        premises: Vec::new(),
        to,
        witness: None,
        payload: Some(payload),
        verdict: Verdict::Unresolved,
        attestation: None,
    }
}

fn bridge_contract_from_receipt(
    receipt: &SourceTransformReceipt,
    source_layer: &str,
    contract_name: &str,
    target_language: &str,
    target_library: &str,
) -> Option<Contract> {
    let site = receipt
        .site_witnesses
        .iter()
        .find(|site| site.contract_cid.is_some())?;
    let source_contract_cid = site.contract_cid.as_ref()?.clone();
    let header = BridgeHeaderV14 {
        schema_version: "1".to_string(),
        kind: "bridge".to_string(),
        name: format!(
            "{contract_name}:{}",
            if site.function_name.is_empty() {
                "boundary"
            } else {
                site.function_name.as_str()
            }
        ),
        source_symbol: site.function_name.clone(),
        source_layer: source_layer.to_string(),
        source_contract_cid: source_contract_cid.clone(),
        target: BridgeTarget::Contract {
            cid: source_contract_cid.clone(),
        },
    };
    let mut value = serde_json::to_value(&header).expect("bridge header serializes");
    if let Value::Object(map) = &mut value {
        map.insert(
            "targetContractCid".to_string(),
            Value::String(source_contract_cid.clone()),
        );
        map.insert(
            "targetLayer".to_string(),
            Value::String(target_library.to_string()),
        );
    }
    let canonical = crate::canonical::json_jcs(&value).expect("bridge header canonicalizes");
    let cid = crate::canonical::json_cid(&value).expect("bridge header CIDs");
    let mut contract = memento_from_parts(
        format!("{contract_name}:{target_language}:{target_library}"),
        Vec::new(),
        Vec::new(),
        any_sort(),
        formula_true(),
        formula_true(),
        None,
    );
    contract.canonical_bytes = canonical.into_bytes();
    contract.cid = cid;
    Some(contract)
}

/// Decode the structured payload from a source-transform claim. Companion
/// to [`SourceTransformAdapter`]'s emission path: given a `DomainClaim`
/// produced by the adapter, return the rewritten source and the
/// [`SourceTransformReceipt`]. Returns an error if the claim payload does
/// not match the adapter's emission shape (different sort name, different
/// `Term` variant, JSON decode failure).
pub fn decode_source_transform_payload(
    claim: &DomainClaim,
) -> Result<(String, SourceTransformReceipt), String> {
    let payload = claim
        .payload
        .as_ref()
        .ok_or_else(|| "source-transform claim missing payload".to_string())?;
    let Term::Const { value, sort } = payload else {
        return Err(format!(
            "source-transform claim payload must be Term::Const, found other variant: {payload:?}"
        ));
    };
    if let Sort::Primitive { name } = sort {
        if name != SOURCE_TRANSFORM_PAYLOAD_SORT {
            return Err(format!(
                "source-transform claim payload sort `{name}` does not match expected `{SOURCE_TRANSFORM_PAYLOAD_SORT}`"
            ));
        }
    } else {
        return Err("source-transform claim payload sort must be Sort::Primitive".to_string());
    }
    let rewritten = value
        .get("rewritten")
        .and_then(Value::as_str)
        .ok_or_else(|| "source-transform claim payload missing `rewritten` string".to_string())?
        .to_string();
    let receipt_value = value
        .get("receipt")
        .ok_or_else(|| "source-transform claim payload missing `receipt` object".to_string())?;
    let receipt: SourceTransformReceipt = serde_json::from_value(receipt_value.clone())
        .map_err(|error| format!("decode source-transform receipt: {error}"))?;
    Ok((rewritten, receipt))
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::core::path_executor::{execute_path, KitRegistry};
    use crate::core::source_transform::{CarrierComment, OutcomeKind, SiteOutcome};
    use crate::core::traits::HashMapInputCatalog;
    use crate::core::types::{ConformanceDeclaration, Path, PathAlgebra, Verb};

    /// Minimal `SiteTransformKit` for the adapter smoke test. Returns
    /// `Materialize` for every carrier it sees, with a fixed binding CID and
    /// the carrier's `function` name as the realized body's function name.
    struct MockKit {
        target_language: String,
        call_count: AtomicUsize,
        contract_cid: Option<String>,
    }

    impl MockKit {
        fn new() -> Self {
            Self {
                target_language: "rust".to_string(),
                call_count: AtomicUsize::new(0),
                contract_cid: None,
            }
        }

        fn with_contract_cid(contract_cid: impl Into<String>) -> Self {
            Self {
                target_language: "rust".to_string(),
                call_count: AtomicUsize::new(0),
                contract_cid: Some(contract_cid.into()),
            }
        }
    }

    impl SiteTransformKit for MockKit {
        fn target_language(&self) -> &str {
            self.target_language.as_str()
        }

        fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(SiteOutcome::Materialize {
                body: format!("fn {}() {{ /* mocked */ }}", carrier.function),
                binding_cid: "blake3:mock-binding".to_string(),
                contract_cid: self.contract_cid.clone(),
                loss_record: Value::Null,
            })
        }
    }

    fn one_carrier_source() -> &'static str {
        "// provekit-concept: {\"concept_name\":\"phase-h-test\",\"function\":\"phase_h_demo\"}\n"
    }

    #[test]
    fn adapter_transform_emits_receipt_for_one_carrier() {
        let kit = MockKit::new();
        let adapter = SourceTransformAdapter::new(kit, "rust", None, "rusqlite");
        let input = Input::Spec(json!({ "source": one_carrier_source() }));

        let claim = adapter
            .transform(&input)
            .expect("adapter transform succeeds on a one-carrier source");

        assert_eq!(adapter.kit().call_count.load(Ordering::SeqCst), 1);
        assert_eq!(claim.domain, DomainKind::Other("source-transform".into()));
        assert_eq!(claim.verdict, Verdict::Unresolved);

        let (rewritten, receipt) =
            decode_source_transform_payload(&claim).expect("payload decodes");
        assert!(
            rewritten.contains("fn phase_h_demo()"),
            "rewritten source should contain the mocked function body, got:\n{rewritten}"
        );
        assert_eq!(receipt.aggregate_summary.exact, 1);
        assert_eq!(receipt.aggregate_summary.lossy, 0);
        assert_eq!(receipt.aggregate_summary.refused, 0);
        assert_eq!(receipt.site_witnesses.len(), 1);
        assert_eq!(receipt.site_witnesses[0].outcome_kind, OutcomeKind::Exact);
        assert_eq!(receipt.source_language, "rust");
        assert_eq!(receipt.target_language, "rust");
        assert_eq!(receipt.target_library, "rusqlite");
        assert!(receipt.source_library.is_none());
    }

    #[test]
    fn adapter_transform_emits_bridge_for_materialized_vendor_contract() {
        let vendor_contract_cid = format!("blake3-512:{}", "a".repeat(128));
        let kit = MockKit::with_contract_cid(vendor_contract_cid.clone());
        let adapter = SourceTransformAdapter::new(kit, "rust", None, "vendor-lib");
        let input = Input::Spec(json!({ "source": one_carrier_source() }));

        let claim = adapter
            .transform(&input)
            .expect("adapter transform succeeds on a contract-bearing materialize site");
        let contract_value: Value = serde_json::from_slice(&claim.contract.canonical_bytes)
            .expect("source-transform contract slot carries bridge JSON");

        assert_eq!(contract_value["schemaVersion"], "1");
        assert_eq!(contract_value["kind"], "bridge");
        assert_eq!(contract_value["sourceSymbol"], "phase_h_demo");
        assert_eq!(
            contract_value["sourceContractCid"],
            Value::String(vendor_contract_cid.clone())
        );
        assert_eq!(
            contract_value["target"],
            json!({
                "kind": "contract",
                "cid": vendor_contract_cid,
            })
        );
        assert_eq!(
            contract_value["targetContractCid"],
            Value::String(vendor_contract_cid.clone())
        );
        assert!(
            contract_value.get("pre").is_none() && contract_value.get("post").is_none(),
            "contract-bearing materialize must emit a bridge, not mint a true->true memento: {contract_value}"
        );
    }

    #[test]
    fn adapter_registers_in_kit_registry_and_composes_under_execute_path() {
        let kit = MockKit::new();
        let adapter = SourceTransformAdapter::new(kit, "rust", None, "rusqlite");

        let source_input = Input::Spec(json!({ "source": one_carrier_source() }));
        let mut inputs = HashMapInputCatalog::default();
        let source_cid = inputs.insert(source_input);

        let mut registry = KitRegistry::default();
        registry.register(
            "source-transform",
            adapter,
            ConformanceDeclaration::NonCarrier {
                reason: "source-transform adapter emits DomainClaim with embedded receipt; \
                    source-emission is via the kit's binding pipeline rather than the path step",
            },
        );

        let path_input = Input::Path(Box::new(Path {
            algebra: vec![PathAlgebra {
                name: "transform".to_string(),
                kit: "source-transform".to_string(),
                inputs: vec![source_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            }],
        }));

        let chain = execute_path(&path_input, &registry, &inputs).expect("path executes");
        let claim = chain.terminal_claim();
        assert_eq!(claim.domain, DomainKind::Other("source-transform".into()));
        let (_rewritten, receipt) =
            decode_source_transform_payload(claim).expect("terminal claim payload decodes");
        assert_eq!(receipt.aggregate_summary.exact, 1);
        assert_eq!(receipt.site_witnesses.len(), 1);
    }
}
