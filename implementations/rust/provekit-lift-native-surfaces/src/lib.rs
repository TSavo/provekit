// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-native-surfaces
//
// v0 lifters for per-language native contract annotation surfaces.
// Each lifter walks the source text of a file via a regex pre-pass and emits
// `EvidenceMemento` records with `source_kind = "native-surface"`.
//
// Covered surfaces (v0):
//   - Spring (Java)    — @PreCondition, @PostCondition, @NotNull, @Min, @Max, @Size
//   - pydantic (Python) — Field(ge=, le=, gt=, lt=), deal.pre, deal.post
//   - Zod (TypeScript) — z.number().min/max, z.string().min/max, z.object chain
//
// v0 philosophy: honest under-coverage beats polluting the lattice.
//   Static annotations (e.g. @NotNull, @Min(0)) → confidence_basis_points = 10000.
//   Opaque/partial predicates (regex string patterns, deal lambdas) → ~5000.
//   If we cannot extract a structured IrFormula predicate, we emit an
//   IrFormula::Atomic with name "opaque" and args = [IrTerm::Const(text)].

use std::collections::BTreeMap;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{
    EvidenceMemento, IrFormula, IrTerm, Sort, SourceKind, SourceLocator, SourceLocatorPoint,
    SourceLocatorSpan,
};

pub mod pydantic;
pub mod spring;
pub mod zod;

pub use pydantic::lift_pydantic_file;
pub use spring::lift_spring_file;
pub use zod::lift_zod_file;

// ---------------------------------------------------------------------------
// Sentinel lifter CID — used until PR-F wires the real lifter CID.
// ---------------------------------------------------------------------------
pub const AUTO_PROMOTE_LIFTER_CID: &str = concat!(
    "blake3-512:",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
);

// ---------------------------------------------------------------------------
// IrFormula constructors used by surface modules.
// ---------------------------------------------------------------------------

/// Build an atomic predicate: `name(args...)`.
pub(crate) fn atomic(name: impl Into<String>, args: Vec<IrTerm>) -> IrFormula {
    IrFormula::Atomic {
        name: name.into(),
        args,
    }
}

/// An integer constant term.
pub(crate) fn int_const(value: i64) -> IrTerm {
    IrTerm::Const {
        value: serde_json::Value::Number(value.into()),
        sort: Sort::Primitive {
            name: "Int".to_string(),
        },
    }
}

/// A string constant term.
pub(crate) fn str_const(value: impl Into<String>) -> IrTerm {
    IrTerm::Const {
        value: serde_json::Value::String(value.into()),
        sort: Sort::Primitive {
            name: "String".to_string(),
        },
    }
}

/// A variable term.
pub(crate) fn var(name: impl Into<String>) -> IrTerm {
    IrTerm::Var { name: name.into() }
}

// ---------------------------------------------------------------------------
// CID helpers (re-implemented locally — not pub in lift-rust-tests).
// ---------------------------------------------------------------------------

pub(crate) fn serde_json_to_cvalue(v: &serde_json::Value) -> Arc<CValue> {
    match v {
        serde_json::Value::Null => CValue::null(),
        serde_json::Value::Bool(b) => CValue::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else {
                CValue::string(n.to_string())
            }
        }
        serde_json::Value::String(s) => CValue::string(s.clone()),
        serde_json::Value::Array(items) => {
            let cv: Vec<Arc<CValue>> = items.iter().map(serde_json_to_cvalue).collect();
            CValue::array(cv)
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<(String, Arc<CValue>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), serde_json_to_cvalue(v)))
                .collect();
            Arc::new(CValue::Object(entries))
        }
    }
}

/// Compute the JCS-canonical CID for an `EvidenceMemento`.
///
/// Locked JCS key order (alphabetical, `cid` elided):
///   confidence_basis_points, extension_fields, kind, lifter_cid,
///   predicate, schemaVersion, source_kind, source_locator
pub(crate) fn evidence_memento_cid(
    confidence_basis_points: u16,
    extension_fields: &BTreeMap<String, serde_json::Value>,
    lifter_cid: &str,
    predicate: &IrFormula,
    source_kind: &SourceKind,
    source_locator: &SourceLocator,
) -> String {
    let pred_json = serde_json::to_value(predicate).expect("IrFormula must be serializable");
    let pred_cv = serde_json_to_cvalue(&pred_json);

    let ext_entries: Vec<(String, Arc<CValue>)> = extension_fields
        .iter()
        .map(|(k, v)| (k.clone(), serde_json_to_cvalue(v)))
        .collect();
    let ext_cv = Arc::new(CValue::Object(ext_entries));

    let kind_str: String = source_kind.clone().into();

    let make_point = |p: &SourceLocatorPoint| {
        CValue::object([
            ("col", CValue::integer(p.col as i64)),
            ("line", CValue::integer(p.line as i64)),
        ])
    };
    let span_cv = CValue::object([
        ("end", make_point(&source_locator.span.end)),
        ("start", make_point(&source_locator.span.start)),
    ]);
    let locator_cv = CValue::object([
        (
            "source_cid",
            CValue::string(source_locator.source_cid.clone()),
        ),
        ("span", span_cv),
    ]);

    let header = CValue::object([
        (
            "confidence_basis_points",
            CValue::integer(confidence_basis_points as i64),
        ),
        ("extension_fields", ext_cv),
        ("kind", CValue::string("evidence")),
        ("lifter_cid", CValue::string(lifter_cid.to_string())),
        ("predicate", pred_cv),
        ("schemaVersion", CValue::string("1")),
        ("source_kind", CValue::string(kind_str)),
        ("source_locator", locator_cv),
    ]);

    let canonical_bytes = encode_jcs(&header);
    blake3_512_of(canonical_bytes.as_bytes())
}

/// Build an `EvidenceMemento` given all fields.
pub(crate) fn build_memento(
    confidence_basis_points: u16,
    extension_fields: BTreeMap<String, serde_json::Value>,
    predicate: IrFormula,
    source_kind: SourceKind,
    source_locator: SourceLocator,
) -> EvidenceMemento {
    let cid = evidence_memento_cid(
        confidence_basis_points,
        &extension_fields,
        AUTO_PROMOTE_LIFTER_CID,
        &predicate,
        &source_kind,
        &source_locator,
    );
    EvidenceMemento {
        cid,
        confidence_basis_points,
        extension_fields,
        kind: "evidence".to_string(),
        lifter_cid: AUTO_PROMOTE_LIFTER_CID.to_string(),
        predicate,
        schema_version: "1".to_string(),
        source_kind,
        source_locator,
    }
}

/// Build a `SourceLocator` from a source CID and a (1-based) line range.
pub(crate) fn make_locator(source_cid: &str, start_line: u32, end_line: u32) -> SourceLocator {
    SourceLocator {
        source_cid: source_cid.to_string(),
        span: SourceLocatorSpan {
            start: SourceLocatorPoint {
                line: start_line,
                col: 0,
            },
            end: SourceLocatorPoint {
                line: end_line,
                col: 0,
            },
        },
    }
}

/// Build the mandatory extension fields for a native-surface memento.
pub(crate) fn native_ext(
    surface_kind: &str,
    target_function_or_field: &str,
    original_text: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut m = BTreeMap::new();
    m.insert(
        "surface_kind".to_string(),
        serde_json::Value::String(surface_kind.to_string()),
    );
    m.insert(
        "target_function_or_field".to_string(),
        serde_json::Value::String(target_function_or_field.to_string()),
    );
    m.insert(
        "original_text".to_string(),
        serde_json::Value::String(original_text.to_string()),
    );
    m
}
