// SPDX-License-Identifier: Apache-2.0
//
// Pretty + JSON formatting for the verifier `Report`.

use owo_colors::OwoColorize;
use provekit_verifier::{LoadError, Report, ReportRow};
use serde_json::{json, Value as Json};

pub fn report_to_json(r: &Report) -> Json {
    let rows: Vec<Json> = r.rows.iter().map(row_to_json).collect();
    let load_errors: Vec<Json> = r.load_errors.iter().map(load_error_to_json).collect();
    let call_edges: Vec<Json> = r.call_edges.iter().map(|ce| {
        json!({
            "sourceContractCid": ce.source_contract_cid,
            "targetContractCid": ce.target_contract_cid,
            "file": ce.file,
        })
    }).collect();
    json!({
        "totalCallsites": r.total_callsites,
        "discharged": r.discharged,
        "violations": r.violations,
        "rows": rows,
        "loadErrors": load_errors,
        "callEdges": call_edges,
    })
}

fn row_to_json(row: &ReportRow) -> Json {
    json!({
        "bridge": row.callsite.bridge_ir_name,
        "targetCid": row.callsite.bridge_target_cid,
        "sourceLayer": row.callsite.bridge_source_layer,
        "targetLayer": row.callsite.bridge_target_layer,
        "property": row.callsite.property_name,
        "propertyCid": row.callsite.property_cid,
        "status": row.status,
        "reason": row.reason,
    })
}

fn load_error_to_json(e: &LoadError) -> Json {
    json!({
        "proofPath": e.proof_path,
        "reason": e.reason,
    })
}

pub fn print_report_pretty(r: &Report, quiet: bool) {
    if !quiet {
        println!("{}", "ProvekIt verifier report".bold());
        println!("  total callsites : {}", r.total_callsites);
        println!(
            "  discharged      : {}",
            r.discharged.to_string().green()
        );
        println!(
            "  violations      : {}",
            if r.violations == 0 {
                r.violations.to_string().green().to_string()
            } else {
                r.violations.to_string().red().to_string()
            }
        );
        println!(
            "  load errors     : {}",
            if r.load_errors.is_empty() {
                "0".green().to_string()
            } else {
                r.load_errors.len().to_string().red().to_string()
            }
        );
        println!();
        for row in &r.rows {
            let status_pretty = match row.status.as_str() {
                "discharged" => "discharged".green().to_string(),
                "unsatisfied" => "unsatisfied".red().to_string(),
                "undecidable" => "undecidable".yellow().to_string(),
                other => other.to_string(),
            };
            println!(
                "  [{}] {}  ({} -> {})",
                status_pretty,
                row.callsite.bridge_ir_name,
                row.callsite.bridge_source_layer,
                row.callsite.bridge_target_layer
            );
            if !row.reason.is_empty() {
                println!("      reason: {}", row.reason);
            }
        }
        if !r.load_errors.is_empty() {
            println!();
            println!("{}", "Load errors:".red().bold());
            for e in &r.load_errors {
                println!("  {}: {}", e.proof_path, e.reason);
            }
        }
        if !r.call_edges.is_empty() {
            println!();
            println!("{}", "Call edges:".dimmed());
            for ce in &r.call_edges {
                println!(
                    "  {} -> {}  ({})",
                    ce.source_contract_cid.chars().take(32).collect::<String>(),
                    ce.target_contract_cid.chars().take(32).collect::<String>(),
                    ce.file
                );
            }
        }
    }
}

/// Decide an exit code from a report. Discharged or empty -> success;
/// any violation or load error -> verification failure (exit 1).
pub fn report_exit_code(r: &Report) -> u8 {
    if r.violations > 0 || !r.load_errors.is_empty() {
        crate::EXIT_VERIFY_FAIL
    } else {
        crate::EXIT_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use provekit_verifier::Report;

    #[test]
    fn empty_report_is_clean() {
        let r = Report::default();
        assert_eq!(report_exit_code(&r), crate::EXIT_OK);
        let j = report_to_json(&r);
        assert_eq!(j["totalCallsites"], 0);
        assert_eq!(j["violations"], 0);
    }

    #[test]
    fn report_with_violation_exits_fail() {
        let mut r = Report::default();
        r.violations = 1;
        assert_eq!(report_exit_code(&r), crate::EXIT_VERIFY_FAIL);
    }

    #[test]
    fn report_with_load_error_exits_fail() {
        let mut r = Report::default();
        r.load_errors.push(LoadError {
            proof_path: "x.proof".into(),
            reason: "bogus".into(),
        });
        assert_eq!(report_exit_code(&r), crate::EXIT_VERIFY_FAIL);
    }
}
