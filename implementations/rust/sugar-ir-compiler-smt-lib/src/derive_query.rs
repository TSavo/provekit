// SPDX-License-Identifier: Apache-2.0
//
// derive_query: emit a QF_BV derive query for a strong-tier universe contract.
//
// Given the bv_tree JSON from an `int32.eq-bv-expr` atom (the walked vendor
// source body, e.g. `bv32.ite(bv32.slt(a,0), bv32.neg(a), a)` for Math.abs)
// and a concrete 32-bit integer input for each var, emit a self-contained
// QF_BV SMT-LIB script that:
//   1. Declares each var (e.g. `a`) as `(_ BitVec 32)`.
//   2. Declares `r` (the result) as `(_ BitVec 32)`.
//   3. Asserts `(= r <bv_expr(symbolic vars)>)` — the universe definition.
//   4. Asserts each var = its concrete input as a bv32 hex literal.
//   5. Emits `(check-sat)\n(get-value (r))\n`.
//
// z3 returns `sat` then `((r #x........))`. We parse the hex pattern
// as a signed i32 (two's complement). That is the DERIVED value —
// computed from the definition, not executed, not hardcoded.
//
// ADDITIVE: this module does not touch the discharge path. The existing
// `(assert (not X))\n(check-sat)` / `(assert X)\n(check-sat)` emission
// and `unsat`->Discharged / `sat`->Unsatisfied mapping are UNCHANGED.
//
// DESIGN: we parse the bv_tree JSON directly (raw serde_json::Value) rather
// than deserializing into IrTerm, because IrTerm::Const requires a `sort`
// field that the bv_tree nodes may not carry. The rendering logic mirrors
// `emit_bv32_term` / `emit_bv32_bool_term` from generated.rs exactly.

use std::collections::HashMap;

use serde_json::Value as Json;

/// Error type for derive-query emission.
#[derive(Debug, Clone)]
pub struct DeriveQueryError(pub String);

impl std::fmt::Display for DeriveQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The result of a derive query: the emitted SMT-LIB script plus metadata.
#[derive(Debug, Clone)]
pub struct DeriveQuery {
    /// The complete QF_BV SMT-LIB script to send to z3.
    pub smt: String,
    /// The bv_tree rendered as a readable SMT expression (with concrete inputs substituted).
    pub bv_expr_rendered: String,
    /// The var names extracted from the bv_tree in DFS order (param order).
    pub var_names: Vec<String>,
    /// The concrete input values (parallel to var_names).
    pub inputs: Vec<i32>,
    /// The result variable name used in the query (always `"r"`).
    pub result_var: String,
}

/// Render an i32 as an SMT-LIB bitvector hex literal: `#x` + 8 hex digits.
fn i32_to_bv32_hex(v: i32) -> String {
    let bits = v as u32;
    format!("#x{:08x}", bits)
}

