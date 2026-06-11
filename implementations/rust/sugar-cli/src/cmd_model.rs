// SPDX-License-Identifier: Apache-2.0
//
// `sugar derive`: z3.model-based derived-value computation.
//
// Given a BV expression tree (the walked universe body, e.g. Math.abs) and one
// or more concrete integer inputs, asks z3 what the universe COMPUTES.
//
// This is the second solver primitive, complementing `check-sat` (yes/no
// membership). The derive path emits a QF_BV query:
//   (set-logic QF_BV)
//   (declare-const a (_ BitVec 32))
//   (declare-const r (_ BitVec 32))
//   (assert (= r (ite (bvslt a #x00000000) (bvneg a) a)))  ; walked body
//   (assert (= a #x80000000))                               ; concrete input
//   (check-sat)
//   (get-value (r))                                         ; => ((r #x80000000)) = -2147483648
//
// The punchline: z3.model independently derives abs(MIN_VALUE) = -2147483648
// from the walked Math.abs body. The JDK's own AbsTests.java SWEARS the same
// value (line 110, "// Strange but true"). Two unrelated witnesses, same
// strange truth, no execution. Derived, not asserted.
//
// NO-VENDOR AXIOM: the lift is the source of truth. The bv_tree MUST come from
// the lifted universe, never from a formula the CLI hardcodes. Two sound
// sources are offered:
//   --from-proof <path>   read the int32.eq-bv-expr universe atom out of a
//                         minted .proof (the universe atom's args[1] = bv_tree).
//   --bv-expr <json>      the extracted bv_tree JSON, passed in by a caller that
//                         already pulled it from the lift/.proof (e.g. run.sh).
// There is NO built-in abs formula. If the universe atom is absent from the
// proof, `--from-proof` has nothing to derive from and REFUSES (it never falls
// back to a built-in). That refusal is the proof the chain is real.
//
// ADDITIVE: this verb is NEW. The discharge path (`sugar prove`/`sugar verify`)
// is not modified.

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::Value as Json;
use sugar_verifier::cbor_decode::decode;

use sugar_ir_compiler_smt_lib::derive_query::emit_derive_query;
use sugar_verifier::solvers::model::solve_with_model;

use crate::{OutputFlags, EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR};

#[derive(Parser, Debug, Clone)]
pub struct ModelArgs {
    /// Read the BV expression tree from a minted .proof file.
    ///
    /// Walks the catalog's members for an `int32.eq-bv-expr` universe atom and
    /// extracts its `args[1]` (the bv_tree walked from the vendor source). This
    /// is the no-vendor-axiom path: the formula comes from the lifted universe,
    /// never from a built-in. If no universe atom is present, REFUSES.
    #[arg(long = "from-proof", conflicts_with = "bv_expr")]
    pub from_proof: Option<PathBuf>,

    /// The BV expression tree as JSON (the walked universe body).
    ///
    /// This is the `bv_tree` from an `int32.eq-bv-expr` atom, e.g.:
    ///   '{"kind":"ctor","name":"bv32.ite","args":[...]}'
    ///
    /// Intended for callers (e.g. run.sh) that already extracted the atom from
    /// the lift output / .proof. NOT a hardcoded convenience — the JSON must
    /// originate from a minted artifact.
    #[arg(long = "bv-expr", conflicts_with = "from_proof")]
    pub bv_expr: Option<String>,

    /// Concrete 32-bit integer input(s) for the query.
    /// Supply one per var in the BV tree (in DFS var order).
    /// For Math.abs: one input (the argument `a`).
    #[arg(long = "input", short = 'i', required = true)]
    pub inputs: Vec<i32>,

    /// Path to z3 binary (default: "z3" on PATH).
    #[arg(long, default_value = "z3")]
    pub z3: String,

    /// Write the derive receipt JSON to this file.
    #[arg(long = "receipt-out")]
    pub receipt_out: Option<PathBuf>,

    #[command(flatten)]
    pub out: OutputFlags,
}

/// The JSON receipt structure.
#[derive(Debug, serde::Serialize)]
struct DeriveReceipt {
    /// Where the bv_tree came from (a minted .proof path, or "caller-supplied bv-expr").
    bv_tree_source: String,
    /// The BV tree JSON (the walked universe expression, extracted from the lift).
    bv_tree: Json,
    /// The BV expression as rendered SMT (with concrete inputs substituted).
    bv_expr_rendered: String,
    /// The complete SMT-LIB script sent to z3.
    smt_query: String,
    /// Concrete input(s) in order.
    inputs: Vec<i32>,
    /// The derived output value (signed i32, two's complement).
    derived_value: i32,
    /// z3's first-line verdict (must be "sat").
    verdict: String,
    /// z3's raw get-value response line.
    model_line: String,
    /// The solver binary used.
    solver: String,
    /// Wall-clock time in milliseconds.
    wall_clock_ms: u64,
}

