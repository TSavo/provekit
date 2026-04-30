// SPDX-License-Identifier: Apache-2.0
//
// IR-JSON serializer. Emits the locked IR-JSON shape per
// protocol/specs/2026-04-30-ir-formal-grammar.md.
//
// Locked key orders (the kit emits insertion order; the canonicalizer's
// JCS pass re-sorts before hashing):
//   var:        {kind, name}
//   const:      {kind, value, sort}
//   ctor:       {kind, name, args}
//   atomic:     {kind, name, args}
//   connective: {kind, operands}
//   quantifier: {kind, name, sort, body}
//   sort:       {kind: "primitive", name}
//   contract:   {kind: "contract", name, outBinding, pre?, post?, inv?}
//
// Output is a String holding kit-shape JSON. The `to_value()` form is
// also provided for hashing flow (claim-envelope JCS-encodes the
// canonicalizer Value tree, not the kit JSON string).

use std::sync::Arc;

use provekit_canonicalizer::Value;

use crate::{ConstValue, ContractDecl, Formula, Sort, Term};

// -------- to canonicalizer Value (used by hashing path) --------------

pub fn sort_to_value(s: &Sort) -> Arc<Value> {
    Value::object([
        ("kind", Value::string("primitive")),
        ("name", Value::string(s.name.clone())),
    ])
}

pub fn term_to_value(t: &Term) -> Arc<Value> {
    match t {
        Term::Var { name } => Value::object([
            ("kind", Value::string("var")),
            ("name", Value::string(name.clone())),
        ]),
        Term::Const { value, sort } => {
            let value_v = match value {
                ConstValue::Int(n) => Value::integer(*n),
                ConstValue::String(s) => Value::string(s.clone()),
                ConstValue::Bool(b) => Value::boolean(*b),
            };
            Value::object([
                ("kind", Value::string("const")),
                ("value", value_v),
                ("sort", sort_to_value(sort)),
            ])
        }
        Term::Ctor { name, args } => {
            let arr: Vec<Arc<Value>> = args.iter().map(|a| term_to_value(a)).collect();
            Value::object([
                ("kind", Value::string("ctor")),
                ("name", Value::string(name.clone())),
                ("args", Value::array(arr)),
            ])
        }
    }
}

pub fn formula_to_value(f: &Formula) -> Arc<Value> {
    match f {
        Formula::Atomic { name, args } => {
            let arr: Vec<Arc<Value>> = args.iter().map(|a| term_to_value(a)).collect();
            Value::object([
                ("kind", Value::string("atomic")),
                ("name", Value::string(name.clone())),
                ("args", Value::array(arr)),
            ])
        }
        Formula::Connective { kind, operands } => {
            let arr: Vec<Arc<Value>> = operands.iter().map(|o| formula_to_value(o)).collect();
            Value::object([
                ("kind", Value::string(kind.clone())),
                ("operands", Value::array(arr)),
            ])
        }
        Formula::Quantifier {
            kind,
            name,
            sort,
            body,
        } => Value::object([
            ("kind", Value::string(kind.clone())),
            ("name", Value::string(name.clone())),
            ("sort", sort_to_value(sort)),
            ("body", formula_to_value(body)),
        ]),
    }
}

// -------- to kit-shape JSON (insertion-order; mirrors C++ kit) -------

pub fn marshal_declarations(decls: &[ContractDecl]) -> String {
    // Mirrors C++ marshal_declarations: emits insertion-order JSON.
    let mut out = String::new();
    out.push('[');
    for (i, d) in decls.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(r#"{"kind":"contract","name":"#);
        write_string(&mut out, &d.name);
        out.push_str(r#","outBinding":"#);
        write_string(&mut out, &d.out_binding);
        if let Some(pre) = &d.pre {
            out.push_str(r#","pre":"#);
            write_formula(&mut out, pre);
        }
        if let Some(post) = &d.post {
            out.push_str(r#","post":"#);
            write_formula(&mut out, post);
        }
        if let Some(inv) = &d.inv {
            out.push_str(r#","inv":"#);
            write_formula(&mut out, inv);
        }
        out.push('}');
    }
    out.push(']');
    out
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for b in s.as_bytes() {
        let c = *b;
        match c {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x00..=0x1F => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(HEX[((c >> 4) & 0xF) as usize] as char);
                out.push(HEX[(c & 0xF) as usize] as char);
            }
            _ => out.push(c as char),
        }
    }
    out.push('"');
}

fn write_sort(out: &mut String, s: &Sort) {
    out.push_str(r#"{"kind":"primitive","name":"#);
    write_string(out, &s.name);
    out.push('}');
}

fn write_term(out: &mut String, t: &Term) {
    match t {
        Term::Var { name } => {
            out.push_str(r#"{"kind":"var","name":"#);
            write_string(out, name);
            out.push('}');
        }
        Term::Const { value, sort } => {
            out.push_str(r#"{"kind":"const","value":"#);
            match value {
                ConstValue::Int(n) => out.push_str(&n.to_string()),
                ConstValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
                ConstValue::String(s) => write_string(out, s),
            }
            out.push_str(r#","sort":"#);
            write_sort(out, sort);
            out.push('}');
        }
        Term::Ctor { name, args } => {
            out.push_str(r#"{"kind":"ctor","name":"#);
            write_string(out, name);
            out.push_str(r#","args":["#);
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_term(out, a);
            }
            out.push_str("]}");
        }
    }
}

fn write_formula(out: &mut String, f: &Formula) {
    match f {
        Formula::Atomic { name, args } => {
            out.push_str(r#"{"kind":"atomic","name":"#);
            write_string(out, name);
            out.push_str(r#","args":["#);
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_term(out, a);
            }
            out.push_str("]}");
        }
        Formula::Connective { kind, operands } => {
            out.push_str(r#"{"kind":"#);
            write_string(out, kind);
            out.push_str(r#","operands":["#);
            for (i, op) in operands.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_formula(out, op);
            }
            out.push_str("]}");
        }
        Formula::Quantifier {
            kind,
            name,
            sort,
            body,
        } => {
            out.push_str(r#"{"kind":"#);
            write_string(out, kind);
            out.push_str(r#","name":"#);
            write_string(out, name);
            out.push_str(r#","sort":"#);
            write_sort(out, sort);
            out.push_str(r#","body":"#);
            write_formula(out, body);
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn marshal_simple_contract() {
        reset_collector();
        must("parseInt", forall(Int(), |n| gt(n, num(0))));
        let decls = finish();
        let s = marshal_declarations(&decls);
        // Insertion-order: kind, name, outBinding, then pre.
        assert!(s.contains(r#""kind":"contract""#));
        assert!(s.contains(r#""name":"parseInt""#));
        assert!(s.contains(r#""outBinding":"out""#));
        assert!(s.contains(r#""pre":{"kind":"forall""#));
        assert!(s.contains(r#""kind":"atomic""#));
        assert!(s.contains(r#""name":">""#));
    }

    #[test]
    fn formula_to_value_round_trips_through_jcs() {
        // Build a forall(Int, n -> n > 0) and JCS-encode the canonical Value.
        let f = forall(Int(), |n| gt(n, num(0)));
        let v = formula_to_value(&f);
        let s = provekit_canonicalizer::encode_jcs(&v);
        // JCS sorts: args, body, kind, name, operands, sort. Top-level
        // forall has keys body, kind, name, sort.
        assert!(s.starts_with('{'));
        assert!(s.contains(r#""kind":"forall""#));
        assert!(s.contains(r#""kind":"atomic""#));
    }
}
