// SPDX-License-Identifier: Apache-2.0
//
// Production seam (items 1+2 of the superposition residual): wire REAL lifted
// body warrants + REAL vendor pins into the superposition report engine.
//
// A vendor pin is an `assert_eq!(SYMBOL(scalar_lits...), scalar_lit)` (or
// reversed): the symbol under test, its scalar argument literals, and the sworn
// output. For each symbol with a body warrant, we instantiate the warrant at
// each pin via `warrant_conjoined_with_vendor_terms` (substitute params, conjoin
// the sworn output), check the closed conjunction with z3, and apply the
// keystone: >=1 SAT licenses the lift, its UNSAT pins are vendor findings; no
// SAT retracts it.
//
// Scalar coverage: int / bool / string literals (and negative ints). A
// non-scalar / non-literal-argument assertion is not extracted here (surfaced
// as "no pins" -> no report, never a fake verdict). Generalizing further means
// consuming the lifter's already-translated assertion atoms (the natural next
// step); literals cover the canonical vendor-test shape.

use std::rc::Rc;

use sugar_ir_symbolic::serialize::marshal_declarations;
use sugar_ir_symbolic::{num, str_const, ConstValue, ContractDecl, Sort, Term};
use sugar_walk::canonical::{cid_of_value, serde_to_canonical};
use sugar_walk::superposition_engine::{
    apply_keystone, LiftVerdict, PinCheck, SatOracle, SuperpositionReport,
};

use crate::warrant_conjoined_with_vendor_terms;

/// A scalar literal appearing in a vendor assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarLit {
    Int(i64),
    Bool(bool),
    Str(String),
}

impl ScalarLit {
    fn to_term(&self) -> Rc<Term> {
        match self {
            ScalarLit::Int(i) => num(*i),
            ScalarLit::Bool(b) => Rc::new(Term::Const {
                value: ConstValue::Bool(*b),
                sort: Sort::bool(),
            }),
            ScalarLit::Str(s) => str_const(s.clone()),
        }
    }

    fn json(&self) -> serde_json::Value {
        match self {
            ScalarLit::Int(i) => serde_json::json!({"int": i}),
            ScalarLit::Bool(b) => serde_json::json!({"bool": b}),
            ScalarLit::Str(s) => serde_json::json!({"str": s}),
        }
    }
}

/// A vendor pin extracted from an `assert_eq!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub symbol: String,
    pub args: Vec<ScalarLit>,
    pub expected: ScalarLit,
}

