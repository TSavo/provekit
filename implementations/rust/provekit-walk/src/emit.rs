// SPDX-License-Identifier: Apache-2.0
//
// Emit the shadow source as a v1.5.0-shape proof.ir bundle.
//
// The bundle is a single JCS-canonical JSON document containing:
//   - schemaVersion: "provekit-walk/1"
//   - shadowSourceCid: top-level CID for the shadow source
//   - shadowSource: the canonical shadow-source bytes (decoded back to a
//     JSON object so consumers can inspect without re-canonicalizing)
//   - arrivals: array of every shadow arrival's edge memento, each
//     shaped as ContractDecl per paper 07 §11
//   - composedChain: optional flat composed edge for the longest chain
//
// This is the "from source to substrate" wire-format gap closed: feed
// any Rust source into walk_demo and out the other side comes a single
// JCS+BLAKE3-addressed bundle that downstream substrate tools (lift,
// linker, mint) can consume.

use std::sync::Arc;

use provekit_canonicalizer::Value;
use serde_json::{json, Value as JsonValue};
use syn::{BinOp, Expr, ExprIf, Lit, Stmt, UnOp};

use crate::canonical::{cid_of_value, jcs_bytes_of_value, serde_to_canonical};
use crate::shadow::{compose_chain, edge_memento_value, ShadowSource};
use crate::signature::{op_cid, RUST_LANGUAGE_SIGNATURE_CID};

/// Emit a single proof.ir bundle for the given shadow source.
/// Returns JCS-canonical bytes ready for write or transmit. The bundle's
/// own CID is included inline.
pub fn shadow_to_proof_ir(s: &ShadowSource) -> Vec<u8> {
    let bundle = build_bundle_value(s);
    jcs_bytes_of_value(&bundle)
}

/// CID of the proof.ir bundle.
pub fn shadow_proof_ir_cid(s: &ShadowSource) -> String {
    let bundle = build_bundle_value(s);
    cid_of_value(&bundle)
}

/// Emit a Rust algebra term over the minted rust:rust signature.
pub fn rust_function_term_json(
    item_fn: &syn::ItemFn,
    source: impl Into<String>,
) -> Result<Vec<u8>, String> {
    let value = rust_function_term_json_value(item_fn, source)?;
    let canonical = serde_to_canonical(value);
    Ok(jcs_bytes_of_value(&canonical))
}

/// CID of the emitted Rust algebra term JSON document.
pub fn rust_function_term_json_cid(
    item_fn: &syn::ItemFn,
    source: impl Into<String>,
) -> Result<String, String> {
    let value = rust_function_term_json_value(item_fn, source)?;
    let canonical = serde_to_canonical(value);
    Ok(cid_of_value(&canonical))
}

