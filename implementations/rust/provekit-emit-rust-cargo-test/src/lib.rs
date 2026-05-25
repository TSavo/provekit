// SPDX-License-Identifier: Apache-2.0
//
// provekit-emit-rust-cargo-test
// ==============================
//
// EMITTER kit (the inverse of `provekit-lift-rust-tests`, the rust
// HARVESTER). Given a contract -- its neutral predicates (`IrFormula`) plus
// the target function signature -- this kit MATERIALIZES the predicates as a
// `#[cfg(test)] mod tests { ... }` block whose `#[test]` functions assert the
// predicates using the standard `assert!` / `assert_eq!` / `assert_ne!`
// macros.
//
// The predicate -> cargo-test-assertion mapping is INLINE rust framework
// knowledge and lives HERE, NOT in a catalog memento family. (An earlier
// attempt to externalize it -- PR-5 -- was closed as an architectural
// mistake; framework spelling belongs to the framework kit.)
//
// Neutral predicate atoms (canonical `IrFormula::Atomic` names emitted by the
// harvester, see provekit-ir-symbolic):
//
//     "="  -> assert_eq!(a, b)
//     "≠"  -> assert_ne!(a, b)
//     "<"  -> assert!(a < b)
//     "≤"  -> assert!(a <= b)
//     ">"  -> assert!(a > b)
//     "≥"  -> assert!(a >= b)
//
// As a convenience the `concept:eq` / `concept:ne` / `concept:lt` / ...
// sugar spellings are also accepted (aliases of the canonical symbols).
//
// Structural option predicates: the harvester does NOT mint a dedicated
// `option-is-some` atom; it emits `assert_eq!(x, Some(_))` as
// `Atomic { name: "=", args: [x, Ctor("Some", [_])] }`. This kit recognizes
// that shape and emits the idiomatic `assert!(x.is_some())` /
// `assert!(x.is_none())` instead of a literal `==` against a wildcard.
//
// Fallible / throws: an `IrFormula::Atomic { name: "fallible-err", args:[x] }`
// (or its `concept:fallible-err` alias) emits `assert!(x.is_err())`. There is
// no primitive `throws` atom in the substrate today; `#[should_panic]` is not
// emitted because no neutral predicate carries that intent. If/when such a
// concept lands this is the single place to extend.

use provekit_ir_types::{IrFormula, IrTerm};
use serde::{Deserialize, Serialize};

/// The function signature a contract's predicates are about. Variables in the
/// predicates that match a formal parameter render as that parameter's name;
/// everything else renders structurally.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Function name (used to label / namespace the emitted tests).
    #[serde(default)]
    pub function: String,
    /// Formal parameter names, in order.
    #[serde(default)]
    pub params: Vec<String>,
    /// Formal parameter types, in order (parallel to `params`). Currently
    /// informational; reserved for future type-directed assertion shaping.
    #[serde(default)]
    pub param_types: Vec<String>,
    /// Return type. Informational for v1.
    #[serde(default)]
    pub return_type: String,
}

/// Result of emitting a cargo-test module for a contract.
#[derive(Debug, Clone)]
pub struct EmittedModule {
    /// The rust source for the test module (`#[cfg(test)] mod tests { ... }`).
    pub source: String,
    /// Per-predicate emit outcomes, in input order.
    pub tests: Vec<EmittedTest>,
    /// Predicates that could not be mapped to an assertion (unsupported atom
    /// or unsupported term shape). Each entry is a human-readable reason.
    pub skipped: Vec<String>,
}

/// One emitted `#[test]` function.
#[derive(Debug, Clone)]
pub struct EmittedTest {
    /// The generated test function name.
    pub name: String,
    /// The single assertion statement (e.g. `assert_eq!(a, b);`).
    pub assertion: String,
}

// ---------------------------------------------------------------------------
// Predicate -> assertion mapping (INLINE rust framework knowledge).
// ---------------------------------------------------------------------------

