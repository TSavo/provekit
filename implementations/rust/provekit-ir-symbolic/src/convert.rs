// SPDX-License-Identifier: Apache-2.0
//
// provekit-ir-symbolic ↔ provekit-ir-types conversions.
//
// The symbolic kit uses `Rc` for authoring convenience; the generated
// compilers expect owned `Box`/`Vec` shapes from `provekit-ir-types`.
// These functions bridge the two representations.

use std::rc::Rc;

use provekit_ir_types as ir;

use crate::{ConstValue, Formula, LetBinding, Sort, Term};

// ---------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------

impl From<Sort> for ir::Sort {
    fn from(s: Sort) -> Self {
        ir::Sort::Primitive { name: s.name }
    }
}

impl From<&Sort> for ir::Sort {
    fn from(s: &Sort) -> Self {
        ir::Sort::Primitive {
            name: s.name.clone(),
        }
    }
}

impl From<ir::Sort> for Sort {
    fn from(s: ir::Sort) -> Self {
        match s {
            ir::Sort::Primitive { name } => Sort { name },
            // The symbolic-side `Sort` wrapper is primitive-only; it does
            // not yet model Function/Dependent. Deferred to #331 (Coq) /
            // #332 (SMT-LIB) along with the rest of the v1.5.0 grammar
            // grow downstream of the canonical `provekit-ir-types::Sort`.
            ir::Sort::Function { .. }
            | ir::Sort::Dependent { .. }
            | ir::Sort::Float { .. }
            | ir::Sort::Region { .. } => {
                unimplemented!(
                    "FunctionSort/DependentSort/FloatSort/RegionSort not supported in symbolic Sort wrapper: \
                     deferred to #331 (Coq) / #332 (SMT-LIB) / #401 (Region)"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ConstValue ↔ serde_json::Value
// ---------------------------------------------------------------------------

impl From<ConstValue> for serde_json::Value {
    fn from(v: ConstValue) -> Self {
        match v {
            ConstValue::Int(n) => serde_json::Value::Number(n.into()),
            ConstValue::String(s) => serde_json::Value::String(s),
            ConstValue::Bool(b) => serde_json::Value::Bool(b),
        }
    }
}

impl From<&ConstValue> for serde_json::Value {
    fn from(v: &ConstValue) -> Self {
        match v {
            ConstValue::Int(n) => serde_json::Value::Number((*n).into()),
            ConstValue::String(s) => serde_json::Value::String(s.clone()),
            ConstValue::Bool(b) => serde_json::Value::Bool(*b),
        }
    }
}

impl TryFrom<serde_json::Value> for ConstValue {
    type Error = String;
    fn try_from(v: serde_json::Value) -> Result<Self, Self::Error> {
        match v {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(ConstValue::Int)
                .ok_or_else(|| format!("ConstValue::Int expected, got {n}")),
            serde_json::Value::String(s) => Ok(ConstValue::String(s)),
            serde_json::Value::Bool(b) => Ok(ConstValue::Bool(b)),
            other => Err(format!(
                "ConstValue expected number/string/bool, got {other}"
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Term
// ---------------------------------------------------------------------------

pub fn term_to_ir(t: &Term) -> ir::Term {
    match t {
        Term::Var { name } => ir::Term::Var { name: name.clone() },
        Term::Const { value, sort } => ir::Term::Const {
            value: value.into(),
            sort: sort.into(),
        },
        Term::Ctor { name, args } => ir::Term::Ctor {
            name: name.clone(),
            args: args.iter().map(|a| term_to_ir(a)).collect(),
        },
        Term::Lambda {
            param_name,
            param_sort,
            body,
        } => ir::Term::Lambda {
            param_name: param_name.clone(),
            param_sort: param_sort.into(),
            body: Box::new(term_to_ir(body)),
        },
        Term::Let { bindings, body } => ir::Term::Let {
            bindings: bindings.iter().map(binding_to_ir).collect(),
            body: Box::new(term_to_ir(body)),
        },
    }
}

pub fn term_from_ir(t: ir::Term) -> Term {
    match t {
        ir::Term::Var { name, .. } => Term::Var { name },
        ir::Term::Const { value, sort } => Term::Const {
            value: value.try_into().expect("valid const value"),
            sort: sort.into(),
        },
        ir::Term::Ctor { name, args } => Term::Ctor {
            name,
            args: args.into_iter().map(|a| Rc::new(term_from_ir(a))).collect(),
        },
        ir::Term::Lambda {
            param_name,
            param_sort,
            body,
        } => Term::Lambda {
            param_name,
            param_sort: param_sort.into(),
            body: Rc::new(term_from_ir(*body)),
        },
        ir::Term::Let { bindings, body } => Term::Let {
            bindings: bindings.into_iter().map(binding_from_ir).collect(),
            body: Rc::new(term_from_ir(*body)),
        },
    }
}

// ---------------------------------------------------------------------------
// LetBinding
// ---------------------------------------------------------------------------

pub fn binding_to_ir(b: &LetBinding) -> ir::LetBinding {
    ir::LetBinding {
        name: b.name.clone(),
        bound_term: term_to_ir(&b.bound_term),
    }
}

pub fn binding_from_ir(b: ir::LetBinding) -> LetBinding {
    LetBinding {
        name: b.name,
        bound_term: Rc::new(term_from_ir(b.bound_term)),
    }
}

// ---------------------------------------------------------------------------
// Formula
// ---------------------------------------------------------------------------

pub fn formula_to_ir(f: &Formula) -> ir::Formula {
    match f {
        Formula::Atomic { name, args } => ir::Formula::Atomic {
            name: name.clone(),
            args: args.iter().map(|a| term_to_ir(a)).collect(),
        },
        Formula::Connective { kind, operands } => match kind.as_str() {
            "and" => ir::Formula::And {
                operands: operands.iter().map(|o| formula_to_ir(o)).collect(),
            },
            "or" => ir::Formula::Or {
                operands: operands.iter().map(|o| formula_to_ir(o)).collect(),
            },
            "not" => ir::Formula::Not {
                operands: operands.iter().map(|o| formula_to_ir(o)).collect(),
            },
            "implies" => ir::Formula::Implies {
                operands: operands.iter().map(|o| formula_to_ir(o)).collect(),
            },
            _ => panic!("unknown connective kind: {kind}"),
        },
        Formula::Quantifier {
            kind,
            name,
            sort,
            body,
        } => match kind.as_str() {
            "forall" => ir::Formula::Forall {
                name: name.clone(),
                sort: sort.into(),
                body: Box::new(formula_to_ir(body)),
            },
            "exists" => ir::Formula::Exists {
                name: name.clone(),
                sort: sort.into(),
                body: Box::new(formula_to_ir(body)),
            },
            _ => panic!("unknown quantifier kind: {kind}"),
        },
        Formula::Choice {
            var_name,
            sort,
            body,
        } => ir::Formula::Choice {
            var_name: var_name.clone(),
            sort: sort.into(),
            body: Box::new(formula_to_ir(body)),
        },
    }
}

pub fn formula_from_ir(f: ir::Formula) -> Formula {
    match f {
        ir::Formula::Atomic { name, args } => Formula::Atomic {
            name,
            args: args.into_iter().map(|a| Rc::new(term_from_ir(a))).collect(),
        },
        ir::Formula::And { operands } => Formula::Connective {
            kind: "and".into(),
            operands: operands
                .into_iter()
                .map(|o| Rc::new(formula_from_ir(o)))
                .collect(),
        },
        ir::Formula::Or { operands } => Formula::Connective {
            kind: "or".into(),
            operands: operands
                .into_iter()
                .map(|o| Rc::new(formula_from_ir(o)))
                .collect(),
        },
        ir::Formula::Not { operands } => Formula::Connective {
            kind: "not".into(),
            operands: operands
                .into_iter()
                .map(|o| Rc::new(formula_from_ir(o)))
                .collect(),
        },
        ir::Formula::Implies { operands } => Formula::Connective {
            kind: "implies".into(),
            operands: operands
                .into_iter()
                .map(|o| Rc::new(formula_from_ir(o)))
                .collect(),
        },
        ir::Formula::Forall { name, sort, body } => Formula::Quantifier {
            kind: "forall".into(),
            name,
            sort: sort.into(),
            body: Rc::new(formula_from_ir(*body)),
        },
        ir::Formula::Exists { name, sort, body } => Formula::Quantifier {
            kind: "exists".into(),
            name,
            sort: sort.into(),
            body: Rc::new(formula_from_ir(*body)),
        },
        ir::Formula::Choice {
            var_name,
            sort,
            body,
        } => Formula::Choice {
            var_name,
            sort: sort.into(),
            body: Rc::new(formula_from_ir(*body)),
        },
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // `substitute` / `apply` appear only inside an unreduced `wp_rule`
        // term. They are eliminated by `libprovekit::wp` before any formula
        // is converted into the symbolic representation; the symbolic engine
        // has no equivalent and is not meant to see them. Reaching this arm
        // is a bug.
        ir::Formula::Substitute { .. } | ir::Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached ir-symbolic formula conversion; \
                 must be reduced via libprovekit::wp first"
            )
        }
        ir::Formula::DivergenceBetween { .. } => {
            unreachable!(
                "platform divergence formula reached ir-symbolic formula conversion; \
                 stage 4 must lower it before symbolic conversion"
            )
        }
    }
}
