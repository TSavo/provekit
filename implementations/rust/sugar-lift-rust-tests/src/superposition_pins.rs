// SPDX-License-Identifier: Apache-2.0
//
// Production seam (items 1+2 of the superposition residual): wire REAL lifted
// body warrants + REAL vendor pins into the superposition report engine.
//
// A vendor pin is an `assert_eq!(SYMBOL(int_lits...), int_lit)` (or reversed):
// the symbol under test, its integer argument literals, and the sworn output.
// For each symbol with a body warrant, we instantiate the warrant at each pin
// via `warrant_conjoined_with_vendor` (substitute params, conjoin the sworn
// output), check the closed conjunction with z3, and apply the keystone: >=1 SAT
// licenses the lift, its UNSAT pins are vendor findings; no SAT retracts it.
//
// This closes the body<->vendor-pin seam for the canonical integer-assertion
// shape. Non-integer / non-literal-argument assertions are not extracted here
// (surfaced as "no pins" -> no report, never a fake verdict).

use sugar_ir_symbolic::serialize::marshal_declarations;
use sugar_ir_symbolic::ContractDecl;
use sugar_walk::canonical::{cid_of_value, serde_to_canonical};
use sugar_walk::superposition_engine::{
    apply_keystone, LiftVerdict, PinCheck, SatOracle, SuperpositionReport,
};

use crate::warrant_conjoined_with_vendor;

/// A vendor pin extracted from an `assert_eq!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntPin {
    pub symbol: String,
    pub args: Vec<i64>,
    pub expected: i64,
}

impl IntPin {
    /// Content-addressed identity of the pin.
    pub fn cid(&self) -> String {
        let j = serde_json::json!({
            "kind": "vendor-pin",
            "symbol": self.symbol,
            "args": self.args,
            "expected": self.expected,
        });
        cid_of_value(serde_to_canonical(j).as_ref())
    }
}

/// The parameter names of a function signature, in order (receiver -> "self").
pub fn param_names(sig: &syn::Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Receiver(_) => Some("self".to_string()),
            syn::FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(id) => Some(id.ident.to_string()),
                _ => None,
            },
        })
        .collect()
}

/// Extract integer pins from a (test) function body. Walks top-level statements
/// and one level of nesting for `assert_eq!`/`assert_ne!`-shaped macros.
pub fn extract_int_pins(block: &syn::Block) -> Vec<IntPin> {
    let mut pins = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Macro(m) => collect_from_macro(&m.mac, &mut pins),
            syn::Stmt::Expr(syn::Expr::Macro(em), _) => collect_from_macro(&em.mac, &mut pins),
            syn::Stmt::Expr(syn::Expr::Block(b), _) => {
                pins.extend(extract_int_pins(&b.block));
            }
            _ => {}
        }
    }
    pins
}

fn collect_from_macro(mac: &syn::Macro, out: &mut Vec<IntPin>) {
    let is_eq = mac
        .path
        .segments
        .last()
        .is_some_and(|s| s.ident == "assert_eq");
    if !is_eq {
        return;
    }
    let parsed = mac.parse_body_with(
        syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
    );
    let exprs = match parsed {
        Ok(p) => p,
        Err(_) => return,
    };
    if exprs.len() < 2 {
        return;
    }
    if let Some(pin) = pin_from_pair(&exprs[0], &exprs[1]) {
        out.push(pin);
    }
}

/// `assert_eq!(SYMBOL(lits), lit)` or `assert_eq!(lit, SYMBOL(lits))`.
fn pin_from_pair(a: &syn::Expr, b: &syn::Expr) -> Option<IntPin> {
    if let (Some((symbol, args)), Some(expected)) = (as_symbol_call(a), as_int_literal(b)) {
        return Some(IntPin {
            symbol,
            args,
            expected,
        });
    }
    if let (Some(expected), Some((symbol, args))) = (as_int_literal(a), as_symbol_call(b)) {
        return Some(IntPin {
            symbol,
            args,
            expected,
        });
    }
    None
}

fn as_int_literal(expr: &syn::Expr) -> Option<i64> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(i),
            ..
        }) => i.base10_parse::<i64>().ok(),
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => as_int_literal(expr).map(|v| -v),
        syn::Expr::Group(g) => as_int_literal(&g.expr),
        syn::Expr::Paren(p) => as_int_literal(&p.expr),
        _ => None,
    }
}

fn as_symbol_call(expr: &syn::Expr) -> Option<(String, Vec<i64>)> {
    match expr {
        syn::Expr::Call(call) => {
            let symbol = match &*call.func {
                syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string())?,
                _ => return None,
            };
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
                args.push(as_int_literal(a)?);
            }
            Some((symbol, args))
        }
        syn::Expr::Group(g) => as_symbol_call(&g.expr),
        syn::Expr::Paren(p) => as_symbol_call(&p.expr),
        _ => None,
    }
}