pub fn run(args: ModelArgs) -> u8 {
    // Resolve the BV tree JSON. It MUST come from the lifted universe — either
    // read out of a minted .proof, or passed in by a caller that already did so.
    // There is no built-in formula.
    let (bv_tree, bv_tree_source): (Json, String) = if let Some(ref proof_path) = args.from_proof {
        match extract_bv_tree_from_proof(proof_path) {
            Ok(tree) => (
                tree,
                format!(
                    "minted .proof: {} (int32.eq-bv-expr universe atom, args[1])",
                    proof_path.display()
                ),
            ),
            Err(e) => {
                eprintln!(
                    "{}: could not extract universe bv_tree from {}: {e}",
                    "refused".red().bold(),
                    proof_path.display()
                );
                eprintln!(
                    "  the proof carries no int32.eq-bv-expr universe atom; there is nothing to derive from."
                );
                eprintln!("  (no built-in formula exists — the lift is the only source of truth.)");
                return EXIT_USER_ERROR;
            }
        }
    } else if let Some(ref json_str) = args.bv_expr {
        match serde_json::from_str(json_str) {
            Ok(v) => (
                v,
                "caller-supplied --bv-expr (extracted from lift/.proof)".to_string(),
            ),
            Err(e) => {
                eprintln!("{}: --bv-expr JSON parse error: {e}", "error".red().bold());
                return EXIT_USER_ERROR;
            }
        }
    } else {
        eprintln!(
            "{}: supply --from-proof <path> or --bv-expr <json> (extracted from a minted universe)",
            "error".red().bold()
        );
        return EXIT_USER_ERROR;
    };

    // Emit the derive query.
    let dq = match emit_derive_query(&bv_tree, &args.inputs) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("{}: derive query emission failed: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    if !args.out.quiet {
        eprintln!("[sugar derive] bv_tree source: {bv_tree_source}");
        eprintln!("[sugar derive] SMT query:\n{}", dq.smt);
    }

    // Run z3 with model extraction.
    let result = solve_with_model(&args.z3, &dq.smt, &dq.result_var, None);

    if !result.error.is_empty() {
        eprintln!("{}: solver error: {}", "error".red().bold(), result.error);
        return EXIT_SOLVER_FAIL;
    }

    if result.timed_out {
        eprintln!("{}: z3 timed out", "error".red().bold());
        return EXIT_SOLVER_FAIL;
    }

    if result.verdict_line != "sat" {
        eprintln!(
            "{}: z3 returned {:?} (expected sat); the derive query is unsatisfiable or errored",
            "error".red().bold(),
            result.verdict_line
        );
        eprintln!("raw stdout:\n{}", result.raw_stdout);
        return EXIT_SOLVER_FAIL;
    }

    let derived = match result.derived_value {
        Some(v) => v,
        None => {
            eprintln!(
                "{}: z3 returned sat but model value could not be parsed from: {:?}",
                "error".red().bold(),
                result.raw_stdout
            );
            return EXIT_SOLVER_FAIL;
        }
    };

    // Extract the model line (second non-empty line of z3 stdout).
    let model_line = result
        .raw_stdout
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .nth(1)
        .unwrap_or("")
        .to_string();

    let receipt = DeriveReceipt {
        bv_tree_source: bv_tree_source.clone(),
        bv_tree: bv_tree.clone(),
        bv_expr_rendered: dq.bv_expr_rendered.clone(),
        smt_query: dq.smt.clone(),
        inputs: args.inputs.clone(),
        derived_value: derived,
        verdict: result.verdict_line.clone(),
        model_line: model_line.clone(),
        solver: args.z3.clone(),
        wall_clock_ms: result.wall_clock.as_millis() as u64,
    };

    // Write receipt file if requested.
    if let Some(ref path) = args.receipt_out {
        let json_str = match serde_json::to_string_pretty(&receipt) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: receipt serialization: {e}", "error".red().bold());
                return EXIT_SOLVER_FAIL;
            }
        };
        if let Err(e) = std::fs::write(path, &json_str) {
            eprintln!("{}: writing receipt to {:?}: {e}", "error".red().bold(), path);
            return EXIT_SOLVER_FAIL;
        }
    }

    if args.out.json {
        println!("{}", serde_json::to_string_pretty(&receipt).unwrap_or_default());
    } else {
        println!();
        println!("  {}", "z3.model derive result".bold());
        println!("  bv_tree from : {bv_tree_source}");
        println!(
            "  input(s)     : {}",
            args.inputs
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("  bv_expr      : {}", dq.bv_expr_rendered);
        println!("  z3 verdict   : {}", result.verdict_line.green().bold());
        println!("  z3 model     : {model_line}");
        println!("  derived value: {}", derived.to_string().green().bold());
        println!();
        println!(
            "  {} z3.model DERIVES {} from the lifted universe body — derived, not executed.",
            "PASS".green().bold(),
            derived.to_string().green().bold()
        );
        println!();
        if let Some(ref path) = args.receipt_out {
            println!("  receipt written to: {path:?}");
        }
    }

    EXIT_OK
}