/// Render a BV32 expression tree from raw JSON with optional var substitution.
///
/// `subst`: if `Some`, var nodes are looked up in the map (concrete rendering);
///          if `None`, var nodes render as their own name (symbolic).
/// Returns `None` if the tree contains an unsupported node.
fn render_bv_term(term: &Json, subst: Option<&HashMap<String, String>>) -> Option<String> {
    match term {
        Json::Object(obj) => {
            let kind = obj.get("kind")?.as_str()?;
            match kind {
                "var" => {
                    let name = obj.get("name")?.as_str()?;
                    match subst {
                        Some(map) => map.get(name).cloned(),
                        None => Some(name.to_string()),
                    }
                }
                "const" => {
                    let v = obj.get("value")?.as_i64()? as i32;
                    Some(i32_to_bv32_hex(v))
                }
                "ctor" => {
                    let name = obj.get("name")?.as_str()?;
                    let args = obj.get("args")?.as_array()?;
                    match name {
                        "bv32.ite" if args.len() == 3 => {
                            let cond = render_bv_bool_term(&args[0], subst)?;
                            let tb = render_bv_term(&args[1], subst)?;
                            let fb = render_bv_term(&args[2], subst)?;
                            Some(format!("(ite {} {} {})", cond, tb, fb))
                        }
                        "bv32.neg" if args.len() == 1 => {
                            let inner = render_bv_term(&args[0], subst)?;
                            Some(format!("(bvneg {})", inner))
                        }
                        "bv32.and" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvand {} {})", l, r))
                        }
                        "bv32.or" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvor {} {})", l, r))
                        }
                        "bv32.xor" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvxor {} {})", l, r))
                        }
                        "bv32.add" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvadd {} {})", l, r))
                        }
                        "bv32.sub" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvsub {} {})", l, r))
                        }
                        "bv32.mul" if args.len() == 2 => {
                            let l = render_bv_term(&args[0], subst)?;
                            let r = render_bv_term(&args[1], subst)?;
                            Some(format!("(bvmul {} {})", l, r))
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Render a BV32 Bool-sorted sub-expression from raw JSON.
fn render_bv_bool_term(term: &Json, subst: Option<&HashMap<String, String>>) -> Option<String> {
    match term {
        Json::Object(obj) => {
            let kind = obj.get("kind")?.as_str()?;
            if kind != "ctor" {
                return None;
            }
            let name = obj.get("name")?.as_str()?;
            let args = obj.get("args")?.as_array()?;
            match name {
                "bv32.slt" if args.len() == 2 => {
                    let l = render_bv_term(&args[0], subst)?;
                    let r = render_bv_term(&args[1], subst)?;
                    Some(format!("(bvslt {} {})", l, r))
                }
                "bv32.ule" if args.len() == 2 => {
                    let l = render_bv_term(&args[0], subst)?;
                    let r = render_bv_term(&args[1], subst)?;
                    Some(format!("(bvule {} {})", l, r))
                }
                "bv32.sle" if args.len() == 2 => {
                    let l = render_bv_term(&args[0], subst)?;
                    let r = render_bv_term(&args[1], subst)?;
                    Some(format!("(bvsle {} {})", l, r))
                }
                "bv32.eq" if args.len() == 2 => {
                    let l = render_bv_term(&args[0], subst)?;
                    let r = render_bv_term(&args[1], subst)?;
                    Some(format!("(= {} {})", l, r))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Collect unique var names from a bv_tree JSON in DFS pre-order.
fn collect_vars(term: &Json, out: &mut Vec<String>) {
    match term {
        Json::Object(obj) => {
            let kind = obj.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if kind == "var" {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    let s = name.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            } else if let Some(args) = obj.get("args").and_then(|v| v.as_array()) {
                for a in args {
                    collect_vars(a, out);
                }
            }
        }
        _ => {}
    }
}

/// Emit a QF_BV derive query.
///
/// `bv_tree_json`: the JSON representation of the BV expression tree, e.g.:
///   `{"kind":"ctor","name":"bv32.ite","args":[...]}`
///   This is the `args[1]` of the `int32.eq-bv-expr` atom from a minted proof.
///
/// `inputs`: concrete i32 values for each var, in DFS order of var names.
///   For Math.abs: one input (the argument `a`).
///
/// Returns the `DeriveQuery` containing the SMT script.
pub fn emit_derive_query(bv_tree_json: &Json, inputs: &[i32]) -> Result<DeriveQuery, DeriveQueryError> {
    // Collect var names in DFS order.
    let mut var_names: Vec<String> = Vec::new();
    collect_vars(bv_tree_json, &mut var_names);

    if var_names.len() != inputs.len() {
        return Err(DeriveQueryError(format!(
            "bv_tree has {} var(s) {:?} but {} input(s) supplied",
            var_names.len(),
            var_names,
            inputs.len()
        )));
    }

    // Build substitution: var_name -> bv32 hex literal of the concrete input.
    let mut subst: HashMap<String, String> = HashMap::new();
    for (vname, &inp) in var_names.iter().zip(inputs.iter()) {
        subst.insert(vname.clone(), i32_to_bv32_hex(inp));
    }

    // Render the bv_tree with the substitution applied (concrete, no free vars).
    let bv_expr_rendered = render_bv_term(bv_tree_json, Some(&subst)).ok_or_else(|| {
        DeriveQueryError(format!(
            "could not render bv_tree to SMT-LIB; unsupported node in tree: {}",
            bv_tree_json
        ))
    })?;

    let result_var = "r".to_string();
    let mut smt = String::new();

    // Logic header (QF_BV: quantifier-free, bitvector arithmetic).
    smt.push_str("(set-logic QF_BV)\n");

    // Declare each var as (_ BitVec 32).
    for vname in &var_names {
        smt.push_str(&format!("(declare-const {} (_ BitVec 32))\n", vname));
    }

    // Declare the result var.
    smt.push_str(&format!("(declare-const {} (_ BitVec 32))\n", result_var));

    // Render the bv_tree SYMBOLICALLY (vars stay as var names) for the
    // universe definition assertion:
    //   (assert (= r (ite (bvslt a #x00000000) (bvneg a) a)))
    let bv_expr_symbolic = render_bv_term(bv_tree_json, None).ok_or_else(|| {
        DeriveQueryError("could not render bv_tree symbolically".into())
    })?;

    // Assert the universe definition: r = bv_expr(symbolic vars).
    smt.push_str(&format!("(assert (= {} {}))\n", result_var, bv_expr_symbolic));

    // Assert each var = its concrete input.
    for (vname, &inp) in var_names.iter().zip(inputs.iter()) {
        let hex = i32_to_bv32_hex(inp);
        smt.push_str(&format!("(assert (= {} {}))\n", vname, hex));
    }

    // Check-sat + get-value.
    smt.push_str("(check-sat)\n");
    smt.push_str(&format!("(get-value ({}))\n", result_var));

    Ok(DeriveQuery {
        smt,
        bv_expr_rendered,
        var_names,
        inputs: inputs.to_vec(),
        result_var,
    })
}

/// Parse z3's `(get-value (r))` response line into a signed i32.
///
/// z3 returns a line like `((r #x80000000))`. We extract the hex pattern
/// and interpret the 32-bit value as a signed two's complement i32.
///
/// Returns `None` if the response does not match the expected pattern.
pub fn parse_model_value(response_line: &str, result_var: &str) -> Option<i32> {
    // Pattern: `((r #x........))` — find `#x` followed by 8 hex digits.
    let hex_start = response_line.find("#x")?;
    let hex_digits: String = response_line[hex_start + 2..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex_digits.len() < 8 {
        return None;
    }
    // Also verify the result_var name appears before the hex (sanity check).
    let before_hex = &response_line[..hex_start];
    if !before_hex.contains(result_var) {
        return None;
    }
    let u = u32::from_str_radix(&hex_digits[..8], 16).ok()?;
    // Interpret as signed two's complement.
    Some(u as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs_bv_tree() -> Json {
        // bv32.ite(bv32.slt(a, 0), bv32.neg(a), a)
        // The walked Math.abs body.
        serde_json::json!({
            "kind": "ctor",
            "name": "bv32.ite",
            "args": [
                {
                    "kind": "ctor",
                    "name": "bv32.slt",
                    "args": [
                        {"kind": "var", "name": "a"},
                        {"kind": "const", "value": 0}
                    ]
                },
                {
                    "kind": "ctor",
                    "name": "bv32.neg",
                    "args": [{"kind": "var", "name": "a"}]
                },
                {"kind": "var", "name": "a"}
            ]
        })
    }

    #[test]
    fn emit_derive_query_abs_min_value() {
        let tree = abs_bv_tree();
        let dq = emit_derive_query(&tree, &[i32::MIN]).expect("emit");
        // Must declare a and r.
        assert!(dq.smt.contains("(declare-const a (_ BitVec 32))"), "missing a decl:\n{}", dq.smt);
        assert!(dq.smt.contains("(declare-const r (_ BitVec 32))"), "missing r decl:\n{}", dq.smt);
        // Universe definition must be symbolic.
        assert!(dq.smt.contains("(assert (= r (ite (bvslt a #x00000000) (bvneg a) a)))"),
            "universe definition wrong:\n{}", dq.smt);
        // Input assertion: MIN_VALUE = #x80000000.
        assert!(dq.smt.contains("(assert (= a #x80000000))"), "input assertion wrong:\n{}", dq.smt);
        // Must end with check-sat + get-value.
        assert!(dq.smt.contains("(check-sat)\n(get-value (r))\n"), "missing check-sat/get-value:\n{}", dq.smt);
        // QF_BV header.
        assert!(dq.smt.starts_with("(set-logic QF_BV)\n"), "must start with QF_BV:\n{}", dq.smt);
    }

    #[test]
    fn emit_derive_query_wrong_arity_errors() {
        let tree = abs_bv_tree();
        let result = emit_derive_query(&tree, &[]);
        assert!(result.is_err(), "should error on arity mismatch");
        let msg = result.unwrap_err().0;
        assert!(msg.contains("1 var") || msg.contains("var(s)"), "error: {msg}");
    }

    #[test]
    fn parse_model_value_min_value() {
        // z3 response: ((r #x80000000))
        let line = "((r #x80000000))";
        let v = parse_model_value(line, "r").expect("parse");
        assert_eq!(v, i32::MIN, "MIN_VALUE two's complement = -2147483648");
    }

    #[test]
    fn parse_model_value_zero() {
        let line = "((r #x00000000))";
        let v = parse_model_value(line, "r").expect("parse zero");
        assert_eq!(v, 0);
    }

    #[test]
    fn parse_model_value_positive() {
        let line = "((r #x00000005))";
        let v = parse_model_value(line, "r").expect("parse 5");
        assert_eq!(v, 5);
    }

    #[test]
    fn parse_model_value_minus_one() {
        let line = "((r #xffffffff))";
        let v = parse_model_value(line, "r").expect("parse -1");
        assert_eq!(v, -1i32);
    }

    #[test]
    fn parse_model_value_wrong_var_returns_none() {
        let line = "((x #x00000005))";
        let v = parse_model_value(line, "r");
        assert!(v.is_none(), "wrong var name should return None");
    }

    // Integration test: actually run z3 on the emitted query and confirm the
    // derived value is -2147483648 for abs(MIN_VALUE).
    #[test]
    fn z3_derives_abs_min_value_is_min_value() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if Command::new("z3").arg("--version").output().is_err() {
            eprintln!("z3 absent: skipping derive z3 integration test");
            return;
        }
        let tree = abs_bv_tree();
        let dq = emit_derive_query(&tree, &[i32::MIN]).expect("emit");

        let mut child = Command::new("z3")
            .args(["-smt2", "-in"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn z3");
        child.stdin.take().unwrap().write_all(dq.smt.as_bytes()).expect("write");
        let out = child.wait_with_output().expect("wait");
        let stdout = String::from_utf8_lossy(&out.stdout);

        let lines: Vec<&str> = stdout
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.is_empty())
            .collect();
        assert!(lines.len() >= 2, "expected at least 2 lines, got: {stdout:?}");
        assert_eq!(lines[0], "sat", "z3 must return sat for the abs derive query; got: {stdout:?}");

        let derived = parse_model_value(lines[1], "r")
            .unwrap_or_else(|| panic!("could not parse model value from: {:?}", lines[1]));
        assert_eq!(
            derived, i32::MIN,
            "z3.model derives abs(MIN_VALUE) = -2147483648 (two's complement truth); got {derived}"
        );
    }
}