/// Normalize a predicate atom name to its canonical symbol. Accepts both the
/// canonical math symbols emitted by the harvester and the `concept:` sugar
/// spellings named in the kit brief.
fn canonical_atom(name: &str) -> &str {
    match name {
        "=" | "concept:eq" | "eq" => "=",
        "\u{2260}" | "concept:ne" | "ne" => "\u{2260}",
        "<" | "concept:lt" | "lt" => "<",
        "\u{2264}" | "concept:le" | "le" | "lte" => "\u{2264}",
        ">" | "concept:gt" | "gt" => ">",
        "\u{2265}" | "concept:ge" | "ge" | "gte" => "\u{2265}",
        "fallible-err" | "concept:fallible-err" | "fallible_err" => "fallible-err",
        other => other,
    }
}

/// Render an `IrTerm` to a rust expression string.
///
/// - `Var { name }`        -> `name`
/// - `Const { value }`     -> rust literal (int / string / bool / null)
/// - `Ctor { name, args }` -> see below
///
/// Constructor shapes (matching what the rust harvester lifts):
/// - nullary ctor `Foo`              -> `Foo`
/// - call-style ctor `f(a, b)`       -> `f(a, b)`  (free function / tuple-struct)
/// - operator ctors (`+ - * / %`)    -> `(a + b)` infix
/// - `array(a, b)`                   -> `[a, b]`
/// - `tuple(a, b)`                   -> `(a, b)`
/// - `vec(a, b)`                     -> `vec![a, b]`
/// - method ctor: when the *first* arg is the receiver this is ambiguous with a
///   free call; the harvester emits method calls as `Ctor("method", [recv, ...])`
///   too, so we render `recv.method(rest)` only for a small whitelist of common
///   accessor methods (`len`, `is_some`, `is_none`, `is_err`, `is_ok`). Anything
///   else renders call-style `name(args)`.
fn render_term(t: &IrTerm) -> Result<String, String> {
    match t {
        IrTerm::Var { name } => Ok(name.clone()),
        IrTerm::Const { value, .. } => render_const(value),
        IrTerm::Ctor { name, args } => render_ctor(name, args),
        IrTerm::Lambda { .. } => Err("lambda terms are not renderable as assertions".into()),
        IrTerm::Let { .. } => Err("let terms are not renderable as assertions".into()),
    }
}

fn render_const(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => Ok(format!("{s:?}")),
        serde_json::Value::Null => Ok("()".to_string()),
        other => Err(format!("unsupported const value: {other}")),
    }
}

/// Method-receiver ctors we render as `recv.method(args)`.
const METHOD_CTORS: &[&str] = &[
    "len", "is_some", "is_none", "is_err", "is_ok", "abs", "count",
];

fn render_ctor(name: &str, args: &[IrTerm]) -> Result<String, String> {
    // Operator ctors, rendered infix.
    if matches!(name, "+" | "-" | "*" | "/" | "%") && args.len() == 2 {
        let l = render_term(&args[0])?;
        let r = render_term(&args[1])?;
        return Ok(format!("({l} {name} {r})"));
    }
    match name {
        "array" => {
            let rendered = render_args(args)?;
            Ok(format!("[{}]", rendered.join(", ")))
        }
        "tuple" => {
            let rendered = render_args(args)?;
            // A 1-tuple in rust requires a trailing comma: `(x,)`. Without it
            // `(x)` is a parenthesized scalar, not a tuple, so it would not
            // round-trip a `Ctor("tuple", [x])` faithfully.
            if rendered.len() == 1 {
                Ok(format!("({},)", rendered[0]))
            } else {
                Ok(format!("({})", rendered.join(", ")))
            }
        }
        "vec" => {
            let rendered = render_args(args)?;
            Ok(format!("vec![{}]", rendered.join(", ")))
        }
        // The harvester lifts bool literals as `Ctor("True"/"False", [])`
        // (provekit-lift-rust-tests, syn::Lit::Bool). Render them as the rust
        // lowercase bool literals, NOT bare `True`/`False` (which are not valid
        // rust and would fail to compile, e.g. `assert_eq!(is_even(4), True)`).
        "True" if args.is_empty() => Ok("true".to_string()),
        "False" if args.is_empty() => Ok("false".to_string()),
        "None" if args.is_empty() => Ok("None".to_string()),
        m if METHOD_CTORS.contains(&m) && !args.is_empty() => {
            let recv = render_term(&args[0])?;
            let rest = render_args(&args[1..])?;
            Ok(format!("{recv}.{m}({})", rest.join(", ")))
        }
        _ => {
            // Free function / tuple-struct ctor call.
            let rendered = render_args(args)?;
            if rendered.is_empty() {
                Ok(name.to_string())
            } else {
                Ok(format!("{name}({})", rendered.join(", ")))
            }
        }
    }
}