/// Build the inspectable JSON value before JCS encoding.
pub fn rust_function_term_json_value(
    item_fn: &syn::ItemFn,
    source: impl Into<String>,
) -> Result<JsonValue, String> {
    let term = lower_function_body_to_term(item_fn)?;
    let term_surface = term.surface();
    Ok(json!({
        "kind": "rust-algebra-term",
        "signature_cid": RUST_LANGUAGE_SIGNATURE_CID,
        "source": source.into(),
        "term_surface": term_surface,
        "term": term.to_json()?,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AlgebraTerm {
    Op {
        name: String,
        args: Vec<AlgebraTerm>,
    },
    Var(String),
    ConstInt(i64),
    ConstBool(bool),
    Unit,
}

impl AlgebraTerm {
    fn op(name: impl Into<String>, args: Vec<AlgebraTerm>) -> Self {
        Self::Op {
            name: name.into(),
            args,
        }
    }

    fn skip() -> Self {
        Self::op("skip", vec![Self::Unit])
    }

    fn to_json(&self) -> Result<JsonValue, String> {
        match self {
            AlgebraTerm::Op { name, args } => {
                let Some(cid) = op_cid(name) else {
                    return Err(format!("operation `{name}` is not in the Rust signature"));
                };
                let args = args
                    .iter()
                    .map(AlgebraTerm::to_json)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({
                    "kind": "op",
                    "name": name,
                    "op_cid": cid,
                    "args": args,
                }))
            }
            AlgebraTerm::Var(name) => Ok(json!({"kind": "var", "name": name})),
            AlgebraTerm::ConstInt(value) => Ok(json!({
                "kind": "const",
                "value": value,
                "sort": {"kind": "ctor", "name": "Int", "args": []}
            })),
            AlgebraTerm::ConstBool(value) => Ok(json!({
                "kind": "const",
                "value": value,
                "sort": {"kind": "ctor", "name": "Bool", "args": []}
            })),
            AlgebraTerm::Unit => Ok(json!({"kind": "unit"})),
        }
    }

    fn surface(&self) -> String {
        match self {
            AlgebraTerm::Op { name, args }
                if name == "skip" && matches!(args.as_slice(), [AlgebraTerm::Unit]) =>
            {
                "skip".to_string()
            }
            AlgebraTerm::Op { name, args } => {
                let args = args
                    .iter()
                    .map(AlgebraTerm::surface)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({args})")
            }
            AlgebraTerm::Var(name) => name.clone(),
            AlgebraTerm::ConstInt(value) => value.to_string(),
            AlgebraTerm::ConstBool(value) => value.to_string(),
            AlgebraTerm::Unit => "unit".to_string(),
        }
    }
}

fn lower_function_body_to_term(item_fn: &syn::ItemFn) -> Result<AlgebraTerm, String> {
    lower_stmts_to_stmt(&item_fn.block.stmts)
}

fn lower_stmts_to_stmt(stmts: &[Stmt]) -> Result<AlgebraTerm, String> {
    let mut lowered = Vec::new();
    for (idx, stmt) in stmts.iter().enumerate() {
        let is_tail = idx + 1 == stmts.len();
        match stmt {
            Stmt::Expr(expr, None) if is_tail => lowered.push(lower_tail_expr_to_stmt(expr)?),
            Stmt::Expr(expr, _) => lowered.push(lower_expr_to_stmt(expr)?),
            Stmt::Local(_) | Stmt::Item(_) | Stmt::Macro(_) => {}
        }
    }
    Ok(seq_all(lowered))
}

fn seq_all(terms: Vec<AlgebraTerm>) -> AlgebraTerm {
    let mut iter = terms.into_iter();
    let Some(first) = iter.next() else {
        return AlgebraTerm::skip();
    };
    iter.fold(first, |acc, term| AlgebraTerm::op("seq", vec![acc, term]))
}

fn lower_tail_expr_to_stmt(expr: &Expr) -> Result<AlgebraTerm, String> {
    if let Expr::If(if_expr) = expr {
        if let Some(term) = lower_tail_if_expr_to_stmt(if_expr)? {
            return Ok(term);
        }
    }
    Ok(AlgebraTerm::op(
        "return",
        vec![lower_expr_to_value_term(expr)?],
    ))
}

fn lower_tail_if_expr_to_stmt(if_expr: &ExprIf) -> Result<Option<AlgebraTerm>, String> {
    let Some((_, else_expr)) = &if_expr.else_branch else {
        return Ok(None);
    };
    let Some(then_expr) = block_single_tail_expr(&if_expr.then_branch) else {
        return Ok(None);
    };
    let Some(else_tail) = expr_single_tail_expr(else_expr) else {
        return Ok(None);
    };
    let cond = lower_expr_to_bool_term(&if_expr.cond)?;
    let then_return = AlgebraTerm::op("return", vec![lower_expr_to_value_term(then_expr)?]);
    let if_stmt = AlgebraTerm::op("if", vec![cond, then_return, AlgebraTerm::skip()]);
    let trailing_return = AlgebraTerm::op("return", vec![lower_expr_to_value_term(else_tail)?]);
    Ok(Some(AlgebraTerm::op("seq", vec![if_stmt, trailing_return])))
}

fn lower_expr_to_stmt(expr: &Expr) -> Result<AlgebraTerm, String> {
    match expr {
        Expr::Return(ret) => {
            let value = match &ret.expr {
                Some(value) => lower_expr_to_value_term(value)?,
                None => AlgebraTerm::Unit,
            };
            Ok(AlgebraTerm::op("return", vec![value]))
        }
        Expr::If(if_expr) => {
            let cond = lower_expr_to_bool_term(&if_expr.cond)?;
            let then_branch = lower_stmts_to_stmt(&if_expr.then_branch.stmts)?;
            let else_branch = match &if_expr.else_branch {
                Some((_, else_expr)) => lower_expr_to_stmt(else_expr)?,
                None => AlgebraTerm::skip(),
            };
            Ok(AlgebraTerm::op("if", vec![cond, then_branch, else_branch]))
        }
        Expr::Block(block) => lower_stmts_to_stmt(&block.block.stmts),
        _ => Ok(AlgebraTerm::skip()),
    }
}

fn lower_expr_to_bool_term(expr: &Expr) -> Result<AlgebraTerm, String> {
    match expr {
        Expr::Binary(binary) => {
            let op = match &binary.op {
                BinOp::Eq(_) => Some("eq"),
                BinOp::Ne(_) => Some("ne"),
                BinOp::Lt(_) => Some("lt"),
                BinOp::Le(_) => Some("le"),
                BinOp::Gt(_) => Some("gt"),
                BinOp::Ge(_) => Some("ge"),
                BinOp::And(_) => Some("and"),
                BinOp::Or(_) => Some("or"),
                _ => None,
            };
            let Some(op) = op else {
                return Err(format!("unsupported boolean operator: {:?}", binary.op));
            };
            Ok(AlgebraTerm::op(
                op,
                vec![
                    lower_expr_to_value_term(&binary.left)?,
                    lower_expr_to_value_term(&binary.right)?,
                ],
            ))
        }
        Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => Ok(AlgebraTerm::op(
            "not",
            vec![lower_expr_to_bool_term(&unary.expr)?],
        )),
        Expr::Paren(paren) => lower_expr_to_bool_term(&paren.expr),
        Expr::Lit(lit) => match &lit.lit {
            Lit::Bool(value) => Ok(AlgebraTerm::ConstBool(value.value)),
            _ => Err("non-bool literal in boolean term".to_string()),
        },
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| AlgebraTerm::Var(segment.ident.to_string()))
            .ok_or_else(|| "empty path in boolean term".to_string()),
        _ => Err(format!("unsupported boolean expression: {expr:?}")),
    }
}

fn lower_expr_to_value_term(expr: &Expr) -> Result<AlgebraTerm, String> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(value) => value
                .base10_parse::<i64>()
                .map(AlgebraTerm::ConstInt)
                .map_err(|err| format!("integer literal does not fit i64: {err}")),
            Lit::Bool(value) => Ok(AlgebraTerm::ConstBool(value.value)),
            _ => Err("unsupported literal expression".to_string()),
        },
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| AlgebraTerm::Var(segment.ident.to_string()))
            .ok_or_else(|| "empty path expression".to_string()),
        Expr::Paren(paren) => lower_expr_to_value_term(&paren.expr),
        Expr::Block(block) => {
            let Some(tail) = block_single_tail_expr(&block.block) else {
                return Err("block expression has no single tail expression".to_string());
            };
            lower_expr_to_value_term(tail)
        }
        Expr::Unary(unary) => {
            let op = match &unary.op {
                UnOp::Neg(_) => "neg",
                UnOp::Not(_) => "bit_not",
                UnOp::Deref(_) => "deref",
                _ => return Err(format!("unsupported unary operator: {:?}", unary.op)),
            };
            Ok(AlgebraTerm::op(
                op,
                vec![lower_expr_to_value_term(&unary.expr)?],
            ))
        }
        Expr::Binary(binary) => {
            let op = match &binary.op {
                BinOp::Add(_) => Some("add"),
                BinOp::Sub(_) => Some("sub"),
                BinOp::Mul(_) => Some("mul"),
                BinOp::Div(_) => Some("div"),
                BinOp::Rem(_) => Some("rem"),
                BinOp::BitAnd(_) => Some("bit_and"),
                BinOp::BitOr(_) => Some("bit_or"),
                BinOp::BitXor(_) => Some("bit_xor"),
                BinOp::Shl(_) => Some("shl"),
                BinOp::Shr(_) => Some("shr"),
                BinOp::Eq(_) => Some("eq"),
                BinOp::Ne(_) => Some("ne"),
                BinOp::Lt(_) => Some("lt"),
                BinOp::Le(_) => Some("le"),
                BinOp::Gt(_) => Some("gt"),
                BinOp::Ge(_) => Some("ge"),
                _ => None,
            };
            let Some(op) = op else {
                return Err(format!("unsupported value operator: {:?}", binary.op));
            };
            Ok(AlgebraTerm::op(
                op,
                vec![
                    lower_expr_to_value_term(&binary.left)?,
                    lower_expr_to_value_term(&binary.right)?,
                ],
            ))
        }
        Expr::Reference(reference) => {
            let op = if reference.mutability.is_some() {
                "borrow_mut"
            } else {
                "borrow"
            };
            Ok(AlgebraTerm::op(
                op,
                vec![lower_expr_to_value_term(&reference.expr)?],
            ))
        }
        Expr::Cast(cast) => lower_expr_to_value_term(&cast.expr),
        _ => Err(format!("unsupported value expression: {expr:?}")),
    }
}

