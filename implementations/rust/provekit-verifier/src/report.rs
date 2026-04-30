// SPDX-License-Identifier: Apache-2.0
//
// Stage 7: report. Aggregate per-callsite verdicts plus load-error
// rows. Mirrors .../verifier/report.cpp.

use crate::types::{CallSite, LoadError, ObligationVerdict, Report, ReportRow};

pub fn add_callsite(
    cs: &CallSite,
    verdict: ObligationVerdict,
    reason: &str,
    r: &mut Report,
) {
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

pub fn add_load_errors(errs: &[LoadError], r: &mut Report) {
    r.load_errors = errs.to_vec();
}