fn render_args(args: &[IrTerm]) -> Result<Vec<String>, String> {
    args.iter().map(render_term).collect()
}

/// Is this term a `Some(_)` / `Some(<term>)` constructor?
fn is_some_ctor(t: &IrTerm) -> bool {
    matches!(t, IrTerm::Ctor { name, .. } if name == "Some")
}

/// Is this term a `None` constructor?
fn is_none_ctor(t: &IrTerm) -> bool {
    matches!(t, IrTerm::Ctor { name, args } if name == "None" && args.is_empty())
}

/// Is this term a wildcard placeholder (`_`)? The harvester represents a
/// "don't care" pattern position as `Var { name: "_" }`.
fn is_wildcard(t: &IrTerm) -> bool {
    matches!(t, IrTerm::Var { name } if name == "_")
}

/// Map a single atomic predicate to a rust assertion statement.
///
/// Returns `Ok(stmt)` for supported predicates, `Err(reason)` otherwise.
fn assertion_for_atom(name: &str, args: &[IrTerm]) -> Result<String, String> {
    let atom = canonical_atom(name);

    // Single-arg fallible predicate: `x` is an `Err`.
    if atom == "fallible-err" {
        if args.len() != 1 {
            return Err(format!("fallible-err expects 1 arg, got {}", args.len()));
        }
        let x = render_term(&args[0])?;
        return Ok(format!("assert!({x}.is_err());"));
    }

    // Binary comparison predicates.
    if args.len() != 2 {
        return Err(format!("atom `{atom}` expects 2 args, got {}", args.len()));
    }

    // option-is-some / option-is-none structural recognition on equality:
    //   `=(x, Some(_))` -> assert!(x.is_some())
    //   `=(x, None)`    -> assert!(x.is_none())
    // (and the symmetric orderings).
    if atom == "=" {
        if let Some(stmt) = option_assertion(&args[0], &args[1])? {
            return Ok(stmt);
        }
    }

    let l = render_term(&args[0])?;
    let r = render_term(&args[1])?;
    let stmt = match atom {
        "=" => format!("assert_eq!({l}, {r});"),
        "\u{2260}" => format!("assert_ne!({l}, {r});"),
        "<" => format!("assert!({l} < {r});"),
        "\u{2264}" => format!("assert!({l} <= {r});"),
        ">" => format!("assert!({l} > {r});"),
        "\u{2265}" => format!("assert!({l} >= {r});"),
        other => return Err(format!("unsupported predicate atom: `{other}`")),
    };
    Ok(stmt)
}

