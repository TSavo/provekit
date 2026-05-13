// SPDX-License-Identifier: Apache-2.0
//
// Round-trip serde tests for the DomainClaim wire form and its
// `Into<DomainClaim>` impl from `ConceptSiteMemento`.
//
// Source of truth:
//   protocol/specs/2026-05-13-domain-claim-normalization.md §1, §2
//
// These tests pin:
//   * DomainClaim deserializes from the wire shape the spec defines.
//   * Round-trip parity at the serde layer: parse -> serialize -> parse
//     yields the same value.
//   * All three verdict kinds round-trip with the correct optional-field
//     presence (discharge_receipt_cid / refusal_reason).
//   * The `From<&ConceptSiteMemento> for DomainClaim` mapping is faithful
//     to spec §2.1 across all three trichotomy verdicts.
//   * The trichotomy verdict is PRESERVED across the conversion: a
//     ConceptSiteMemento with verdict X produces a DomainClaim with
//     VerdictKind X for every X.
//
// Byte-exact CID pinning lives in provekit-claim-envelope (this crate has
// no JCS encoder).

use std::collections::BTreeMap;

use provekit_ir_types::{
    CodeSite, CodeSiteSpan, ConceptSiteMemento, ConceptSiteProvenance, Discharge, DomainClaim,
    DomainClaimConversionError, DomainClaimProvenance, IrFormula, LossRecord, VerdictBody,
    VerdictKind,
};

// 128 hex chars after the "blake3-512:" prefix; deterministic placeholders.
const KIT_CID: &str = "blake3-512:kit00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a";
const INPUT_CID: &str = "blake3-512:in1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111aa";
const TRUTH_CID: &str = "blake3-512:tr2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222ab";
const RECEIPT_CID: &str = "blake3-512:rc3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333ac";
const SIGNER: &str = "ed25519:VGVzdFNpZ25lcjAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==";
const SIGNATURE: &str = "ed25519:VGVzdFNpZ25hdHVyZTAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==";

// ConceptSiteMemento fixture CIDs (used in §2.1 mapping tests).
const FN_CID: &str = "blake3-512:fn00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const SRC_CID: &str = "blake3-512:src1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const CONCEPT_CID: &str = "blake3-512:concept22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";
const LOCAL_CID: &str = "blake3-512:local333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333";
const CS_RECEIPT_CID: &str = "blake3-512:rcpt4444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444";
const LIFTER_CID: &str = "blake3-512:lifter5555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555";
const CLUST_CID: &str = "blake3-512:clust666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
const DISCH_CID: &str = "blake3-512:disch7777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777";
const BINDING_CID: &str = "blake3-512:bind999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999";

// ================================================================
// DomainClaim wire shape -- "exact" verdict
// ================================================================

const EXACT_FIXTURE: &str = r#"{
  "input_cid":  "blake3-512:in1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111aa",
  "kind":       "domain-claim",
  "kit_cid":    "blake3-512:kit00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a",
  "provenance": {
    "declared_at": "2026-05-13T12:00:00Z",
    "signer":      "ed25519:VGVzdFNpZ25lcjAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA=="
  },
  "signature":  "ed25519:VGVzdFNpZ25hdHVyZTAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==",
  "truth_cid":  "blake3-512:tr2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222ab",
  "verdict":    {
    "discharge_receipt_cid": "blake3-512:rc3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333ac",
    "kind":                  "exact",
    "loss_record":           {}
  }
}"#;

#[test]
fn exact_wire_deserializes() {
    let c: DomainClaim = serde_json::from_str(EXACT_FIXTURE).expect("parse exact");
    assert_eq!(c.kind, "domain-claim");
    assert_eq!(c.kit_cid, KIT_CID);
    assert_eq!(c.input_cid, INPUT_CID);
    assert_eq!(c.truth_cid, TRUTH_CID);
    assert_eq!(c.verdict.kind, VerdictKind::Exact);
    assert_eq!(
        c.verdict.discharge_receipt_cid.as_deref(),
        Some(RECEIPT_CID)
    );
    assert!(c.verdict.refusal_reason.is_none());
    assert!(c.verdict.loss_record.0.is_empty());
    assert_eq!(c.provenance.signer, SIGNER);
    assert_eq!(c.signature, SIGNATURE);
}