fn block_single_tail_expr(block: &syn::Block) -> Option<&Expr> {
    match block.stmts.as_slice() {
        [Stmt::Expr(expr, None)] => Some(expr),
        _ => None,
    }
}

fn expr_single_tail_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Block(block) => block_single_tail_expr(&block.block),
        other => Some(other),
    }
}

fn build_bundle_value(s: &ShadowSource) -> Arc<Value> {
    // Collect every arrival's edge memento as a separate object inside
    // the bundle's `arrivals` array. Each carries its own CID (as a
    // sibling field) so consumers can index without re-hashing.
    let arrivals: Vec<Arc<Value>> = s
        .all_arrivals()
        .map(|(_slot, arrival)| {
            let edge_value = edge_memento_value(arrival);
            let edge_cid = cid_of_value(&edge_value);
            Value::object([
                ("cid", Value::string(edge_cid)),
                ("memento", edge_value),
                ("arrivalCid", Value::string(arrival.cid.clone())),
                ("calleeName", Value::string(arrival.callee_name.clone())),
                ("sourceIndex", Value::integer(arrival.source_index as i64)),
            ])
        })
        .collect();

    // Best-effort composed chain: take the longest chain (stable tie-break).
    let composed_chain_value: Arc<Value> = match longest_chain(s) {
        Some(arrivals) if !arrivals.is_empty() => {
            let composed = compose_chain(arrivals.iter().copied());
            let component_cids: Vec<Arc<Value>> = composed
                .component_cids
                .iter()
                .map(|c| Value::string(c.clone()))
                .collect();
            Value::object([
                ("cid", Value::string(composed.cid)),
                ("componentCids", Value::array(component_cids)),
            ])
        }
        _ => Value::null(),
    };

    Value::object([
        ("schemaVersion", Value::string("provekit-walk/1")),
        ("kind", Value::string("walk-bundle")),
        ("shadowSourceCid", Value::string(s.cid.clone())),
        ("fnName", Value::string(s.fn_name.clone())),
        ("slotCount", Value::integer(s.slots.len() as i64)),
        ("arrivals", Value::array(arrivals)),
        ("composedChain", composed_chain_value),
    ])
}

