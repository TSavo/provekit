// SPDX-License-Identifier: Apache-2.0
//
// OpacityManifest emission for the bundled SMT-LIB v2.6 compiler.
// Spec: protocol/specs/2026-05-02-opacity-manifest-grammar.md.
//
// The SMT-LIB target theory cannot soundly translate two IR shapes:
//
//   1. A `Lambda`/`Forall`/`Exists`/`Choice`/`Const` whose sort is
//      `Sort::Function`. SMT-LIB v2.6 has no first-class predicate
//      quantification, so we mark the *parent* IR node opaque with
//      reasonCode `"predicate_quantification"` (spec §4).
//
//   2. A `Lambda`/`Forall`/`Exists`/`Choice`/`Const` whose sort is
//      `Sort::Dependent`. SMT-LIB has no value-dependent types, so we
//      mark the *parent* IR node opaque with reasonCode
//      `"dependent_type"` (spec §4).
//
// **Granularity rule.** Per spec §3, the compiler is the authority on
// granularity. We pick the parent term/formula node (not the bare
// sort) because §4's reason-code definitions and §8.1's worked example
// both describe an opaque *Lambda*, not an opaque sort. The
// position-CID is `BLAKE3-512(JCS(parent_subterm))`.
//
// `emit_sort()` therefore never sees a `Function`/`Dependent` arm at
// runtime; we keep the arm but make it `unreachable!()` with a
// pointer to this comment. If the invariant ever breaks, tests catch
// it before users do.
//
// All bytes here are deterministic across runs:
//   - `opacities` sorted by positionCid asc, then reasonCode asc.
//   - JSON serialization through `provekit-canonicalizer` (RFC 8785).
//   - No timestamps, no PIDs, no source paths.

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{Formula, Sort, Term};
use serde::{Deserialize, Serialize};

use crate::{COMPILER_NAME, COMPILER_VERSION};

/// `ir-compiler-protocol/2` is the OpacityManifest's protocol
/// identifier (distinct from the `provekit-ir-compiler/1` trait
/// constant; see spec §0 + §2).
pub const OPACITY_PROTOCOL_VERSION: &str = "ir-compiler-protocol/2";

/// One opaque-position entry.
///
/// Wire shape per spec §2: `{ "positionCid": String, "reasonCode": String }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpacityEntry {
    #[serde(rename = "positionCid")]
    pub position_cid: String,
    #[serde(rename = "reasonCode")]
    pub reason_code: String,
}

/// The full OpacityManifest emitted alongside a compiled formula.
///
/// Wire shape per spec §2:
/// ```text
/// {
///   "protocolVersion": "ir-compiler-protocol/2",
///   "compiler": String,
///   "compilerVersion": String,
///   "opacities": [Opacity, ...]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpacityManifest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub compiler: String,
    #[serde(rename = "compilerVersion")]
    pub compiler_version: String,
    pub opacities: Vec<OpacityEntry>,
}

impl OpacityManifest {
    /// Empty manifest skeleton (no opacities).
    pub fn empty() -> Self {
        Self {
            protocol_version: OPACITY_PROTOCOL_VERSION.to_string(),
            compiler: COMPILER_NAME.to_string(),
            compiler_version: COMPILER_VERSION.to_string(),
            opacities: Vec::new(),
        }
    }

    /// Build from collected opacity entries. Sorts entries per spec
    /// §2.3 (positionCid asc, reasonCode asc).
    pub fn from_entries(mut entries: Vec<OpacityEntry>) -> Self {
        entries.sort_by(|a, b| {
            a.position_cid
                .cmp(&b.position_cid)
                .then_with(|| a.reason_code.cmp(&b.reason_code))
        });
        Self {
            protocol_version: OPACITY_PROTOCOL_VERSION.to_string(),
            compiler: COMPILER_NAME.to_string(),
            compiler_version: COMPILER_VERSION.to_string(),
            opacities: entries,
        }
    }

    /// JCS-canonical bytes of the manifest. Deterministic.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let v = manifest_to_canonicalizer_value(self);
        encode_jcs(&v).into_bytes()
    }
}

// --- Reason codes (closed enum from spec §4) -----------------------------

/// Reason: SMT-LIB cannot quantify over predicates / function sorts.
pub const REASON_PREDICATE_QUANTIFICATION: &str = "predicate_quantification";

/// Reason: SMT-LIB has no dependent types.
pub const REASON_DEPENDENT_TYPE: &str = "dependent_type";

// --- Position CID computation -------------------------------------------

/// Compute `positionCid = "blake3-512:" || hex(BLAKE3-512(JCS(node)))`
/// for a JSON-serialized IR subterm. Per spec §3 the bytes hashed are
/// the JCS canonicalization of the IR-JSON node.
fn position_cid_for_value(json_value: &serde_json::Value) -> String {
    let v = serde_json_to_canonicalizer_value(json_value);
    let canonical = encode_jcs(&v);
    blake3_512_of(canonical.as_bytes())
}

