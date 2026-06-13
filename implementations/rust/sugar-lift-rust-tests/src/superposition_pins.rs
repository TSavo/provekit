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
use sugar_ir_symbolic::{num, str_const, ConstValue, ContractDecl, Formula, Sort, Term};
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

impl Pin {
    fn to_term_pin(&self) -> TermPin {
        TermPin {
            symbol: self.symbol.clone(),
            args: self.args.iter().map(ScalarLit::to_term).collect(),
            expected: self.expected.to_term(),
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

/// A vendor pin whose argument and expected values are arbitrary lifted TERMS,
/// not just literals. Recovered from the lifter's already-translated assertion
/// atoms (`eq(call:SYMBOL(argTerms), expectedTerm)`), so expression arguments —
/// `double(2 + 1)`, nested calls — flow through with whatever teeth the term
/// algebra gives them. This is the doctrine-right extractor: consume what the
/// lifter lifted, do not re-parse the AST.
#[derive(Debug, Clone)]
pub struct TermPin {
    pub symbol: String,
    pub args: Vec<Rc<Term>>,
    pub expected: Rc<Term>,
}

impl TermPin {
    pub fn cid(&self) -> String {
        let j = serde_json::json!({
            "kind": "vendor-pin",
            "symbol": self.symbol,
            "args": self.args.iter().map(|a| term_to_json(a)).collect::<Vec<_>>(),
            "expected": term_to_json(&self.expected),
        });
        cid_of_value(serde_to_canonical(j).as_ref())
    }
}

fn term_to_json(t: &Term) -> serde_json::Value {
    match t {
        Term::Var { name } => serde_json::json!({ "var": name }),
        Term::Const { value, .. } => match value {
            ConstValue::Int(i) => serde_json::json!({ "int": i }),
            ConstValue::Bool(b) => serde_json::json!({ "bool": b }),
            ConstValue::String(s) => serde_json::json!({ "str": s }),
            ConstValue::Real(r) => serde_json::json!({ "real": r }),
        },
        Term::Ctor { name, args } => serde_json::json!({
            "ctor": name,
            "args": args.iter().map(|a| term_to_json(a)).collect::<Vec<_>>(),
        }),
        Term::Lambda { .. } => serde_json::json!({ "lambda": true }),
        Term::Let { .. } => serde_json::json!({ "let": true }),
    }
}

/// Extract term pins from the lifter's assertion declarations (the lifted vendor
/// `#[test]` assertions). Each `eq(call:SYMBOL(args), expected)` atom becomes a
/// pin; conjunctions are walked. This subsumes the literal-only AST extraction —
/// the lifter handles whatever it could translate (arithmetic, calls, casts).
pub fn pins_from_assertion_decls(decls: &[ContractDecl]) -> Vec<TermPin> {
    let mut out = Vec::new();
    for d in decls {
        if let Some(inv) = &d.inv {
            collect_term_pins(inv, &mut out);
        }
    }
    out
}

fn collect_term_pins(f: &Formula, out: &mut Vec<TermPin>) {
    match f {
        Formula::Connective { kind, operands } if kind == "and" => {
            for op in operands {
                collect_term_pins(op, out);
            }
        }
        // The lifter emits equality atoms named "=" (also accept "eq").
        Formula::Atomic { name, args } if (name == "=" || name == "eq") && args.len() == 2 => {
            if let Some(pin) = pin_from_eq(&args[0], &args[1]) {
                out.push(pin);
            }
        }
        _ => {}
    }
}

fn pin_from_eq(a: &Rc<Term>, b: &Rc<Term>) -> Option<TermPin> {
    if let Some((symbol, args)) = call_term(a) {
        return Some(TermPin {
            symbol,
            args,
            expected: Rc::clone(b),
        });
    }
    if let Some((symbol, args)) = call_term(b) {
        return Some(TermPin {
            symbol,
            args,
            expected: Rc::clone(a),
        });
    }
    None
}

/// A `call:SYMBOL(args)` constructor term -> (last path segment, args).
fn call_term(t: &Term) -> Option<(String, Vec<Rc<Term>>)> {
    if let Term::Ctor { name, args } = t {
        if let Some(sym) = name.strip_prefix("call:") {
            let sym = sym.rsplit("::").next().unwrap_or(sym).to_string();
            return Some((sym, args.clone()));
        }
    }
    None
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
    let term_pins: Vec<TermPin> = pins.iter().map(Pin::to_term_pin).collect();
    symbol_report_terms(symbol, body_warrant, param_names, &term_pins, oracle)
}

/// Core report path over term pins (the lifter-atom extractor feeds this).
pub fn symbol_report_terms(
    symbol: &str,
    body_warrant: &ContractDecl,
    param_names: &[String],
    pins: &[TermPin],
    oracle: &dyn SatOracle,
) -> Option<SuperpositionReport> {
    let mine: Vec<&TermPin> = pins
        .iter()
        .filter(|p| p.symbol == symbol && p.args.len() == param_names.len())
        .collect();
    if mine.is_empty() {
        return None; // no liftable pin -> silent, not a verdict.
    }

    let mut checks: Vec<PinCheck> = Vec::with_capacity(mine.len());
    for p in &mine {
        let bindings: Vec<(&str, Rc<Term>)> = param_names
            .iter()
            .map(|n| n.as_str())
            .zip(p.args.iter().cloned())
            .collect();
        let conjoined =
            warrant_conjoined_with_vendor_terms(body_warrant, &bindings, Rc::clone(&p.expected));
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
    fn lifter_atoms_give_expression_args_teeth() {
        // The doctrine-right extractor: consume the lifter's atoms, not the raw
        // AST. `double(2 + 1)` has an EXPRESSION argument — the literal-only AST
        // path would miss it; the lifted atom carries it as a term. double(2+1)
        // is 6, so ==6 holds (Strong) and ==7 contradicts (a finding).
        let (decl, params) = body_warrant("fn double(x: i32) -> i32 { x * 2 }");

        let good = lift_assertion_pins("fn t() { assert_eq!(double(2 + 1), 6); }");
        let bad_block = lift_assertion_pins("fn t() { assert_eq!(double(2 + 1), 7); assert_eq!(double(4), 8); }");
        if z3_present() {
            // good: the expression-arg pin holds.
            let r_good = symbol_report_terms("double", &decl, &params, &good, &Z3Oracle::default())
                .expect("expression-arg pin composes");
            assert_eq!(r_good.strength, Strength::Strong, "double(2+1)==6 holds");
            // bad: 6 != 7 -> the expression-arg pin is a finding (licensed by double(4)==8).
            let r_bad = symbol_report_terms("double", &decl, &params, &bad_block, &Z3Oracle::default())
                .expect("licensed by the correct pin");
            assert_eq!(r_bad.strength, Strength::Weak);
            assert_eq!(r_bad.findings.len(), 1, "the wrong expression-arg pin is a finding");
        }
    }

    /// Lift a test fn through the real adapter and recover its term pins.
    fn lift_assertion_pins(src: &str) -> Vec<TermPin> {
        let wrapped = format!("#[test] {src}");
        let file: syn::File = syn::parse_str(&wrapped).unwrap();
        let out = crate::lift_file_with_options(&file, "test.rs", &crate::LiftOptions::default());
        pins_from_assertion_decls(&out.decls)
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