fn longest_chain(s: &ShadowSource) -> Option<Vec<&crate::shadow::ShadowArrival>> {
    // Group arrivals by callee_root_cid and pick the chain with the most
    // arrivals. BTreeMap (sorted by callee_root_cid key) guarantees
    // deterministic iteration order so that when two chains have the same
    // length the FIRST key in lexicographic order wins — result is
    // byte-for-byte identical across calls regardless of HashMap seed.
    use std::collections::BTreeMap;
    let mut chains: BTreeMap<String, Vec<&crate::shadow::ShadowArrival>> = BTreeMap::new();
    for (_, arrival) in s.all_arrivals() {
        chains
            .entry(arrival.callee_root_cid.clone())
            .or_default()
            .push(arrival);
    }
    chains.into_values().max_by_key(|c| c.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atomic_ge, build_shadow_source, const_int, lift_function_precondition, var, CalleeContract,
    };

    fn parse_named(src: &str, name: &str) -> syn::ItemFn {
        let file: syn::File = syn::parse_str(src).unwrap();
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == name => Some(f),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn rust_term_json_round_trips_with_stable_cid() {
        let src = r#"
            fn foo(x: i32) -> i32 { if x == 0 { -22 } else { x } }
        "#;
        let foo_fn = parse_named(src, "foo");
        let bytes = rust_function_term_json(&foo_fn, "foo.rs").unwrap();
        let cid = rust_function_term_json_cid(&foo_fn, "foo.rs").unwrap();
        assert!(cid.starts_with("blake3-512:"));
        assert_eq!(bytes, rust_function_term_json(&foo_fn, "foo.rs").unwrap());
        assert_eq!(cid, rust_function_term_json_cid(&foo_fn, "foo.rs").unwrap());

        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(parsed["kind"].as_str(), Some("rust-algebra-term"));
        assert_eq!(
            parsed["signature_cid"].as_str(),
            Some(crate::signature::RUST_LANGUAGE_SIGNATURE_CID)
        );
        assert_eq!(
            parsed["term_surface"].as_str(),
            Some("seq(if(eq(x, 0), return(neg(22)), skip), return(x))")
        );
        assert_eq!(parsed["term"]["name"].as_str(), Some("seq"));
        assert_eq!(
            parsed["term"]["op_cid"].as_str(),
            crate::signature::op_cid("seq")
        );
    }

    #[test]
    fn rust_term_json_distinct_for_distinct_sources() {
        let src_a = r#"
            fn foo(x: i32) -> i32 { if x == 0 { -22 } else { x } }
        "#;
        let src_b = r#"
            fn foo(x: i32) -> i32 { if x == 1 { -22 } else { x } }
        "#;
        let a_fn = parse_named(src_a, "foo");
        let b_fn = parse_named(src_b, "foo");
        let cid_a = rust_function_term_json_cid(&a_fn, "foo.rs").unwrap();
        let cid_b = rust_function_term_json_cid(&b_fn, "foo.rs").unwrap();
        assert_ne!(cid_a, cid_b);
    }

    #[test]
    fn proof_ir_round_trips_with_stable_cid() {
        let src = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn main() { let y: u32 = 42; let result = f(y); }
        "#;
        let f_fn = parse_named(src, "f");
        let main_fn = parse_named(src, "main");
        let pre = lift_function_precondition(&f_fn);
        let s = build_shadow_source(
            &main_fn,
            &[CalleeContract {
                callee_name: "f".to_string(),
                formal_params: vec!["x".to_string()],
                precondition: pre,
            }],
        );
        let bytes = shadow_to_proof_ir(&s);
        let cid = shadow_proof_ir_cid(&s);
        assert!(!bytes.is_empty());
        assert!(cid.starts_with("blake3-512:"));
        // Stable across calls.
        assert_eq!(bytes, shadow_to_proof_ir(&s));
        assert_eq!(cid, shadow_proof_ir_cid(&s));
        // The bytes should parse as JSON.
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(parsed["schemaVersion"].as_str(), Some("provekit-walk/1"));
        assert_eq!(parsed["shadowSourceCid"].as_str(), Some(s.cid.as_str()));
    }

    #[test]
    fn proof_ir_distinct_for_distinct_sources() {
        let src_a = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn main() { let y: u32 = 42; let result = f(y); }
        "#;
        let src_b = r#"
            fn f(x: u32) -> u32 { if x < 20 { panic!(); } x * 3 }
            fn main() { let y: u32 = 99; let result = f(y); }
        "#;
        let make_bundle = |src: &str| {
            let f_fn = parse_named(src, "f");
            let main_fn = parse_named(src, "main");
            let pre = lift_function_precondition(&f_fn);
            let s = build_shadow_source(
                &main_fn,
                &[CalleeContract {
                    callee_name: "f".to_string(),
                    formal_params: vec!["x".to_string()],
                    precondition: pre,
                }],
            );
            shadow_proof_ir_cid(&s)
        };
        // Suppress unused-helper warning; both calls below.
        let _bare = atomic_ge(var("x"), const_int(10));
        assert_ne!(make_bundle(src_a), make_bundle(src_b));
    }

    // Bug #1: longest_chain must be deterministic when two callees produce
    // chains of equal length. With HashMap (random iteration) the tie-break
    // was non-deterministic; with BTreeMap it picks the lexicographically
    // first key every time.
    #[test]
    fn longest_chain_tie_break_is_deterministic() {
        let src = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn g(y: u32) -> u32 { if y < 5  { panic!(); } y + 1 }
            fn main() {
                let a: u32 = 42;
                let b: u32 = 20;
                let r1 = f(a);
                let r2 = g(b);
            }
        "#;
        let f_fn = parse_named(src, "f");
        let g_fn = parse_named(src, "g");
        let main_fn = parse_named(src, "main");
        let pre_f = lift_function_precondition(&f_fn);
        let pre_g = lift_function_precondition(&g_fn);
        let s = build_shadow_source(
            &main_fn,
            &[
                CalleeContract {
                    callee_name: "f".to_string(),
                    formal_params: vec!["x".to_string()],
                    precondition: pre_f,
                },
                CalleeContract {
                    callee_name: "g".to_string(),
                    formal_params: vec!["y".to_string()],
                    precondition: pre_g,
                },
            ],
        );
        let bytes_first = shadow_to_proof_ir(&s);
        for _ in 0..50 {
            assert_eq!(
                bytes_first,
                shadow_to_proof_ir(&s),
                "bundle bytes must be deterministic across calls (tie-break in longest_chain)"
            );
        }
        let cid_first = shadow_proof_ir_cid(&s);
        for _ in 0..50 {
            assert_eq!(
                cid_first,
                shadow_proof_ir_cid(&s),
                "bundle CID must be deterministic across calls"
            );
        }
    }
}