/// Recognize the option-membership shapes hidden inside an equality atom.
/// Returns `Ok(Some(stmt))` when one side is a `Some(_)`/`None` ctor and the
/// other is the value under test, `Ok(None)` when this is an ordinary equality.
fn option_assertion(a: &IrTerm, b: &IrTerm) -> Result<Option<String>, String> {
    // `x == Some(_)` / `Some(_) == x` -> assert!(x.is_some())
    if is_some_ctor(b) && is_some_wildcard(b) {
        let x = render_term(a)?;
        return Ok(Some(format!("assert!({x}.is_some());")));
    }
    if is_some_ctor(a) && is_some_wildcard(a) {
        let x = render_term(b)?;
        return Ok(Some(format!("assert!({x}.is_some());")));
    }
    // `x == None` / `None == x` -> assert!(x.is_none())
    if is_none_ctor(b) {
        let x = render_term(a)?;
        return Ok(Some(format!("assert!({x}.is_none());")));
    }
    if is_none_ctor(a) {
        let x = render_term(b)?;
        return Ok(Some(format!("assert!({x}.is_none());")));
    }
    Ok(None)
}

/// Is this a `Some(_)` ctor whose single argument is a wildcard? When the
/// inner value is concrete (e.g. `Some(42)`) we keep the literal equality so
/// the assertion checks the payload, not just presence.
fn is_some_wildcard(t: &IrTerm) -> bool {
    matches!(
        t,
        IrTerm::Ctor { name, args }
            if name == "Some" && args.len() == 1 && is_wildcard(&args[0])
    )
}

/// Collect the atomic predicates inside an `IrFormula`, flattening top-level
/// `And` conjunctions. (A contract is conventionally a conjunction of
/// point predicates; each conjunct becomes its own `#[test]`.) Quantifiers,
/// disjunctions, and implications are not materializable as concrete cargo
/// tests and are reported as skipped by the caller.
fn flatten_atoms(
    f: &IrFormula,
    out: &mut Vec<(String, Vec<IrTerm>)>,
    unsupported: &mut Vec<String>,
) {
    match f {
        IrFormula::Atomic { name, args } => out.push((name.clone(), args.clone())),
        IrFormula::And { operands } => {
            for op in operands {
                flatten_atoms(op, out, unsupported);
            }
        }
        IrFormula::Or { .. } => unsupported.push("disjunction (Or) is not materializable".into()),
        IrFormula::Not { .. } => {
            unsupported.push("negation (Not) is not materializable as a test".into())
        }
        IrFormula::Implies { .. } => {
            unsupported.push("implication (Implies) is not materializable".into())
        }
        IrFormula::Forall { .. } => {
            unsupported.push("universal quantifier (Forall) is not materializable".into())
        }
        IrFormula::Exists { .. } => {
            unsupported.push("existential quantifier (Exists) is not materializable".into())
        }
        _ => unsupported.push("unsupported formula node".into()),
    }
}

/// Sanitize a function name into a valid rust test-ident fragment.
fn ident_fragment(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty()
        || out
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    {
        format!("c_{out}")
    } else {
        out
    }
}

/// Emit a cargo-test module verifying `predicates` for the function described
/// by `sig`. Each materializable atomic predicate becomes one `#[test]`.
pub fn emit_test_module(sig: &FunctionSignature, predicates: &[IrFormula]) -> EmittedModule {
    let base = ident_fragment(if sig.function.is_empty() {
        "contract"
    } else {
        &sig.function
    });

    let mut atoms: Vec<(String, Vec<IrTerm>)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for f in predicates {
        flatten_atoms(f, &mut atoms, &mut skipped);
    }

    let mut tests: Vec<EmittedTest> = Vec::new();
    for (i, (name, args)) in atoms.iter().enumerate() {
        match assertion_for_atom(name, args) {
            Ok(assertion) => tests.push(EmittedTest {
                name: format!("{base}_predicate_{i}"),
                assertion,
            }),
            Err(reason) => skipped.push(format!("predicate {i} (`{name}`): {reason}")),
        }
    }

    let mut body = String::new();
    body.push_str("#[cfg(test)]\n");
    body.push_str("mod tests {\n");
    body.push_str("    #![allow(unused_imports)]\n");
    body.push_str("    use super::*;\n");
    if tests.is_empty() {
        body.push_str("    // No materializable predicates in this contract.\n");
    }
    for t in &tests {
        body.push_str("\n    #[test]\n");
        body.push_str(&format!("    fn {}() {{\n", t.name));
        body.push_str(&format!("        {}\n", t.assertion));
        body.push_str("    }\n");
    }
    body.push_str("}\n");

    EmittedModule {
        source: body,
        tests,
        skipped,
    }
}