#[test]
fn exact_wire_round_trips() {
    let c1: DomainClaim = serde_json::from_str(EXACT_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&c1).expect("serialize");
    let c2: DomainClaim = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(c1, c2);
}

// ================================================================
// DomainClaim wire shape -- "loudly-bounded-lossy" verdict
// ================================================================

const LOUDLY_LOSSY_FIXTURE: &str = r#"{
  "input_cid":  "blake3-512:in1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111aa",
  "kind":       "domain-claim",
  "kit_cid":    "blake3-512:kit00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a",
  "provenance": {
    "declared_at": "2026-05-13T12:00:00Z",
    "signer":      "ed25519:VGVzdFNpZ25lcjAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA=="
  },
  "signature":  "ed25519:VGVzdFNpZ25hdHVyZTAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==",
  "truth_cid":  "blake3-512:tr2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222ab",
  "verdict":    {
    "discharge_receipt_cid": "blake3-512:rc3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333ac",
    "kind":                  "loudly-bounded-lossy",
    "loss_record": {
      "domain_narrowing": {
        "kind": "atomic",
        "name": ">",
        "args": [
          {"kind": "var", "name": "x"},
          {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
        ]
      }
    }
  }
}"#;

#[test]
fn loudly_lossy_wire_deserializes() {
    let c: DomainClaim = serde_json::from_str(LOUDLY_LOSSY_FIXTURE).expect("parse lossy");
    assert_eq!(c.verdict.kind, VerdictKind::LoudlyBoundedLossy);
    assert_eq!(
        c.verdict.discharge_receipt_cid.as_deref(),
        Some(RECEIPT_CID)
    );
    assert!(c.verdict.refusal_reason.is_none());
    assert_eq!(c.verdict.loss_record.0.len(), 1);
    assert!(c.verdict.loss_record.0.contains_key("domain_narrowing"));
}

#[test]
fn loudly_lossy_wire_round_trips() {
    let c1: DomainClaim = serde_json::from_str(LOUDLY_LOSSY_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&c1).expect("serialize");
    let c2: DomainClaim = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(c1, c2);
}

// ================================================================
// DomainClaim wire shape -- "refuse" verdict
// ================================================================

const REFUSE_FIXTURE: &str = r#"{
  "input_cid":  "blake3-512:in1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111aa",
  "kind":       "domain-claim",
  "kit_cid":    "blake3-512:kit00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a",
  "provenance": {
    "declared_at": "2026-05-13T12:00:00Z",
    "signer":      "ed25519:VGVzdFNpZ25lcjAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA=="
  },
  "signature":  "ed25519:VGVzdFNpZ25hdHVyZTAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMA==",
  "truth_cid":  "blake3-512:tr2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222ab",
  "verdict":    {
    "kind":           "refuse",
    "loss_record":    {},
    "refusal_reason": "wp and witness disagreed at boundary case"
  }
}"#;

#[test]
fn refuse_wire_deserializes() {
    let c: DomainClaim = serde_json::from_str(REFUSE_FIXTURE).expect("parse refuse");
    assert_eq!(c.verdict.kind, VerdictKind::Refuse);
    assert!(c.verdict.discharge_receipt_cid.is_none());
    assert!(c.verdict.refusal_reason.is_some());
    assert_eq!(
        c.verdict.refusal_reason.as_deref().unwrap(),
        "wp and witness disagreed at boundary case"
    );
}

#[test]
fn refuse_wire_round_trips() {
    let c1: DomainClaim = serde_json::from_str(REFUSE_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&c1).expect("serialize");
    let c2: DomainClaim = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(c1, c2);
}

// ================================================================
// Optional-field absence: discharge_receipt_cid / refusal_reason are
// OMITTED when None on serialization.
// ================================================================

#[test]
fn optional_fields_absent_when_none_in_exact() {
    let c = make_claim(VerdictBody {
        discharge_receipt_cid: Some(RECEIPT_CID.to_string()),
        kind: VerdictKind::Exact,
        loss_record: LossRecord::default(),
        refusal_reason: None,
    });
    let serialized = serde_json::to_string(&c).expect("serialize");
    assert!(serialized.contains("\"discharge_receipt_cid\""));
    assert!(!serialized.contains("\"refusal_reason\""));
}