/// Read a minted .proof catalog and extract the bv_tree (args[1]) of the first
/// `int32.eq-bv-expr` universe atom found in any member.
///
/// Returns an error (REFUSE) if the proof carries no such atom — there is no
/// built-in fallback. This is the no-vendor-axiom guard: the bv_tree is only
/// ever the lifted universe expression.
fn extract_bv_tree_from_proof(path: &PathBuf) -> Result<Json, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let catalog = decode(&bytes).map_err(|e| format!("CBOR decode {}: {e}", path.display()))?;
    let map = catalog
        .as_map()
        .ok_or_else(|| "catalog root is not a CBOR map".to_string())?;
    let members = map
        .get("members")
        .and_then(|v| v.as_map())
        .ok_or_else(|| "catalog has no `members` map".to_string())?;

    for (_cid, val) in members {
        let bytes = match val.as_bstr() {
            Some(b) => b,
            None => continue,
        };
        let txt = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: Json = match serde_json::from_str(txt) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(tree) = find_universe_bv_tree(&parsed) {
            return Ok(tree);
        }
    }

    Err("no int32.eq-bv-expr universe atom in any catalog member".to_string())
}

/// Recursively search a JSON node for an `int32.eq-bv-expr` atom and return a
/// clone of its `args[1]` (the bv_tree). The atom shape (per the lift) is:
///   {"kind":"atomic","name":"int32.eq-bv-expr","args":[<subject>, <bv_tree>]}
fn find_universe_bv_tree(node: &Json) -> Option<Json> {
    match node {
        Json::Object(obj) => {
            if obj.get("name").and_then(|n| n.as_str()) == Some("int32.eq-bv-expr") {
                if let Some(args) = obj.get("args").and_then(|a| a.as_array()) {
                    if args.len() == 2 {
                        return Some(args[1].clone());
                    }
                }
            }
            for (_k, v) in obj {
                if let Some(found) = find_universe_bv_tree(v) {
                    return Some(found);
                }
            }
            None
        }
        Json::Array(arr) => {
            for v in arr {
                if let Some(found) = find_universe_bv_tree(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_universe_bv_tree_pulls_args1() {
        // A member-body-shaped node carrying an int32.eq-bv-expr atom.
        let body = serde_json::json!({
            "header": {
                "inv": {
                    "kind": "and",
                    "operands": [
                        {"kind": "atomic", "name": "=", "args": []},
                        {
                            "kind": "atomic",
                            "name": "int32.eq-bv-expr",
                            "args": [
                                {"kind": "ctor", "name": "call:abs", "args": []},
                                {"kind": "ctor", "name": "bv32.ite", "args": [
                                    {"kind": "var", "name": "a"}
                                ]}
                            ]
                        }
                    ]
                }
            }
        });
        let tree = find_universe_bv_tree(&body).expect("should find bv_tree");
        assert_eq!(tree["name"], "bv32.ite", "must return args[1] (the bv_tree)");
    }

    #[test]
    fn find_universe_bv_tree_absent_returns_none() {
        let body = serde_json::json!({
            "header": {"inv": {"kind": "atomic", "name": "=", "args": []}}
        });
        assert!(
            find_universe_bv_tree(&body).is_none(),
            "no universe atom -> None (the refuse path)"
        );
    }

    #[test]
    fn extracted_abs_tree_drives_a_derive_query() {
        // The bv_tree as it appears under args[1] in a minted proof (note the
        // `sort` field on const nodes — we read `value`, ignoring `sort`).
        let bv_tree = serde_json::json!({
            "kind": "ctor",
            "name": "bv32.ite",
            "args": [
                {"kind": "ctor", "name": "bv32.slt", "args": [
                    {"kind": "var", "name": "a"},
                    {"kind": "const", "sort": {"kind": "primitive", "name": "Int"}, "value": 0}
                ]},
                {"kind": "ctor", "name": "bv32.neg", "args": [{"kind": "var", "name": "a"}]},
                {"kind": "var", "name": "a"}
            ]
        });
        let dq = emit_derive_query(&bv_tree, &[i32::MIN]).expect("emit");
        assert!(dq.smt.contains("(get-value (r))"));
        assert!(dq.smt.contains("(assert (= a #x80000000))"));
        assert!(dq.smt.contains("(assert (= r (ite (bvslt a #x00000000) (bvneg a) a)))"));
    }
}