// ---------------------------------------------------------------------------
// PEP 1.7.0 realize-plugin RPC server (matches the rust kit conventions:
// `initialize` / `provekit.plugin.invoke` / `shutdown`, JSON-RPC over stdio).
// ---------------------------------------------------------------------------

use serde_json::Value;

fn error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// Handle a single JSON-RPC request value, returning the response value.
pub fn dispatch(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "name": "provekit-emit-rust-cargo-test",
                "version": "0.1.0",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": ["rust"],
                    "emits_signed_mementos": false,
                    "ir_version": "v1.1.0",
                    "test_framework": "cargo-test"
                }
            }
        }),
        "provekit.plugin.invoke" => {
            let Some(params) = request.get("params").and_then(Value::as_object) else {
                return error(id, -32602, "INVALID_PARAMS: params must be an object");
            };
            let sig = FunctionSignature {
                function: params
                    .get("function")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                params: string_array(params.get("params")),
                param_types: string_array(params.get("param_types")),
                return_type: params
                    .get("return_type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            // Accept either a single `predicate` object or a `predicates` array.
            let predicates: Vec<IrFormula> = if let Some(arr) =
                params.get("predicates").and_then(Value::as_array)
            {
                let mut out = Vec::with_capacity(arr.len());
                for v in arr {
                    match serde_json::from_value::<IrFormula>(v.clone()) {
                        Ok(f) => out.push(f),
                        Err(e) => {
                            return error(id, -32602, &format!("INVALID_PARAMS: predicate: {e}"))
                        }
                    }
                }
                out
            } else if let Some(p) = params.get("predicate") {
                match serde_json::from_value::<IrFormula>(p.clone()) {
                    Ok(f) => vec![f],
                    Err(e) => return error(id, -32602, &format!("INVALID_PARAMS: predicate: {e}")),
                }
            } else {
                return error(
                    id,
                    -32602,
                    "INVALID_PARAMS: missing `predicate`/`predicates`",
                );
            };

            let emitted = emit_test_module(&sig, &predicates);
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "source": emitted.source,
                    "extension": "rs",
                    "test_count": emitted.tests.len(),
                    "skipped": emitted.skipped,
                    "is_stub": false
                }
            })
        }
        "shutdown" | "provekit.plugin.shutdown" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": null
        }),
        _ => error(id, -32601, &format!("METHOD_NOT_FOUND: {method}")),
    }
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Run the JSON-RPC server: read one request object per line on stdin, write
/// one response object per line on stdout. Mirrors `provekit-realize-rust`.
pub fn run_rpc() {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let method = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let response = match serde_json::from_str::<Value>(line) {
            Ok(request) => dispatch(&request),
            Err(err) => error(Value::Null, -32700, &format!("PARSE_ERROR: {err}")),
        };
        let _ = serde_json::to_writer(&mut stdout, &response);
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
        if method == "shutdown" || method == "provekit.plugin.shutdown" {
            break;
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use provekit_ir_types::Sort;

    fn var(name: &str) -> IrTerm {
        IrTerm::Var {
            name: name.to_string(),
        }
    }

    fn int_const(n: i64) -> IrTerm {
        IrTerm::Const {
            value: serde_json::json!(n),
            sort: Sort::Primitive {
                name: "i64".to_string(),
            },
        }
    }

    fn atom(name: &str, args: Vec<IrTerm>) -> IrFormula {
        IrFormula::Atomic {
            name: name.to_string(),
            args,
        }
    }

    fn sig(name: &str, params: &[&str]) -> FunctionSignature {
        FunctionSignature {
            function: name.to_string(),
            params: params.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn eq_maps_to_assert_eq() {
        let s = assertion_for_atom("=", &[var("a"), var("b")]).unwrap();
        assert_eq!(s, "assert_eq!(a, b);");
    }

    #[test]
    fn ne_maps_to_assert_ne() {
        let s = assertion_for_atom("\u{2260}", &[var("a"), var("b")]).unwrap();
        assert_eq!(s, "assert_ne!(a, b);");
    }

    #[test]
    fn lt_le_gt_ge_map_to_operators() {
        assert_eq!(
            assertion_for_atom("<", &[var("a"), var("b")]).unwrap(),
            "assert!(a < b);"
        );
        assert_eq!(
            assertion_for_atom("\u{2264}", &[var("a"), var("b")]).unwrap(),
            "assert!(a <= b);"
        );
        assert_eq!(
            assertion_for_atom(">", &[var("a"), var("b")]).unwrap(),
            "assert!(a > b);"
        );
        assert_eq!(
            assertion_for_atom("\u{2265}", &[var("a"), var("b")]).unwrap(),
            "assert!(a >= b);"
        );
    }

    #[test]
    fn concept_sugar_aliases_accepted() {
        assert_eq!(
            assertion_for_atom("concept:eq", &[var("a"), var("b")]).unwrap(),
            "assert_eq!(a, b);"
        );
        assert_eq!(
            assertion_for_atom("concept:lt", &[var("a"), var("b")]).unwrap(),
            "assert!(a < b);"
        );
    }

    #[test]
    fn option_is_some_wildcard_shape() {
        let some_wild = IrTerm::Ctor {
            name: "Some".to_string(),
            args: vec![var("_")],
        };
        let s = assertion_for_atom("=", &[var("x"), some_wild]).unwrap();
        assert_eq!(s, "assert!(x.is_some());");
    }

    #[test]
    fn option_is_none_shape() {
        let none = IrTerm::Ctor {
            name: "None".to_string(),
            args: vec![],
        };
        let s = assertion_for_atom("=", &[var("x"), none]).unwrap();
        assert_eq!(s, "assert!(x.is_none());");
    }

    #[test]
    fn some_with_concrete_payload_keeps_equality() {
        // `Some(42)` is concrete -> keep the payload check, do NOT collapse to is_some().
        let some_42 = IrTerm::Ctor {
            name: "Some".to_string(),
            args: vec![int_const(42)],
        };
        let s = assertion_for_atom("=", &[var("x"), some_42]).unwrap();
        assert_eq!(s, "assert_eq!(x, Some(42));");
    }

    #[test]
    fn bool_literal_ctor_renders_lowercase() {
        // The harvester lifts `true`/`false` as Ctor("True"/"False", []).
        let true_lit = IrTerm::Ctor {
            name: "True".to_string(),
            args: vec![],
        };
        let false_lit = IrTerm::Ctor {
            name: "False".to_string(),
            args: vec![],
        };
        // `is_even(4) == true`
        let call_true = IrTerm::Ctor {
            name: "is_even".to_string(),
            args: vec![int_const(4)],
        };
        assert_eq!(
            assertion_for_atom("=", &[call_true, true_lit]).unwrap(),
            "assert_eq!(is_even(4), true);"
        );
        let call_false = IrTerm::Ctor {
            name: "is_even".to_string(),
            args: vec![int_const(3)],
        };
        assert_eq!(
            assertion_for_atom("=", &[call_false, false_lit]).unwrap(),
            "assert_eq!(is_even(3), false);"
        );
    }

    #[test]
    fn one_tuple_renders_with_trailing_comma() {
        let one_tuple = IrTerm::Ctor {
            name: "tuple".to_string(),
            args: vec![int_const(7)],
        };
        assert_eq!(render_term(&one_tuple).unwrap(), "(7,)");
    }

    #[test]
    fn fallible_err_shape() {
        let s = assertion_for_atom("fallible-err", &[var("r")]).unwrap();
        assert_eq!(s, "assert!(r.is_err());");
    }

    #[test]
    fn call_ctor_renders_as_call() {
        let call = IrTerm::Ctor {
            name: "parse_int".to_string(),
            args: vec![IrTerm::Const {
                value: serde_json::json!("42"),
                sort: Sort::Primitive {
                    name: "str".to_string(),
                },
            }],
        };
        let s = assertion_for_atom("=", &[call, int_const(42)]).unwrap();
        assert_eq!(s, "assert_eq!(parse_int(\"42\"), 42);");
    }

    #[test]
    fn method_ctor_renders_as_method_call() {
        let len_call = IrTerm::Ctor {
            name: "len".to_string(),
            args: vec![var("s")],
        };
        let s = assertion_for_atom("=", &[len_call, int_const(3)]).unwrap();
        assert_eq!(s, "assert_eq!(s.len(), 3);");
    }

    #[test]
    fn operator_ctor_renders_infix() {
        let sum = IrTerm::Ctor {
            name: "+".to_string(),
            args: vec![var("a"), var("b")],
        };
        let s = assertion_for_atom("=", &[sum, int_const(5)]).unwrap();
        assert_eq!(s, "assert_eq!((a + b), 5);");
    }

    #[test]
    fn unsupported_atom_is_err() {
        assert!(assertion_for_atom("bvult", &[var("a"), var("b")]).is_err());
    }

    #[test]
    fn module_wraps_predicates_in_tests() {
        let preds = vec![atom("=", vec![var("a"), var("b")])];
        let m = emit_test_module(&sig("my_fn", &["a", "b"]), &preds);
        assert!(m.source.contains("#[cfg(test)]"));
        assert!(m.source.contains("mod tests"));
        assert!(m.source.contains("#[test]"));
        assert!(m.source.contains("fn my_fn_predicate_0()"));
        assert!(m.source.contains("assert_eq!(a, b);"));
        assert_eq!(m.tests.len(), 1);
        assert!(m.skipped.is_empty());
    }

    #[test]
    fn conjunction_flattens_to_multiple_tests() {
        let conj = IrFormula::And {
            operands: vec![
                atom("=", vec![var("a"), var("b")]),
                atom("<", vec![var("a"), var("c")]),
            ],
        };
        let m = emit_test_module(&sig("f", &[]), &[conj]);
        assert_eq!(m.tests.len(), 2);
        assert!(m.source.contains("assert_eq!(a, b);"));
        assert!(m.source.contains("assert!(a < c);"));
    }

    #[test]
    fn quantifier_is_skipped_not_emitted() {
        let forall = IrFormula::Forall {
            name: "x".to_string(),
            sort: Sort::Primitive {
                name: "i64".to_string(),
            },
            body: Box::new(atom("=", vec![var("x"), var("x")])),
        };
        let m = emit_test_module(&sig("f", &[]), &[forall]);
        assert_eq!(m.tests.len(), 0);
        assert_eq!(m.skipped.len(), 1);
    }

    #[test]
    fn rpc_initialize_reports_kit() {
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize"
        });
        let resp = dispatch(&req);
        assert_eq!(resp["result"]["name"], "provekit-emit-rust-cargo-test");
        assert_eq!(resp["result"]["protocol_version"], "pep/1.7.0");
    }

    #[test]
    fn rpc_invoke_emits_source() {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "add",
                "params": ["a", "b"],
                "predicates": [
                    { "kind": "atomic", "name": "=", "args": [
                        { "kind": "var", "name": "a" },
                        { "kind": "var", "name": "b" }
                    ] }
                ]
            }
        });
        let resp = dispatch(&req);
        let src = resp["result"]["source"].as_str().unwrap();
        assert!(src.contains("assert_eq!(a, b);"), "got: {src}");
        assert_eq!(resp["result"]["test_count"], 1);
    }

    #[test]
    fn rpc_unknown_method_is_error() {
        let req = serde_json::json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" });
        let resp = dispatch(&req);
        assert!(resp.get("error").is_some());
    }
}