/// Position CID for an `IrTerm`. Serializes via serde to IR-JSON, then
/// JCS-canonicalizes, then BLAKE3-512s. Panics only if the IR term
/// cannot be serialized to JSON (which should be impossible for valid
/// IR per the type system).
pub fn position_cid_for_term(term: &Term) -> String {
    let json =
        serde_json::to_value(term).expect("IR Term always serializes to JSON");
    position_cid_for_value(&json)
}

/// Position CID for an `IrFormula`.
pub fn position_cid_for_formula(formula: &Formula) -> String {
    let json = serde_json::to_value(formula)
        .expect("IR Formula always serializes to JSON");
    position_cid_for_value(&json)
}

// --- Opacity collection: walks the IR, finds untranslatable sorts -------

/// Classify a sort: returns `Some(reason_code)` iff the sort makes the
/// parent node opaque under SMT-LIB. `None` means the parent is fine.
pub fn classify_sort(sort: &Sort) -> Option<&'static str> {
    match sort {
        Sort::Primitive { .. } => None,
        Sort::Function { .. } => Some(REASON_PREDICATE_QUANTIFICATION),
        Sort::Dependent { .. } => Some(REASON_DEPENDENT_TYPE),
    }
}

/// Walk a `Term` and append opacity entries for every parent node
/// whose sort SMT-LIB cannot soundly translate.
pub fn collect_opacity_term(term: &Term, out: &mut Vec<OpacityEntry>) {
    match term {
        Term::Var { .. } => {}
        Term::Const { sort, .. } => {
            if let Some(reason) = classify_sort(sort) {
                out.push(OpacityEntry {
                    position_cid: position_cid_for_term(term),
                    reason_code: reason.to_string(),
                });
            }
        }
        Term::Ctor { args, .. } => {
            for a in args {
                collect_opacity_term(a, out);
            }
        }
        Term::Lambda { param_sort, body, .. } => {
            if let Some(reason) = classify_sort(param_sort) {
                out.push(OpacityEntry {
                    position_cid: position_cid_for_term(term),
                    reason_code: reason.to_string(),
                });
                // We do NOT recurse into the body when the parent
                // itself is opaque: the parent's bytes already cover
                // the whole subtree at the spec level (§3 invariant).
            } else {
                collect_opacity_term(body, out);
            }
        }
        Term::Let { bindings, body, .. } => {
            for b in bindings {
                collect_opacity_term(&b.bound_term, out);
            }
            collect_opacity_term(body, out);
        }
    }
}

/// Walk a `Formula` and append opacity entries for every parent node
/// whose sort SMT-LIB cannot soundly translate.
pub fn collect_opacity_formula(formula: &Formula, out: &mut Vec<OpacityEntry>) {
    match formula {
        Formula::Atomic { args, .. } => {
            for a in args {
                collect_opacity_term(a, out);
            }
        }
        Formula::And { operands }
        | Formula::Or { operands }
        | Formula::Not { operands }
        | Formula::Implies { operands } => {
            for o in operands {
                collect_opacity_formula(o, out);
            }
        }
        Formula::Forall { sort, body, .. }
        | Formula::Exists { sort, body, .. }
        | Formula::Choice { sort, body, .. } => {
            if let Some(reason) = classify_sort(sort) {
                out.push(OpacityEntry {
                    position_cid: position_cid_for_formula(formula),
                    reason_code: reason.to_string(),
                });
                // Skip recursion into the body — opacity at the
                // parent already covers the subtree.
            } else {
                collect_opacity_formula(body, out);
            }
        }
    }
}

/// Build the OpacityManifest for a top-level Formula.
pub fn manifest_for_formula(formula: &Formula) -> OpacityManifest {
    let mut entries = Vec::new();
    collect_opacity_formula(formula, &mut entries);
    OpacityManifest::from_entries(entries)
}

// --- Conversion helpers (serde_json -> canonicalizer::Value) -------------

/// Convert a `serde_json::Value` into the canonicalizer's `Value`.
///
/// The canonicalizer carries `i64` integers; non-integer JSON numbers
/// (floats, NaN, Infinity, u64 > i64::MAX) violate the IR's
/// integer-only number rule and CANNOT be canonically represented in
/// the substrate's Value type. Silent stringification would produce
/// distinct positionCids across implementations for the same logical
/// IR subtree (one impl stringifies, another impl might error or fail
/// the JSON schema check), breaking cross-implementation
/// byte-equivalence.
///
/// Per Supra omnia, rectum: fail loud at the first unrepresentable
/// number rather than corrupt the cross-impl CID. The compile aborts
/// with a clear message naming the offending number; the caller fixes
/// the upstream IR-JSON or extends the canonicalizer.
fn serde_json_to_canonicalizer_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(u) = n.as_u64() {
                if u <= i64::MAX as u64 {
                    Value::Integer(u as i64)
                } else {
                    panic!(
                        "opacity positionCid: u64 number {u} exceeds i64::MAX; \
                         IR violates integer-only number rule. Cross-impl \
                         positionCid would diverge if silently stringified. \
                         Fix the upstream IR-JSON or extend the canonicalizer."
                    );
                }
            } else {
                // Float / NaN / Infinity — JSON numbers that aren't
                // integers. The canonicalizer's Value doesn't carry
                // them; any mapping is implementation-defined and
                // breaks cross-impl byte-equivalence.
                panic!(
                    "opacity positionCid: non-integer JSON number {n} \
                     cannot be canonically represented. IR violates \
                     integer-only number rule. Cross-impl positionCid \
                     would diverge if silently stringified. Fix the \
                     upstream IR-JSON or extend the canonicalizer."
                );
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(std::sync::Arc::new(
                    serde_json_to_canonicalizer_value(it),
                ));
            }
            Value::Array(out)
        }
        serde_json::Value::Object(map) => {
            let mut out: Vec<(String, std::sync::Arc<Value>)> =
                Vec::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.push((
                    k.clone(),
                    std::sync::Arc::new(serde_json_to_canonicalizer_value(v)),
                ));
            }
            Value::Object(out)
        }
    }
}

