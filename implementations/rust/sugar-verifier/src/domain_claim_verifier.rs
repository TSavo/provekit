// SPDX-License-Identifier: Apache-2.0
//
// DomainClaim verifier entry point.
//
// Implements the `k(I) = t` surface of the verifier: accepts a slice of
// `sugar_ir_types::DomainClaim` wire objects, validates the trichotomy
// invariants per spec §1.2, and aggregates results into a `ClaimReport`.
//
// Source of truth:
//   protocol/specs/2026-05-13-domain-claim-normalization.md §1.2, §4, §5
//
// This module does NOT run SMT or touch the MementoPool. The verdict is
// already on the wire-form claim; verification here is shape-validation +
// aggregation only. Signature verification and CID recomputation require
// the JCS encoder in `sugar-claim-envelope`; both are out of scope for
// this PR and tracked as follow-ups per spec §3.1.
//
// Trichotomy invariants (spec §1.2):
//
//   Exact:               discharge_receipt_cid REQUIRED,
//                        refusal_reason ABSENT,
//                        loss_record EMPTY.
//   LoudlyBoundedLossy:  discharge_receipt_cid REQUIRED,
//                        refusal_reason ABSENT,
//                        loss_record NON-EMPTY.
//   Refuse:              discharge_receipt_cid ABSENT,
//                        refusal_reason REQUIRED,
//                        loss_record MAY be empty or non-empty.

use sugar_ir_types::{DomainClaim, LossRecord, VerdictKind};

// ---------------------------------------------------------------------------
// Error type for trichotomy invariant violations
// ---------------------------------------------------------------------------