#[test]
fn optional_fields_absent_when_none_in_refuse() {
    let c = make_claim(VerdictBody {
        discharge_receipt_cid: None,
        kind: VerdictKind::Refuse,
        loss_record: LossRecord::default(),
        refusal_reason: Some("the discharger refused".to_string()),
    });
    let serialized = serde_json::to_string(&c).expect("serialize");
    assert!(!serialized.contains("\"discharge_receipt_cid\""));
    assert!(serialized.contains("\"refusal_reason\""));
}

// ================================================================
// VerdictKind serde labels match the wire-form strings used by
// ConceptSiteMemento.discharge.verdict. This is the byte-deterministic
// invariant that makes the From<&ConceptSiteMemento> impl faithful.
// ================================================================

#[test]
fn verdict_kind_exact_label() {
    let s = serde_json::to_string(&VerdictKind::Exact).expect("serialize");
    assert_eq!(s, "\"exact\"");
}

#[test]
fn verdict_kind_loudly_lossy_label() {
    let s = serde_json::to_string(&VerdictKind::LoudlyBoundedLossy).expect("serialize");
    assert_eq!(s, "\"loudly-bounded-lossy\"");
}

#[test]
fn verdict_kind_refuse_label() {
    let s = serde_json::to_string(&VerdictKind::Refuse).expect("serialize");
    assert_eq!(s, "\"refuse\"");
}

// ================================================================
// From<&ConceptSiteMemento> for DomainClaim -- spec §2.1
// ================================================================

fn make_concept_site(
    verdict: &str,
    discharge_receipt_cid: Option<&str>,
    refusal_reason: Option<&str>,
    loss_record: LossRecord,
) -> ConceptSiteMemento {
    ConceptSiteMemento {
        cid: BINDING_CID.to_string(),
        code_site: CodeSite {
            function_term_cid: FN_CID.to_string(),
            source_cid: SRC_CID.to_string(),
            span: CodeSiteSpan {
                end: 1303,
                start: 1248,
            },
        },
        concept_cid: CONCEPT_CID.to_string(),
        discharge: Discharge {
            method: "wp".to_string(),
            refusal_reason: refusal_reason.map(|s| s.to_string()),
            verdict: verdict.to_string(),
            discharge_receipt_cid: discharge_receipt_cid.map(|s| s.to_string()),
            loss_record,
        },
        kind: "concept-site".to_string(),
        local_contract_cid: LOCAL_CID.to_string(),
        provenance: ConceptSiteProvenance {
            clusterer_cid: CLUST_CID.to_string(),
            discharger_cid: DISCH_CID.to_string(),
            lifter_cid: LIFTER_CID.to_string(),
        },
        realization_mode_hint: None,
        schema_version: "1".to_string(),
        witnesses: vec![],
    }
}

#[test]
fn concept_site_exact_maps_to_domain_claim() {
    let cs = make_concept_site("exact", Some(CS_RECEIPT_CID), None, LossRecord::default());
    let claim = DomainClaim::try_from(&cs).expect("convert");
    // §2.1 mapping table:
    //   kit_cid   <- provenance.discharger_cid
    //   input_cid <- code_site.source_cid
    //   truth_cid <- concept_cid
    assert_eq!(claim.kit_cid, DISCH_CID);
    assert_eq!(claim.input_cid, SRC_CID);
    assert_eq!(claim.truth_cid, CONCEPT_CID);
    // Trichotomy preserved.
    assert_eq!(claim.verdict.kind, VerdictKind::Exact);
    assert_eq!(
        claim.verdict.discharge_receipt_cid.as_deref(),
        Some(CS_RECEIPT_CID)
    );
    assert!(claim.verdict.refusal_reason.is_none());
    assert!(claim.verdict.loss_record.0.is_empty());
    // Unsigned: signature empty, signer empty placeholder.
    assert_eq!(claim.signature, "");
    assert_eq!(claim.provenance.signer, "");
    // kind discriminator.
    assert_eq!(claim.kind, "domain-claim");
}

