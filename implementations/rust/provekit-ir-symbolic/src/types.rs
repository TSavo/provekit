//! IR data types. Field order in every struct/enum variant mirrors the
//! TypeScript object-literal order in `src/ir/formulas.ts` so that
//! `serde_json::to_string_pretty(&value)` is byte-equivalent to
//! `JSON.stringify(value, null, 2)` on the TS-produced shape.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Sort {
    #[serde(rename = "primitive")]
    Primitive { name: String },
    #[serde(rename = "set")]
    Set { element: Box<Sort> },
    #[serde(rename = "tuple")]
    Tuple { elements: Vec<Sort> },
    #[serde(rename = "function")]
    Function {
        domain: Vec<Sort>,
        #[serde(rename = "range")]
        range: Box<Sort>,
    },
}

pub mod sorts {
    use super::Sort;

    pub fn primitive(name: &str) -> Sort {
        Sort::Primitive { name: name.to_string() }
    }

    pub fn bool_() -> Sort { primitive("Bool") }
    pub fn int() -> Sort { primitive("Int") }
    pub fn real() -> Sort { primitive("Real") }
    pub fn string() -> Sort { primitive("String") }
    pub fn ref_() -> Sort { primitive("Ref") }
    pub fn node() -> Sort { primitive("Node") }
    pub fn edge() -> Sort { primitive("Edge") }

    pub fn set_of(element: Sort) -> Sort {
        Sort::Set { element: Box::new(element) }
    }

    pub fn tuple_of(elements: Vec<Sort>) -> Sort {
        Sort::Tuple { elements }
    }

    pub fn func_of(domain: Vec<Sort>, range: Sort) -> Sort {
        Sort::Function { domain, range: Box::new(range) }
    }
}

// ---------------------------------------------------------------------------
// IrTerm
// ---------------------------------------------------------------------------
//
// Variant field order matches TS `IrTerm` from formulas.ts:
//   var:   { kind, name, sort }
//   const: { kind, value, sort }
//   ctor:  { kind, name, args, sort }
//
// Const's `value` is `unknown` in TS — Rust uses serde_json::Value, which
// round-trips numbers, strings, bools, and null. BigInt is a known TS
// limitation (JSON.stringify(7n) throws); not handled here either.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IrTerm {
    #[serde(rename = "var")]
    Var { name: String, sort: Sort },
    #[serde(rename = "const")]
    Const { value: JsonValue, sort: Sort },
    #[serde(rename = "ctor")]
    Ctor {
        name: String,
        args: Vec<IrTerm>,
        sort: Sort,
    },
}

impl IrTerm {
    pub fn sort(&self) -> &Sort {
        match self {
            IrTerm::Var { sort, .. } | IrTerm::Const { sort, .. } | IrTerm::Ctor { sort, .. } => sort,
        }
    }

    /// Build an IrTerm::Ctor with the given name, args, and sort.
    /// Convenience used by the extension and bridge factories.
    pub fn ctor(name: &str, args: Vec<IrTerm>, sort: Sort) -> IrTerm {
        IrTerm::Ctor {
            name: name.to_string(),
            args,
            sort,
        }
    }
}

impl IrFormula {
    /// Build an atomic IrFormula with the given predicate and args.
    /// Convenience used by the extension factories.
    pub fn atomic(predicate: &str, args: Vec<IrTerm>) -> IrFormula {
        IrFormula::Atomic {
            predicate: predicate.to_string(),
            args,
        }
    }
}

// ---------------------------------------------------------------------------
// IrFormulaLambda
// ---------------------------------------------------------------------------
//
// TS shape: { kind: "lambda", varName, sort, body }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrFormulaLambda {
    pub kind: LambdaKind,
    #[serde(rename = "varName")]
    pub var_name: String,
    pub sort: Sort,
    pub body: Box<IrFormula>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LambdaKind {
    #[serde(rename = "lambda")]
    Lambda,
}

impl Default for LambdaKind {
    fn default() -> Self { LambdaKind::Lambda }
}

