//! Symbolic primitives — IR builders for kit-supplied built-in functions.
//!
//! Mirrors `src/ir/symbolic/primitives.ts`. Each function returns an
//! IrTerm or IrFormula; nothing actually computes. Running user invariant
//! code produces the IR directly.

use serde_json::Value as JsonValue;

use crate::types::{lift_to_term, sorts, IrFormula, IrTerm, Liftable, Sort};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Build an Int (integer) or Real (non-integer) constant term, mirroring
/// the TS `num` overload that switches sort by integer-ness.
pub fn num<T: NumLike>(value: T) -> IrTerm {
    value.into_const_term()
}

pub trait NumLike {
    fn into_const_term(self) -> IrTerm;
}

impl NumLike for i64 {
    fn into_const_term(self) -> IrTerm {
        IrTerm::Const {
            value: JsonValue::Number(self.into()),
            sort: sorts::int(),
        }
    }
}
impl NumLike for i32 {
    fn into_const_term(self) -> IrTerm { (self as i64).into_const_term() }
}
impl NumLike for f64 {
    fn into_const_term(self) -> IrTerm {
        // Integer-valued f64 must serialize as integer JSON for cross-
        // language byte-equivalence: JS has no int/float split, so
        // JSON.stringify(3.0) emits "3", and TS num(3.0) produces an Int-
        // sorted const with value 3. Rust's serde_json::Number::from_f64
        // would emit "3.0" instead. Round-trip through i64 when lossless.
        let (value, sort) = if self.fract() == 0.0
            && self.is_finite()
            && (self as i64) as f64 == self
        {
            (JsonValue::Number((self as i64).into()), sorts::int())
        } else {
            (
                serde_json::Number::from_f64(self)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null),
                sorts::real(),
            )
        };
        IrTerm::Const { value, sort }
    }
}

/// Build a Real constant term unconditionally. Note: integer-valued f64
/// inputs serialize as integer JSON (e.g., `real(3.0)` emits `"value": 3`)
/// so the JSON byte-matches TS, even though the sort is Real.
pub fn real(value: f64) -> IrTerm {
    let json_value = if value.fract() == 0.0
        && value.is_finite()
        && (value as i64) as f64 == value
    {
        JsonValue::Number((value as i64).into())
    } else {
        serde_json::Number::from_f64(value)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)
    };
    IrTerm::Const { value: json_value, sort: sorts::real() }
}

/// Build a String constant term.
pub fn str_<S: Into<String>>(value: S) -> IrTerm {
    IrTerm::Const {
        value: JsonValue::String(value.into()),
        sort: sorts::string(),
    }
}

/// Build a Bool constant term.
pub fn bool_(value: bool) -> IrTerm {
    IrTerm::Const {
        value: JsonValue::Bool(value),
        sort: sorts::bool_(),
    }
}

// ---------------------------------------------------------------------------
// ctor helper
// ---------------------------------------------------------------------------

fn ctor(name: &str, args: Vec<IrTerm>, sort: Sort) -> IrTerm {
    IrTerm::Ctor {
        name: name.to_string(),
        args,
        sort,
    }
}

// ---------------------------------------------------------------------------
// Built-in function primitives
// ---------------------------------------------------------------------------

pub fn parse_int(s: IrTerm) -> IrTerm { ctor("parseInt", vec![s], sorts::int()) }
pub fn parse_float(s: IrTerm) -> IrTerm { ctor("parseFloat", vec![s], sorts::real()) }

pub fn is_nan(n: IrTerm) -> IrTerm { ctor("isNaN", vec![n], sorts::bool_()) }
pub fn is_finite(n: IrTerm) -> IrTerm { ctor("isFinite", vec![n], sorts::bool_()) }
pub fn is_integer(n: IrTerm) -> IrTerm { ctor("isInteger", vec![n], sorts::bool_()) }

pub fn abs(n: IrTerm) -> IrTerm {
    let s = n.sort().clone();
    ctor("Math.abs", vec![n], s)
}
pub fn max(a: IrTerm, b: IrTerm) -> IrTerm {
    let s = a.sort().clone();
    ctor("Math.max", vec![a, b], s)
}
pub fn min(a: IrTerm, b: IrTerm) -> IrTerm {
    let s = a.sort().clone();
    ctor("Math.min", vec![a, b], s)
}
pub fn floor(n: IrTerm) -> IrTerm { ctor("Math.floor", vec![n], sorts::int()) }
pub fn ceil(n: IrTerm) -> IrTerm { ctor("Math.ceil", vec![n], sorts::int()) }
pub fn sqrt(n: IrTerm) -> IrTerm { ctor("Math.sqrt", vec![n], sorts::real()) }
pub fn sign(n: IrTerm) -> IrTerm { ctor("Math.sign", vec![n], sorts::int()) }

pub fn string_length(s: IrTerm) -> IrTerm {
    ctor("String.prototype.length", vec![s], sorts::int())
}
pub fn string_includes(s: IrTerm, sub: IrTerm) -> IrTerm {
    ctor("String.prototype.includes", vec![s, sub], sorts::bool_())
}

pub fn array_length(arr: IrTerm) -> IrTerm {
    ctor("Array.prototype.length", vec![arr], sorts::int())
}
pub fn array_includes(arr: IrTerm, item: IrTerm) -> IrTerm {
    ctor("Array.prototype.includes", vec![arr, item], sorts::bool_())
}

// ---------------------------------------------------------------------------
// Term arithmetic — accept Liftable inputs (number or IrTerm)
// ---------------------------------------------------------------------------

pub fn add<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrTerm {
    ctor("+", vec![lift_to_term(a.into()), lift_to_term(b.into())], sorts::int())
}
pub fn sub<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrTerm {
    ctor("-", vec![lift_to_term(a.into()), lift_to_term(b.into())], sorts::int())
}
pub fn mul<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrTerm {
    ctor("*", vec![lift_to_term(a.into()), lift_to_term(b.into())], sorts::int())
}
pub fn div<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrTerm {
    ctor("/", vec![lift_to_term(a.into()), lift_to_term(b.into())], sorts::real())
}
pub fn neg<A: Into<Liftable>>(a: A) -> IrTerm {
    ctor("-", vec![lift_to_term(a.into())], sorts::int())
}

// ---------------------------------------------------------------------------
// Atomic predicates
// ---------------------------------------------------------------------------

fn atom(predicate: &str, args: Vec<IrTerm>) -> IrFormula {
    IrFormula::Atomic {
        predicate: predicate.to_string(),
        args,
    }
}

fn atom_lift<A: Into<Liftable>, B: Into<Liftable>>(predicate: &str, a: A, b: B) -> IrFormula {
    atom(predicate, vec![lift_to_term(a.into()), lift_to_term(b.into())])
}

pub fn eq<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift("=", a, b)
}
pub fn neq<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift("\u{2260}", a, b)
}
pub fn lt<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift("<", a, b)
}
pub fn lte<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift("\u{2264}", a, b)
}
pub fn gt<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift(">", a, b)
}
pub fn gte<A: Into<Liftable>, B: Into<Liftable>>(a: A, b: B) -> IrFormula {
    atom_lift("\u{2265}", a, b)
}

pub fn is_true<A: Into<Liftable>>(b: A) -> IrFormula {
    atom("true", vec![lift_to_term(b.into())])
}
pub fn is_false<A: Into<Liftable>>(b: A) -> IrFormula {
    atom("false", vec![lift_to_term(b.into())])
}
