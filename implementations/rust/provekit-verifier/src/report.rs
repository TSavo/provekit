// SPDX-License-Identifier: Apache-2.0
//
// Stage 7: report. Aggregate per-callsite verdicts plus load-error
// rows. Mirrors .../verifier/report.cpp.

use crate::types::{CallSite, LoadError, ObligationVerdict, Report, ReportRow};

pub fn add_callsite(cs: &CallSite, verdict: ObligationVerdict, reason: &str, r: &mut Report) {
    r.total_callsites += 1;
    r.rows.push(ReportRow {
        callsite: cs.clone(),
        status: verdict.as_str().to_string(),
        reason: reason.to_string(),
    });
    if verdict == ObligationVerdict::Discharged {
        r.discharged += 1;
    } else {
        r.violations += 1;
    }
}

/// Add a contract self-post verification row. A self-post obligation
/// (`post[result := body]`) has no real callsite, so we synthesize a
/// minimal `CallSite` whose `property_name`/`property_cid` carry the
/// contract CID. Counted into the same discharged/violations totals so
/// the receipt's headline reflects self-posts alongside callsites.
pub fn add_self_post(contract_cid: &str, verdict: ObligationVerdict, reason: &str, r: &mut Report) {
    r.total_callsites += 1;
    let cs = CallSite {
        property_name: format!("self-post:{contract_cid}"),
        property_cid: contract_cid.to_string(),
        ..CallSite::default()
    };
    r.rows.push(ReportRow {
        callsite: cs,
        status: verdict.as_str().to_string(),
        reason: reason.to_string(),
    });
    if verdict == ObligationVerdict::Discharged {
        r.discharged += 1;
    } else {
        r.violations += 1;
    }
}

pub fn add_load_errors(errs: &[LoadError], r: &mut Report) {
    r.load_errors = errs.to_vec();
}