// ---------------------------------------------------------------------------
// IrFormula
// ---------------------------------------------------------------------------
//
// Variant field order matches TS `IrFormula`:
//   forall:  { kind, sort, predicate }
//   exists:  { kind, sort, predicate }
//   and:     { kind, conjuncts }
//   or:      { kind, disjuncts }
//   not:     { kind, body }
//   implies: { kind, antecedent, consequent }
//   atomic:  { kind, predicate, args }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IrFormula {
    #[serde(rename = "forall")]
    Forall { sort: Sort, predicate: IrFormulaLambda },
    #[serde(rename = "exists")]
    Exists { sort: Sort, predicate: IrFormulaLambda },
    #[serde(rename = "and")]
    And { conjuncts: Vec<IrFormula> },
    #[serde(rename = "or")]
    Or { disjuncts: Vec<IrFormula> },
    #[serde(rename = "not")]
    Not { body: Box<IrFormula> },
    #[serde(rename = "implies")]
    Implies {
        antecedent: Box<IrFormula>,
        consequent: Box<IrFormula>,
    },
    #[serde(rename = "atomic")]
    Atomic { predicate: String, args: Vec<IrTerm> },
}

// ---------------------------------------------------------------------------
// BindingScope (parallel to TS for completeness; not load-bearing for the
// symbolic-primitives surface, but kits will need it.)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum BindingScope {
    #[serde(rename = "function")]
    Function { name: String },
    #[serde(rename = "module")]
    Module { path: String },
    #[serde(rename = "class")]
    Class { name: String },
    #[serde(rename = "method")]
    Method {
        #[serde(rename = "className")]
        class_name: String,
        #[serde(rename = "methodName")]
        method_name: String,
    },
    #[serde(rename = "region")]
    Region { start: String, end: String },
    #[serde(rename = "transition")]
    Transition { name: String },
    #[serde(rename = "whenever")]
    Whenever { predicate: IrFormula },
}

// ---------------------------------------------------------------------------
// Lift helper — mirrors TS `liftToTerm`
// ---------------------------------------------------------------------------

/// A value that can be lifted into an IrTerm. Mirrors the TS Liftable union.
#[derive(Debug, Clone)]
pub enum Liftable {
    Term(IrTerm),
    Int(i64),
    Real(f64),
    String(String),
    Bool(bool),
    Null,
}

impl From<IrTerm> for Liftable {
    fn from(t: IrTerm) -> Self { Liftable::Term(t) }
}
impl From<&IrTerm> for Liftable {
    fn from(t: &IrTerm) -> Self { Liftable::Term(t.clone()) }
}
impl From<i64> for Liftable {
    fn from(v: i64) -> Self { Liftable::Int(v) }
}
impl From<i32> for Liftable {
    fn from(v: i32) -> Self { Liftable::Int(v as i64) }
}
impl From<f64> for Liftable {
    fn from(v: f64) -> Self { Liftable::Real(v) }
}
impl From<&str> for Liftable {
    fn from(v: &str) -> Self { Liftable::String(v.to_string()) }
}
impl From<String> for Liftable {
    fn from(v: String) -> Self { Liftable::String(v) }
}
impl From<bool> for Liftable {
    fn from(v: bool) -> Self { Liftable::Bool(v) }
}

pub fn lift_to_term(v: Liftable) -> IrTerm {
    match v {
        Liftable::Term(t) => t,
        Liftable::Int(n) => IrTerm::Const {
            value: JsonValue::Number(n.into()),
            sort: sorts::int(),
        },
        Liftable::Real(n) => IrTerm::Const {
            value: serde_json::Number::from_f64(n)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null),
            sort: sorts::int(), // mirrors TS liftToTerm: any `number` -> Int sort
        },
        Liftable::String(s) => IrTerm::Const {
            value: JsonValue::String(s),
            sort: sorts::string(),
        },
        Liftable::Bool(b) => IrTerm::Const {
            value: JsonValue::Bool(b),
            sort: sorts::bool_(),
        },
        Liftable::Null => IrTerm::Const {
            value: JsonValue::Null,
            sort: sorts::ref_(),
        },
    }
}