impl Pin {
    /// Content-addressed identity of the pin.
    pub fn cid(&self) -> String {
        let j = serde_json::json!({
            "kind": "vendor-pin",
            "symbol": self.symbol,
            "args": self.args.iter().map(ScalarLit::json).collect::<Vec<_>>(),
            "expected": self.expected.json(),
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

/// Extract vendor pins from a (test) function body. Walks top-level statements
/// and one level of block nesting for `assert_eq!`-shaped macros.
pub fn extract_pins(block: &syn::Block) -> Vec<Pin> {
    let mut pins = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Macro(m) => collect_from_macro(&m.mac, &mut pins),
            syn::Stmt::Expr(syn::Expr::Macro(em), _) => collect_from_macro(&em.mac, &mut pins),
            syn::Stmt::Expr(syn::Expr::Block(b), _) => pins.extend(extract_pins(&b.block)),
            _ => {}
        }
    }
    pins
}

fn collect_from_macro(mac: &syn::Macro, out: &mut Vec<Pin>) {
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
fn pin_from_pair(a: &syn::Expr, b: &syn::Expr) -> Option<Pin> {
    if let (Some((symbol, args)), Some(expected)) = (as_symbol_call(a), as_scalar_literal(b)) {
        return Some(Pin {
            symbol,
            args,
            expected,
        });
    }
    if let (Some(expected), Some((symbol, args))) = (as_scalar_literal(a), as_symbol_call(b)) {
        return Some(Pin {
            symbol,
            args,
            expected,
        });
    }
    None
}

fn as_scalar_literal(expr: &syn::Expr) -> Option<ScalarLit> {
    match expr {
        syn::Expr::Lit(syn::ExprLit { lit, .. }) => match lit {
            syn::Lit::Int(i) => i.base10_parse::<i64>().ok().map(ScalarLit::Int),
            syn::Lit::Bool(b) => Some(ScalarLit::Bool(b.value)),
            syn::Lit::Str(s) => Some(ScalarLit::Str(s.value())),
            _ => None,
        },
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => match as_scalar_literal(expr)? {
            ScalarLit::Int(v) => Some(ScalarLit::Int(-v)),
            _ => None,
        },
        syn::Expr::Group(g) => as_scalar_literal(&g.expr),
        syn::Expr::Paren(p) => as_scalar_literal(&p.expr),
        _ => None,
    }
}

fn as_symbol_call(expr: &syn::Expr) -> Option<(String, Vec<ScalarLit>)> {
    match expr {
        syn::Expr::Call(call) => {
            let symbol = match &*call.func {
                syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string())?,
                _ => return None,
            };
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
                args.push(as_scalar_literal(a)?);
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
    pins: &[Pin],
    oracle: &dyn SatOracle,
) -> Option<SuperpositionReport> {
    let mine: Vec<&Pin> = pins
        .iter()
        .filter(|p| p.symbol == symbol && p.args.len() == param_names.len())
        .collect();
    if mine.is_empty() {
        return None; // no liftable pin -> silent, not a verdict.
    }

    let mut checks: Vec<PinCheck> = Vec::with_capacity(mine.len());
    for p in &mine {
        let arg_terms: Vec<Rc<Term>> = p.args.iter().map(ScalarLit::to_term).collect();
        let bindings: Vec<(&str, Rc<Term>)> = param_names
            .iter()
            .map(|n| n.as_str())
            .zip(arg_terms.iter().cloned())
            .collect();
        let conjoined = warrant_conjoined_with_vendor_terms(body_warrant, &bindings, p.expected.to_term());
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
    fn extracts_pins_of_each_scalar_kind() {
        let block = test_block(
            r#"fn t() {
                assert_eq!(double(3), 6);
                assert_eq!(8, double(4));
                assert_eq!(is_even(4), true);
                assert_eq!(name_of(0), "zero");
            }"#,
        );
        let pins = extract_pins(&block);
        assert_eq!(pins.len(), 4);
        assert_eq!(pins[0], Pin { symbol: "double".into(), args: vec![ScalarLit::Int(3)], expected: ScalarLit::Int(6) });
        assert_eq!(pins[1], Pin { symbol: "double".into(), args: vec![ScalarLit::Int(4)], expected: ScalarLit::Int(8) });
        assert_eq!(pins[2], Pin { symbol: "is_even".into(), args: vec![ScalarLit::Int(4)], expected: ScalarLit::Bool(true) });
        assert_eq!(pins[3], Pin { symbol: "name_of".into(), args: vec![ScalarLit::Int(0)], expected: ScalarLit::Str("zero".into()) });
    }

    #[test]
    fn consistent_pins_make_a_strong_report() {
        let (decl, params) = body_warrant("fn double(x: i32) -> i32 { x * 2 }");
        let block = test_block("fn t() { assert_eq!(double(3), 6); assert_eq!(double(4), 8); }");
        let pins = extract_pins(&block);
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
        let (decl, params) = body_warrant("fn double(x: i32) -> i32 { x * 2 }");
        let block = test_block("fn t() { assert_eq!(double(4), 8); assert_eq!(double(3), 7); }");
        let pins = extract_pins(&block);
        let report = symbol_report("double", &decl, &params, &pins, &Z3Oracle::default());
        if z3_present() {
            let report = report.expect("licensed by the correct pin");
            assert_eq!(report.strength, Strength::Weak);
            assert_eq!(report.findings.len(), 1, "the contradicting pin is a finding");
            assert_eq!(report.levers.len(), 2);
            assert!(report.verdict.contains("ordering, not logic"));
            assert_eq!(
                sugar_canonicalizer::blake3_512_of(&report.member_bytes()),
                report.cid()
            );
        }
    }

    #[test]
    fn bool_and_string_pins_compose_through_z3() {
        // Bool/string pins extract and compose through the closed check. Whether
        // a wrong pin becomes a FINDING depends on the body warrant having
        // structural teeth — `x % 2 == 0` lifts to the opaque functional warrant
        // (out = call:is_even(x)), which coexists with any sworn output (weak
        // teeth, by design). The point here: the bool path is well-sorted and
        // composes (no crash, a report is produced), not a fake finding.
        let (decl, params) = body_warrant("fn is_even(x: i32) -> bool { x % 2 == 0 }");
        let block =
            test_block("fn t() { assert_eq!(is_even(4), true); assert_eq!(is_even(6), true); }");
        let pins = extract_pins(&block);
        assert_eq!(pins.len(), 2);
        assert!(matches!(pins[0].expected, ScalarLit::Bool(true)));
        let report = symbol_report("is_even", &decl, &params, &pins, &Z3Oracle::default());
        if z3_present() {
            let report = report.expect("bool pins compose -> licensed");
            // Opaque functional warrant coexists with the sworn outputs: Strong,
            // no false finding.
            assert_eq!(report.strength, Strength::Strong);
            assert!(report.findings.is_empty());
        }
    }

    #[test]
    fn no_pins_is_no_report_not_a_fake_verdict() {
        let (decl, params) = body_warrant("fn triple(x: i32) -> i32 { x * 3 }");
        let block = test_block("fn t() { assert_eq!(double(3), 6); }");
        let pins = extract_pins(&block);
        let report = symbol_report("triple", &decl, &params, &pins, &Z3Oracle::default());
        assert!(report.is_none(), "no pin for this symbol -> no report (silent)");
    }
}