fn inv_json(decl: &ContractDecl) -> serde_json::Value {
    let doc = marshal_declarations(std::slice::from_ref(decl));
    let parsed: serde_json::Value = serde_json::from_str(&doc).unwrap_or(serde_json::Value::Null);
    parsed
        .get(0)
        .and_then(|d| d.get("inv"))
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// Content-addressed identity of a body warrant (name + inv).
fn warrant_cid(decl: &ContractDecl) -> String {
    let j = serde_json::json!({
        "kind": "body-warrant",
        "name": decl.name,
        "inv": inv_json(decl),
    });
    cid_of_value(serde_to_canonical(j).as_ref())
}

/// Build the superposition report for one symbol: check its body warrant against
/// each of its vendor pins (instantiated), apply the keystone. Returns None when
/// there are no pins (nothing to check) or the lift is retracted (no pin SAT —
/// our overreach, never a vendor accusation).
pub fn symbol_report(
    symbol: &str,
    body_warrant: &ContractDecl,
    param_names: &[String],
    pins: &[IntPin],
    oracle: &dyn SatOracle,
) -> Option<SuperpositionReport> {
    let mine: Vec<&IntPin> = pins
        .iter()
        .filter(|p| p.symbol == symbol && p.args.len() == param_names.len())
        .collect();
    if mine.is_empty() {
        return None; // no liftable pin -> silent, not a verdict.
    }

    let mut checks: Vec<PinCheck> = Vec::with_capacity(mine.len());
    for p in &mine {
        let bindings: Vec<(&str, i64)> = param_names
            .iter()
            .map(|n| n.as_str())
            .zip(p.args.iter().copied())
            .collect();
        let conjoined = warrant_conjoined_with_vendor(body_warrant, &bindings, p.expected);
        let inv = inv_json(&conjoined);
        checks.push(PinCheck {
            pin_cid: p.cid(),
            result: oracle.check(&[&inv]),
        });
    }

    match apply_keystone(&checks) {
        LiftVerdict::Retracted { .. } => None,
        LiftVerdict::Licensed { findings, .. } => Some(SuperpositionReport::for_symbol(
            symbol,
            warrant_cid(body_warrant),
            findings,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sugar_walk::superposition::Strength;
    use sugar_walk::superposition_engine::Z3Oracle;

    fn z3_present() -> bool {
        std::path::Path::new("/usr/local/bin/z3").exists()
    }

    fn body_warrant(src: &str) -> (ContractDecl, Vec<String>) {
        let f: syn::ItemFn = syn::parse_str(src).unwrap();
        let name = f.sig.ident.to_string();
        let decl =
            crate::broad_functional_warrant(&name, &f.sig, &f.block).expect("body warrants");
        (decl, param_names(&f.sig))
    }

    fn test_block(src: &str) -> syn::Block {
        let f: syn::ItemFn = syn::parse_str(src).unwrap();
        *f.block
    }

    #[test]
    fn extracts_int_pins_from_assert_eq() {
        let block =
            test_block("fn t() { assert_eq!(double(3), 6); assert_eq!(8, double(4)); }");
        let pins = extract_int_pins(&block);
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0], IntPin { symbol: "double".into(), args: vec![3], expected: 6 });
        // reversed form recovered too
        assert_eq!(pins[1], IntPin { symbol: "double".into(), args: vec![4], expected: 8 });
    }

    #[test]
    fn consistent_pins_make_a_strong_report() {
        let (decl, params) = body_warrant("fn double(x: i32) -> i32 { x * 2 }");
        let block = test_block("fn t() { assert_eq!(double(3), 6); assert_eq!(double(4), 8); }");
        let pins = extract_int_pins(&block);
        let report = symbol_report("double", &decl, &params, &pins, &Z3Oracle::default());
        if z3_present() {
            let report = report.expect("licensed by consistent pins");
            assert_eq!(report.strength, Strength::Strong);
            assert!(report.findings.is_empty());
            assert!(report.levers.is_empty());
            assert!(report.verdict.contains("one reading"));
        }
    }

    #[test]
    fn a_pin_the_body_contradicts_is_a_finding_weak() {
        // body: double(x)=x*2. Vendor swears double(3)==7 (it is 6) AND double(4)==8.
        // double(3)==6 holds (licenses), double(3)==7 is the finding -> Weak.
        let (decl, params) = body_warrant("fn double(x: i32) -> i32 { x * 2 }");
        let block = test_block(
            "fn t() { assert_eq!(double(4), 8); assert_eq!(double(3), 7); }",
        );
        let pins = extract_int_pins(&block);
        let report = symbol_report("double", &decl, &params, &pins, &Z3Oracle::default());
        if z3_present() {
            let report = report.expect("licensed by the correct pin");
            assert_eq!(report.strength, Strength::Weak);
            assert_eq!(report.findings.len(), 1, "the contradicting pin is a finding");
            assert_eq!(report.levers.len(), 2);
            assert!(report.verdict.contains("ordering, not logic"));
            // recomputable
            assert_eq!(
                sugar_canonicalizer::blake3_512_of(&report.member_bytes()),
                report.cid()
            );
        }
    }

    #[test]
    fn no_pins_is_no_report_not_a_fake_verdict() {
        let (decl, params) = body_warrant("fn triple(x: i32) -> i32 { x * 3 }");
        let block = test_block("fn t() { assert_eq!(double(3), 6); }"); // pins a different symbol
        let pins = extract_int_pins(&block);
        let report = symbol_report("triple", &decl, &params, &pins, &Z3Oracle::default());
        assert!(report.is_none(), "no pin for this symbol -> no report (silent)");
    }
}