/// A trichotomy invariant violation on a single `DomainClaim`.
///
/// Source of truth: spec §1.2. Each variant corresponds to one of the
/// normative rows in the §1.2 consistency table. These are substrate
/// invariant violations: a well-formed claim from a well-formed source
/// memento should never trigger one of these. A trigger indicates a bug
/// in the source-memento validator or a tampered wire object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrichotomyError {
    /// `kind == "exact"` but `loss_record` is non-empty.
    ExactWithLoss {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "exact"` but `discharge_receipt_cid` is absent.
    ExactMissingReceipt {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "exact"` but `refusal_reason` is present (must be absent).
    ExactWithRefusalReason {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "loudly-bounded-lossy"` but `loss_record` is empty.
    LossyWithoutLoss {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "loudly-bounded-lossy"` but `discharge_receipt_cid` is absent.
    LossyMissingReceipt {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "loudly-bounded-lossy"` but `refusal_reason` is present (must be absent).
    LossyWithRefusalReason {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "refuse"` but `discharge_receipt_cid` is present (must be absent).
    RefuseWithReceipt {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// `kind == "refuse"` but `refusal_reason` is absent (required).
    RefuseMissingReason {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
    },
    /// Source memento could not be converted to a `DomainClaim`.
    ///
    /// Used exclusively by `#[deprecated]` shims that convert legacy mementos.
    /// The inner string is the `Display` form of the `DomainClaimConversionError`.
    SourceConversion(String),
}

impl std::fmt::Display for TrichotomyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExactWithLoss {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=exact but loss_record is \
                 non-empty (spec §1.2 requires loss_record empty for exact). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::ExactMissingReceipt {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=exact but \
                 discharge_receipt_cid is absent (spec §1.2 requires it present for exact). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::ExactWithRefusalReason {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=exact but \
                 refusal_reason is present (spec §1.2 requires it absent for exact). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::LossyWithoutLoss {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=loudly-bounded-lossy but \
                 loss_record is empty (spec §1.2 requires it non-empty for loudly-bounded-lossy). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::LossyMissingReceipt {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=loudly-bounded-lossy but \
                 discharge_receipt_cid is absent (spec §1.2 requires it present). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::LossyWithRefusalReason {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=loudly-bounded-lossy but \
                 refusal_reason is present (spec §1.2 requires it absent). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::RefuseWithReceipt {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=refuse but \
                 discharge_receipt_cid is present (spec §1.2 requires it absent for refuse). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::RefuseMissingReason {
                kit_cid,
                input_cid,
                truth_cid,
            } => write!(
                f,
                "trichotomy violation: k(I)=t claim has kind=refuse but \
                 refusal_reason is absent (spec §1.2 requires it present for refuse). \
                 k={kit_cid} I={input_cid} t={truth_cid}"
            ),
            Self::SourceConversion(msg) => write!(
                f,
                "source memento could not be converted to DomainClaim: {msg}"
            ),
        }
    }
}

impl std::error::Error for TrichotomyError {}

// ---------------------------------------------------------------------------
// Per-claim outcome
// ---------------------------------------------------------------------------

/// The outcome of verifying a single `DomainClaim` (shape-only, no SMT).
///
/// Matches the trichotomy buckets from spec §1 plus an `Invalid` bucket for
/// claims that fail the §1.2 consistency invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// `k(I) = t` holds exactly. `discharge_receipt_cid` is confirmed present.
    Exact {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
        discharge_receipt_cid: String,
    },
    /// `k(I) = t` holds modulo bounded loss. Loss dimensions confirmed non-empty.
    LoudlyBoundedLossy {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
        discharge_receipt_cid: String,
        loss_record: LossRecord,
    },
    /// `k(I)` could not be shown equal to `t`. Surfaces as a gap.
    Refuse {
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
        refusal_reason: String,
    },
    /// The claim fails §1.2 consistency invariants.
    Invalid(TrichotomyError),
}

// ---------------------------------------------------------------------------
// Aggregated report
// ---------------------------------------------------------------------------

/// Aggregated result of verifying a collection of `DomainClaim`s.
///
/// Source of truth: spec §4 (verifier consumption preview) and the task
/// spec for PR-C.
#[derive(Debug, Default, Clone)]
pub struct ClaimReport {
    /// Claims that held exactly: `loss_record` empty, `discharge_receipt_cid` present.
    /// Each entry is `(kit_cid, input_cid, truth_cid, discharge_receipt_cid)`.
    pub exact: Vec<(String, String, String, String)>,

    /// Claims that held modulo bounded loss.
    /// Each entry is `(kit_cid, input_cid, truth_cid, discharge_receipt_cid, loss_record)`.
    pub lossy: Vec<(String, String, String, String, LossRecord)>,

    /// Claims where `k(I)` could not be shown equal to `t`. Each is a gap.
    /// Each entry is `(kit_cid, input_cid, truth_cid, refusal_reason)`.
    pub gaps: Vec<(String, String, String, String)>,

    /// Claims that violated the §1.2 consistency invariants.
    pub invalid: Vec<TrichotomyError>,
}

impl ClaimReport {
    /// True iff all claims are either `Exact` or `LoudlyBoundedLossy` and none
    /// are `Invalid` or `Refuse`.
    pub fn all_discharged(&self) -> bool {
        self.gaps.is_empty() && self.invalid.is_empty()
    }

    /// Number of claims processed.
    pub fn total(&self) -> usize {
        self.exact.len() + self.lossy.len() + self.gaps.len() + self.invalid.len()
    }

    /// Number of claims that count toward "green": `Exact` + `LoudlyBoundedLossy`.
    pub fn green_count(&self) -> usize {
        self.exact.len() + self.lossy.len()
    }
}

// ---------------------------------------------------------------------------
// Trichotomy validator (spec §1.2)
// ---------------------------------------------------------------------------

/// Validate the §1.2 consistency invariants for a single `DomainClaim`.
///
/// Returns `Ok(())` when the claim is consistent. Returns `Err(TrichotomyError)`
/// on the FIRST invariant violation found (fail-fast: multiple violations may
/// exist, but one is sufficient to reject the claim).
pub fn validate_trichotomy(claim: &DomainClaim) -> Result<(), TrichotomyError> {
    let v = &claim.verdict;
    let k = claim.kit_cid.clone();
    let i = claim.input_cid.clone();
    let t = claim.truth_cid.clone();
    let loss_empty = v.loss_record.0.is_empty();

    match &v.kind {
        VerdictKind::Exact => {
            if v.discharge_receipt_cid.is_none() {
                return Err(TrichotomyError::ExactMissingReceipt {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
            if v.refusal_reason.is_some() {
                return Err(TrichotomyError::ExactWithRefusalReason {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
            if !loss_empty {
                return Err(TrichotomyError::ExactWithLoss {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
        }
        VerdictKind::LoudlyBoundedLossy => {
            if v.discharge_receipt_cid.is_none() {
                return Err(TrichotomyError::LossyMissingReceipt {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
            if v.refusal_reason.is_some() {
                return Err(TrichotomyError::LossyWithRefusalReason {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
            if loss_empty {
                return Err(TrichotomyError::LossyWithoutLoss {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
        }
        VerdictKind::Refuse => {
            if v.discharge_receipt_cid.is_some() {
                return Err(TrichotomyError::RefuseWithReceipt {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
            if v.refusal_reason.is_none() {
                return Err(TrichotomyError::RefuseMissingReason {
                    kit_cid: k,
                    input_cid: i,
                    truth_cid: t,
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-claim verifier
// ---------------------------------------------------------------------------

/// Verify a single `DomainClaim` wire object.
///
/// Validates trichotomy invariants (spec §1.2) and maps to a `ClaimOutcome`.
/// Shape-only: no SMT, no MementoPool, no signature check.
pub fn verify_claim(claim: &DomainClaim) -> ClaimOutcome {
    match validate_trichotomy(claim) {
        Err(e) => ClaimOutcome::Invalid(e),
        Ok(()) => match &claim.verdict.kind {
            VerdictKind::Exact => ClaimOutcome::Exact {
                kit_cid: claim.kit_cid.clone(),
                input_cid: claim.input_cid.clone(),
                truth_cid: claim.truth_cid.clone(),
                discharge_receipt_cid: claim
                    .verdict
                    .discharge_receipt_cid
                    .clone()
                    .expect("invariant: Exact has discharge_receipt_cid"),
            },
            VerdictKind::LoudlyBoundedLossy => ClaimOutcome::LoudlyBoundedLossy {
                kit_cid: claim.kit_cid.clone(),
                input_cid: claim.input_cid.clone(),
                truth_cid: claim.truth_cid.clone(),
                discharge_receipt_cid: claim
                    .verdict
                    .discharge_receipt_cid
                    .clone()
                    .expect("invariant: LoudlyBoundedLossy has discharge_receipt_cid"),
                loss_record: claim.verdict.loss_record.clone(),
            },
            VerdictKind::Refuse => ClaimOutcome::Refuse {
                kit_cid: claim.kit_cid.clone(),
                input_cid: claim.input_cid.clone(),
                truth_cid: claim.truth_cid.clone(),
                refusal_reason: claim
                    .verdict
                    .refusal_reason
                    .clone()
                    .expect("invariant: Refuse has refusal_reason"),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Batch verifier
// ---------------------------------------------------------------------------

/// Verify a collection of `DomainClaim` wire objects.
///
/// Each claim is validated independently. Results are aggregated into a
/// `ClaimReport` with four buckets: `exact`, `lossy`, `gaps`, `invalid`.
///
/// This is the primary entry point for the `k(I) = t` verifier surface.
/// No SMT, no MementoPool, no signature check.
pub fn verify_claims(claims: &[DomainClaim]) -> ClaimReport {
    let mut report = ClaimReport::default();
    for claim in claims {
        match verify_claim(claim) {
            ClaimOutcome::Exact {
                kit_cid,
                input_cid,
                truth_cid,
                discharge_receipt_cid,
            } => {
                report
                    .exact
                    .push((kit_cid, input_cid, truth_cid, discharge_receipt_cid));
            }
            ClaimOutcome::LoudlyBoundedLossy {
                kit_cid,
                input_cid,
                truth_cid,
                discharge_receipt_cid,
                loss_record,
            } => {
                report.lossy.push((
                    kit_cid,
                    input_cid,
                    truth_cid,
                    discharge_receipt_cid,
                    loss_record,
                ));
            }
            ClaimOutcome::Refuse {
                kit_cid,
                input_cid,
                truth_cid,
                refusal_reason,
            } => {
                report
                    .gaps
                    .push((kit_cid, input_cid, truth_cid, refusal_reason));
            }
            ClaimOutcome::Invalid(e) => {
                report.invalid.push(e);
            }
        }
    }
    report
}

// ---------------------------------------------------------------------------
// Deprecation shims (spec §5)
// ---------------------------------------------------------------------------
//
// These shims allow existing callers that build typed mementos to route
// through the unified `DomainClaim` path automatically. Each is marked
// `#[deprecated]` with a note pointing at the `verify_claims` entry point.
//
// The shim is a thin wrapper: convert the source memento to a `DomainClaim`
// via `TryFrom`, then delegate to `verify_claim`. A `TryFrom` error (e.g.
// `UnboundContract` for a bare FCM) becomes `ClaimOutcome::Invalid` with
// a synthetic `TrichotomyError`-shaped message surfaced as
// `Invalid(TrichotomyError::RefuseMissingReason)` -- but only if the
// conversion error is itself a substrate invariant violation.
//
// NOTE: the FCM shim intentionally returns `Invalid(SourceConversion)` rather
// than panicking on bare contracts; callers that pass bare FCMs need to be
// migrated to wrap them in a ConceptSiteMemento or CompoundContractMemento first.

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sugar_ir_types::{
        DomainClaim, DomainClaimProvenance, IrFormula, LossRecord, VerdictBody, VerdictKind,
    };

    use super::*;

    // -----------------------------------------------------------------------
    // Builder helpers
    // -----------------------------------------------------------------------

    fn provenance() -> DomainClaimProvenance {
        DomainClaimProvenance {
            declared_at: "2026-05-12T00:00:00Z".to_string(),
            signer: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        }
    }

    fn exact_verdict() -> VerdictBody {
        VerdictBody {
            kind: VerdictKind::Exact,
            loss_record: LossRecord(BTreeMap::new()),
            discharge_receipt_cid: Some(
                "blake3-512:aabb000000000000000000000000000000000000000000000000000000000000\
                 0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            ),
            refusal_reason: None,
        }
    }

    fn lossy_verdict() -> VerdictBody {
        let mut map = BTreeMap::new();
        map.insert(
            "domain_narrowing".to_string(),
            IrFormula::And { operands: vec![] },
        );
        VerdictBody {
            kind: VerdictKind::LoudlyBoundedLossy,
            loss_record: LossRecord(map),
            discharge_receipt_cid: Some(
                "blake3-512:ccdd000000000000000000000000000000000000000000000000000000000000\
                 0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            ),
            refusal_reason: None,
        }
    }

    fn refuse_verdict() -> VerdictBody {
        VerdictBody {
            kind: VerdictKind::Refuse,
            loss_record: LossRecord(BTreeMap::new()),
            discharge_receipt_cid: None,
            refusal_reason: Some("no tractable loss-record found".to_string()),
        }
    }

    fn make_claim(verdict: VerdictBody) -> DomainClaim {
        DomainClaim::unsigned(
            "blake3-512:kkkk000000000000000000000000000000000000000000000000000000000000\
             0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            "blake3-512:iiii000000000000000000000000000000000000000000000000000000000000\
             0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            "blake3-512:tttt000000000000000000000000000000000000000000000000000000000000\
             0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            verdict,
            provenance(),
        )
    }

    // -----------------------------------------------------------------------
    // Vec<DomainClaim> acceptance
    // -----------------------------------------------------------------------

    #[test]
    fn empty_slice_yields_empty_report() {
        let report = verify_claims(&[]);
        assert_eq!(report.total(), 0);
        assert!(report.all_discharged());
        assert_eq!(report.green_count(), 0);
    }

    #[test]
    fn single_exact_claim_accepted() {
        let claim = make_claim(exact_verdict());
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.exact.len(), 1, "one exact claim expected");
        assert_eq!(report.lossy.len(), 0);
        assert_eq!(report.gaps.len(), 0);
        assert_eq!(report.invalid.len(), 0);
        assert!(report.all_discharged());
        assert_eq!(report.green_count(), 1);
    }

    #[test]
    fn vec_of_domain_claims_all_three_trichotomy_kinds() {
        let claims = vec![
            make_claim(exact_verdict()),
            make_claim(lossy_verdict()),
            make_claim(refuse_verdict()),
        ];
        let report = verify_claims(&claims);
        assert_eq!(report.total(), 3);
        assert_eq!(report.exact.len(), 1);
        assert_eq!(report.lossy.len(), 1);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.invalid.len(), 0);
        assert!(!report.all_discharged(), "one gap means not all discharged");
        assert_eq!(report.green_count(), 2);
    }

    // -----------------------------------------------------------------------
    // Trichotomy invariant validation (spec §1.2)
    // -----------------------------------------------------------------------

    #[test]
    fn exact_with_loss_is_invalid() {
        let mut v = exact_verdict();
        let mut map = BTreeMap::new();
        map.insert(
            "domain_narrowing".to_string(),
            IrFormula::And { operands: vec![] },
        );
        v.loss_record = LossRecord(map);
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1, "exact+loss_record must be invalid");
        assert!(
            matches!(&report.invalid[0], TrichotomyError::ExactWithLoss { .. }),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn exact_missing_receipt_is_invalid() {
        let mut v = exact_verdict();
        v.discharge_receipt_cid = None;
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(
                &report.invalid[0],
                TrichotomyError::ExactMissingReceipt { .. }
            ),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn exact_with_refusal_reason_is_invalid() {
        let mut v = exact_verdict();
        v.refusal_reason = Some("should not be here".to_string());
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(
                &report.invalid[0],
                TrichotomyError::ExactWithRefusalReason { .. }
            ),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn lossy_without_loss_is_invalid() {
        let mut v = lossy_verdict();
        v.loss_record = LossRecord(BTreeMap::new());
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(&report.invalid[0], TrichotomyError::LossyWithoutLoss { .. }),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn lossy_missing_receipt_is_invalid() {
        let mut v = lossy_verdict();
        v.discharge_receipt_cid = None;
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(
                &report.invalid[0],
                TrichotomyError::LossyMissingReceipt { .. }
            ),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn refuse_with_receipt_is_invalid() {
        let mut v = refuse_verdict();
        v.discharge_receipt_cid = Some(
            "blake3-512:eeff000000000000000000000000000000000000000000000000000000000000\
             0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        );
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(
                &report.invalid[0],
                TrichotomyError::RefuseWithReceipt { .. }
            ),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    #[test]
    fn refuse_missing_reason_is_invalid() {
        let mut v = refuse_verdict();
        v.refusal_reason = None;
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.invalid.len(), 1);
        assert!(
            matches!(
                &report.invalid[0],
                TrichotomyError::RefuseMissingReason { .. }
            ),
            "wrong error variant: {:?}",
            report.invalid[0]
        );
    }

    // -----------------------------------------------------------------------
    // Verdict aggregation
    // -----------------------------------------------------------------------

    #[test]
    fn loss_records_aggregated_in_lossy_bucket() {
        let mut map = BTreeMap::new();
        map.insert(
            "domain_narrowing".to_string(),
            IrFormula::And { operands: vec![] },
        );
        map.insert(
            "ub_introduction".to_string(),
            IrFormula::And { operands: vec![] },
        );
        let mut v = lossy_verdict();
        v.loss_record = LossRecord(map.clone());
        let claim = make_claim(v);
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.lossy.len(), 1);
        let (_, _, _, _, lr) = &report.lossy[0];
        assert_eq!(
            lr.0.len(),
            2,
            "both loss dimensions should be in the record"
        );
        assert!(lr.0.contains_key("domain_narrowing"));
        assert!(lr.0.contains_key("ub_introduction"));
    }

    #[test]
    fn refuse_surfaces_as_gap_with_reason() {
        let claim = make_claim(refuse_verdict());
        let report = verify_claims(std::slice::from_ref(&claim));
        assert_eq!(report.gaps.len(), 1);
        let (k, i, t, reason) = &report.gaps[0];
        assert!(!k.is_empty());
        assert!(!i.is_empty());
        assert!(!t.is_empty());
        assert_eq!(reason, "no tractable loss-record found");
    }

    #[test]
    fn green_count_is_exact_plus_lossy() {
        let claims = vec![
            make_claim(exact_verdict()),
            make_claim(exact_verdict()),
            make_claim(lossy_verdict()),
            make_claim(refuse_verdict()),
        ];
        let report = verify_claims(&claims);
        assert_eq!(report.green_count(), 3, "2 exact + 1 lossy = 3 green");
        assert_eq!(report.gaps.len(), 1);
        assert!(!report.all_discharged());
    }

    #[test]
    fn all_discharged_true_when_no_gaps_or_invalid() {
        let claims = vec![make_claim(exact_verdict()), make_claim(lossy_verdict())];
        let report = verify_claims(&claims);
        assert!(report.all_discharged());
    }

    #[test]
    fn invalid_claim_sets_all_discharged_false() {
        let mut v = exact_verdict();
        v.discharge_receipt_cid = None; // trigger ExactMissingReceipt
        let claims = vec![make_claim(exact_verdict()), make_claim(v)];
        let report = verify_claims(&claims);
        assert!(!report.all_discharged());
        assert_eq!(report.invalid.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Backward-compat shim: ConceptSiteMemento
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Backward-compat shim: bare FCM (always-Err via PR-B TryFrom)
    // -----------------------------------------------------------------------

    #[test]
    fn bare_fcm_returns_unbound_contract_error() {
        use libsugar::compose::{EffectSet, FunctionContractMemento, Locus};
        use sugar_ir_types::Sort;
        use sugar_ir_types::{DomainClaimConversionError, IrFormula};

        let fcm = FunctionContractMemento {
            fn_name: "my_fn".to_string(),
            formals: vec![],
            formal_sorts: vec![],
            formal_regions: vec![],
            return_sort: Sort::Primitive {
                name: "bool".to_string(),
            },
            return_region: None,
            pre: IrFormula::And { operands: vec![] },
            post: IrFormula::And { operands: vec![] },
            body_cid: None,
            effects: EffectSet::empty(),
            locus: Locus {
                file: Some("test.rs".to_string()),
                line: 1,
                col: 0,
            },
            canonical_bytes: vec![],
            cid: "blake3-512:000000".to_string(),
            auto_minted_mementos: vec![],
            panic_loci: vec![],
            concept_hint: None,
        };

        // The PR-B TryFrom always returns Err(UnboundContract) for bare FCMs.
        let result = sugar_ir_types::DomainClaim::try_from(&fcm);
        assert_eq!(
            result,
            Err(DomainClaimConversionError::UnboundContract),
            "bare FCM must produce UnboundContract error"
        );
    }
}