/// Build a canonicalizer `Value` directly for an OpacityManifest, so
/// we never depend on serde-json field ordering.
fn manifest_to_canonicalizer_value(m: &OpacityManifest) -> Value {
    use std::sync::Arc;
    let opacities: Vec<Arc<Value>> = m
        .opacities
        .iter()
        .map(|e| {
            Arc::new(Value::Object(vec![
                (
                    "positionCid".to_string(),
                    Arc::new(Value::String(e.position_cid.clone())),
                ),
                (
                    "reasonCode".to_string(),
                    Arc::new(Value::String(e.reason_code.clone())),
                ),
            ]))
        })
        .collect();

    Value::Object(vec![
        (
            "protocolVersion".to_string(),
            Arc::new(Value::String(m.protocol_version.clone())),
        ),
        (
            "compiler".to_string(),
            Arc::new(Value::String(m.compiler.clone())),
        ),
        (
            "compilerVersion".to_string(),
            Arc::new(Value::String(m.compiler_version.clone())),
        ),
        ("opacities".to_string(), std::sync::Arc::new(Value::Array(opacities))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_manifest_canonical_bytes_stable() {
        let m = OpacityManifest::empty();
        let bytes = m.to_canonical_bytes();
        let s = std::str::from_utf8(&bytes).unwrap();
        // JCS sorts keys: compiler, compilerVersion, opacities, protocolVersion.
        assert!(s.starts_with(r#"{"compiler":"smt-lib-reference""#), "{}", s);
        assert!(s.contains(r#""opacities":[]"#), "{}", s);
        assert!(s.contains(r#""protocolVersion":"ir-compiler-protocol/2""#));
    }

    #[test]
    fn entries_sort_by_position_cid_then_reason() {
        // Two synthetic entries with the same positionCid but different
        // reason codes — exercise the secondary sort key.
        let entries = vec![
            OpacityEntry {
                position_cid: "blake3-512:bb".to_string(),
                reason_code: "predicate_quantification".to_string(),
            },
            OpacityEntry {
                position_cid: "blake3-512:aa".to_string(),
                reason_code: "dependent_type".to_string(),
            },
            OpacityEntry {
                position_cid: "blake3-512:aa".to_string(),
                reason_code: "predicate_quantification".to_string(),
            },
        ];
        let m = OpacityManifest::from_entries(entries);
        assert_eq!(m.opacities[0].position_cid, "blake3-512:aa");
        assert_eq!(m.opacities[0].reason_code, "dependent_type");
        assert_eq!(m.opacities[1].position_cid, "blake3-512:aa");
        assert_eq!(m.opacities[1].reason_code, "predicate_quantification");
        assert_eq!(m.opacities[2].position_cid, "blake3-512:bb");
    }

    #[test]
    fn classify_sort_returns_correct_codes() {
        let prim: Sort = serde_json::from_value(json!({
            "kind": "primitive", "name": "Int"
        }))
        .unwrap();
        assert_eq!(classify_sort(&prim), None);

        let func: Sort = serde_json::from_value(json!({
            "kind": "function",
            "args": [{"kind": "primitive", "name": "Int"}],
            "return": {"kind": "primitive", "name": "Bool"}
        }))
        .unwrap();
        assert_eq!(classify_sort(&func), Some(REASON_PREDICATE_QUANTIFICATION));

        let dep: Sort = serde_json::from_value(json!({
            "kind": "dependent",
            "name": "Vec",
            "indexVar": "n",
            "indexSort": {"kind": "primitive", "name": "Int"}
        }))
        .unwrap();
        assert_eq!(classify_sort(&dep), Some(REASON_DEPENDENT_TYPE));
    }

    #[test]
    fn position_cid_is_blake3_512_of_jcs() {
        // Synthesize a Const term with a function sort, hash it, check
        // the prefix and length.
        let t: Term = serde_json::from_value(json!({
            "kind": "const",
            "value": 0,
            "sort": {
                "kind": "function",
                "args": [{"kind": "primitive", "name": "Int"}],
                "return": {"kind": "primitive", "name": "Bool"}
            }
        }))
        .unwrap();
        let cid = position_cid_for_term(&t);
        assert!(cid.starts_with("blake3-512:"));
        assert_eq!(cid.len(), "blake3-512:".len() + 128);
    }
}