#[test]
fn concept_site_loudly_lossy_maps_to_domain_claim() {
    let mut loss = LossRecord::default();
    loss.0.insert(
        "ub_introduction".to_string(),
        IrFormula::Atomic {
            name: "true".to_string(),
            args: vec![],
        },
    );
    let cs = make_concept_site("loudly-bounded-lossy", Some(CS_RECEIPT_CID), None, loss);
    let claim = DomainClaim::try_from(&cs).expect("convert");
    assert_eq!(claim.verdict.kind, VerdictKind::LoudlyBoundedLossy);
    assert_eq!(
        claim.verdict.discharge_receipt_cid.as_deref(),
        Some(CS_RECEIPT_CID)
    );
    assert!(claim.verdict.refusal_reason.is_none());
    assert_eq!(claim.verdict.loss_record.0.len(), 1);
    assert!(claim.verdict.loss_record.0.contains_key("ub_introduction"));
}

#[test]
fn concept_site_refuse_maps_to_domain_claim() {
    let cs = make_concept_site(
        "refuse",
        None,
        Some("witness sample W contradicted wp claim"),
        LossRecord::default(),
    );
    let claim = DomainClaim::try_from(&cs).expect("convert");
    assert_eq!(claim.verdict.kind, VerdictKind::Refuse);
    assert!(claim.verdict.discharge_receipt_cid.is_none());
    assert_eq!(
        claim.verdict.refusal_reason.as_deref(),
        Some("witness sample W contradicted wp claim")
    );
}

#[test]
fn concept_site_bad_verdict_string_errors_explicitly() {
    let cs = make_concept_site("not-a-real-verdict", None, None, LossRecord::default());
    let err = DomainClaim::try_from(&cs).expect_err("must error");
    match err {
        DomainClaimConversionError::InvalidVerdictString(s) => {
            assert_eq!(s, "not-a-real-verdict")
        }
        other => panic!("expected InvalidVerdictString, got {other:?}"),
    }
}

#[test]
fn concept_site_to_domain_claim_round_trips_through_wire() {
    // The conversion produces a DomainClaim whose JCS bytes deserialize
    // back to an equal DomainClaim. This is the serde-level shape
    // invariant; byte-exact JCS canonicalization lives in
    // provekit-claim-envelope.
    let cs = make_concept_site("exact", Some(CS_RECEIPT_CID), None, LossRecord::default());
    let claim = DomainClaim::try_from(&cs).expect("convert");
    let serialized = serde_json::to_string(&claim).expect("serialize");
    let reparsed: DomainClaim = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(claim, reparsed);
}

// ================================================================
// Helpers
// ================================================================

fn make_claim(verdict: VerdictBody) -> DomainClaim {
    DomainClaim {
        input_cid: INPUT_CID.to_string(),
        kind: "domain-claim".to_string(),
        kit_cid: KIT_CID.to_string(),
        provenance: DomainClaimProvenance {
            declared_at: "2026-05-13T12:00:00Z".to_string(),
            signer: SIGNER.to_string(),
        },
        signature: SIGNATURE.to_string(),
        truth_cid: TRUTH_CID.to_string(),
        verdict,
    }
}

#[test]
fn unsigned_constructor_zeros_signature_and_provenance() {
    let verdict = VerdictBody {
        discharge_receipt_cid: Some(RECEIPT_CID.to_string()),
        kind: VerdictKind::Exact,
        loss_record: LossRecord::default(),
        refusal_reason: None,
    };
    let prov = DomainClaimProvenance {
        declared_at: String::new(),
        signer: String::new(),
    };
    let c = DomainClaim::unsigned(
        KIT_CID.to_string(),
        INPUT_CID.to_string(),
        TRUTH_CID.to_string(),
        verdict,
        prov,
    );
    assert_eq!(c.signature, "");
    assert_eq!(c.provenance.signer, "");
    assert_eq!(c.kind, "domain-claim");
}

#[test]
fn empty_loss_record_serializes_as_empty_object() {
    let c = make_claim(VerdictBody {
        discharge_receipt_cid: Some(RECEIPT_CID.to_string()),
        kind: VerdictKind::Exact,
        loss_record: LossRecord(BTreeMap::new()),
        refusal_reason: None,
    });
    let s = serde_json::to_string(&c).expect("serialize");
    // The loss_record field must be present and equal to an empty object,
    // never omitted; this is the §1.2 invariant for `exact` claims.
    assert!(s.contains("\"loss_record\":{}"));
}
