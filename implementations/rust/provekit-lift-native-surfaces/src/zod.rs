// SPDX-License-Identifier: Apache-2.0
//
// Zod (TypeScript) native contract surface lifter — v0
//
// Recognised Zod chained-method idioms:
//
//   z.number().min(N)       — emits `≥(self, N)`,  conf 10000
//   z.number().max(N)       — emits `≤(self, N)`,  conf 10000
//   z.number().int()        — emits `is_integer(self)`, conf 10000
//   z.number().positive()   — emits `>(self, 0)`,  conf 10000
//   z.number().nonnegative()— emits `≥(self, 0)`,  conf 10000
//   z.number().negative()   — emits `<(self, 0)`,  conf 10000
//   z.number().nonpositive()— emits `≤(self, 0)`,  conf 10000
//   z.string().min(N)       — emits `≥(len(self), N)`, conf 10000
//   z.string().max(N)       — emits `≤(len(self), N)`, conf 10000
//   z.string().length(N)    — emits `=(len(self), N)`, conf 10000
//   z.string().regex(...)   — opaque regex, conf 5000
//   z.string().email()      — emits `is_email(self)`, conf 10000
//   z.string().url()        — emits `is_url(self)`, conf 10000
//   z.array(...).min(N)     — emits `≥(len(self), N)`, conf 10000
//   z.array(...).max(N)     — emits `≤(len(self), N)`, conf 10000
//   z.array(...).length(N)  — emits `=(len(self), N)`, conf 10000
//
// Binding heuristic: look for `const name = ...` or `name:` before the z.xxx chain
// on the same line.
//
// Each recognised idiom on a line produces one (or more) mementos for that line.
// A single chain like `z.number().min(0).max(100)` on one line produces two mementos.

use regex::Regex;
use std::sync::OnceLock;

use provekit_ir_types::{EvidenceMemento, IrTerm, SourceKind};

use crate::{atomic, build_memento, int_const, make_locator, native_ext, str_const, var};

// ---------------------------------------------------------------------------
// Regexes
// ---------------------------------------------------------------------------

fn re_number_min() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\.min\s*\(\s*(-?\d+)\s*\)").unwrap())
}

fn re_number_max() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\.max\s*\(\s*(-?\d+)\s*\)").unwrap())
}

fn re_length() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\.length\s*\(\s*(-?\d+)\s*\)").unwrap())
}

fn re_zod_chain_type() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // Captures the Zod primitive type: number, string, array, bigint, etc.
    R.get_or_init(|| Regex::new(r"\bz\.(number|string|array|bigint|boolean)\b").unwrap())
}

fn re_zod_unary_method() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // zero-arg Zod refinement methods
    R.get_or_init(|| {
        Regex::new(r"\.(positive|nonnegative|negative|nonpositive|int|email|url)\(\s*\)").unwrap()
    })
}

fn re_regex_method() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // .regex(/.../) or .regex(new RegExp("..."))
    R.get_or_init(|| Regex::new(r"\.regex\s*\((.+?)\)").unwrap())
}

/// Detect if the line contains a Zod chain and return the primitive kind.
fn zod_chain_kind(line: &str) -> Option<&'static str> {
    let cap = re_zod_chain_type().captures(line)?;
    match &cap[1] {
        "number" | "bigint" => Some("number"),
        "string" => Some("string"),
        "array" => Some("array"),
        _ => None,
    }
}

/// Extract the binding/schema name from the line.
/// Matches `const name = ...` or `name:` patterns.
fn extract_zod_target(line: &str) -> String {
    // `const name = z.xxx` or `let name = z.xxx` or `var name = z.xxx`
    let re_const = Regex::new(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=").unwrap();
    if let Some(cap) = re_const.captures(line) {
        return cap[1].to_string();
    }
    // `name: z.xxx` (object property)
    let re_prop = Regex::new(r"^\s*([A-Za-z_$][\w$]*)\s*:").unwrap();
    if let Some(cap) = re_prop.captures(line) {
        return cap[1].to_string();
    }
    "_unknown".to_string()
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Walk `source` (TypeScript file text) and emit `EvidenceMemento` records
/// for every recognised Zod schema constraint.
pub fn lift_zod_file(source: &str, source_cid: &str) -> Vec<EvidenceMemento> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let locator = make_locator(source_cid, line_no, line_no);
        let target = extract_zod_target(line);

        let Some(kind) = zod_chain_kind(line) else {
            continue;
        };

        let len_subject = || IrTerm::Ctor {
            name: "len".to_string(),
            args: vec![var("self")],
        };

        // .min(N) — lower bound
        for cap in re_number_min().captures_iter(line) {
            if let Ok(n) = cap[1].parse::<i64>() {
                let subject = if kind == "number" {
                    vec![var("self")]
                } else {
                    vec![len_subject()]
                };
                let ext = native_ext("zod", &target, line.trim());
                let predicate = atomic("≥", {
                    let mut v = subject;
                    v.push(int_const(n));
                    v
                });
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // .max(N) — upper bound
        for cap in re_number_max().captures_iter(line) {
            if let Ok(n) = cap[1].parse::<i64>() {
                let subject = if kind == "number" {
                    vec![var("self")]
                } else {
                    vec![len_subject()]
                };
                let ext = native_ext("zod", &target, line.trim());
                let predicate = atomic("≤", {
                    let mut v = subject;
                    v.push(int_const(n));
                    v
                });
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // .length(N) — exact length (string/array)
        for cap in re_length().captures_iter(line) {
            if let Ok(n) = cap[1].parse::<i64>() {
                let ext = native_ext("zod", &target, line.trim());
                let predicate = atomic("=", vec![len_subject(), int_const(n)]);
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // zero-arg methods: .positive(), .nonnegative(), etc.
        for cap in re_zod_unary_method().captures_iter(line) {
            let method = &cap[1];
            let (pred_name, args): (&str, Vec<IrTerm>) = match method {
                "positive" => (">", vec![var("self"), int_const(0)]),
                "nonnegative" => ("≥", vec![var("self"), int_const(0)]),
                "negative" => ("<", vec![var("self"), int_const(0)]),
                "nonpositive" => ("≤", vec![var("self"), int_const(0)]),
                "int" => ("is_integer", vec![var("self")]),
                "email" => ("is_email", vec![var("self")]),
                "url" => ("is_url", vec![var("self")]),
                _ => continue,
            };
            let ext = native_ext("zod", &target, line.trim());
            let predicate = atomic(pred_name, args);
            out.push(build_memento(
                10000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // .regex(...)
        if let Some(cap) = re_regex_method().captures(line) {
            let regex_text = cap[1].trim().to_string();
            let ext = native_ext("zod", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(&regex_text)]);
            out.push(build_memento(
                5000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use provekit_ir_types::IrFormula;

    const FAKE_CID: &str = "blake3-512:aabbcc";

    #[test]
    fn test_zod_number_min_max_recognition() {
        let src = "const score = z.number().min(0).max(100);\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        assert_eq!(mementos.len(), 2, "expected 2 mementos: ≥ and ≤");
        for m in &mementos {
            assert_eq!(m.source_kind, SourceKind::NativeSurface);
            assert_eq!(m.kind, "evidence");
            assert_eq!(m.confidence_basis_points, 10000);
        }
    }

    #[test]
    fn test_zod_number_predicate_shape() {
        let src = "const age = z.number().min(0).max(120);\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        let names: Vec<String> = mementos
            .iter()
            .filter_map(|m| match &m.predicate {
                IrFormula::Atomic { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"≥".to_string()), "expected ≥ from .min()");
        assert!(names.contains(&"≤".to_string()), "expected ≤ from .max()");
        // target should be "age"
        for m in &mementos {
            assert_eq!(
                m.extension_fields["target_function_or_field"]
                    .as_str()
                    .unwrap(),
                "age"
            );
        }
    }

    #[test]
    fn test_zod_number_json_round_trip() {
        let src = "const score = z.number().min(0).max(100);\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        for m in &mementos {
            let json = serde_json::to_string(m).expect("must serialize");
            let back: EvidenceMemento = serde_json::from_str(&json).expect("must deserialize");
            assert_eq!(back.cid, m.cid);
            assert!(back.extension_fields.contains_key("surface_name"));
            assert!(back.extension_fields.contains_key("target_function_or_field"));
            assert!(back.extension_fields.contains_key("original_text"));
        }
    }

    #[test]
    fn test_zod_string_length_constraints() {
        let src = "  username: z.string().min(3).max(20),\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        assert_eq!(mementos.len(), 2);
        // Both should use len(self) as subject
        for m in &mementos {
            match &m.predicate {
                IrFormula::Atomic { args, .. } => match &args[0] {
                    provekit_ir_types::IrTerm::Ctor { name, .. } => {
                        assert_eq!(name, "len");
                    }
                    other => panic!("expected Ctor(len, ...), got {:?}", other),
                },
                other => panic!("expected Atomic, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_zod_string_exact_length() {
        let src = "const zip = z.string().length(5);\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        assert_eq!(mementos.len(), 1);
        match &mementos[0].predicate {
            IrFormula::Atomic { name, .. } => assert_eq!(name, "="),
            other => panic!("expected Atomic(=), got {:?}", other),
        }
    }

    #[test]
    fn test_zod_unary_positive() {
        let src = "const qty = z.number().positive();\n";
        let mementos = lift_zod_file(src, FAKE_CID);
        assert_eq!(mementos.len(), 1);
        match &mementos[0].predicate {
            IrFormula::Atomic { name, args } => {
                assert_eq!(name, ">");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Atomic(>), got {:?}", other),
        }
    }

    #[test]
    fn test_zod_regex_opaque() {
        let src = r#"const phone = z.string().regex(/^\d{10}$/);"#;
        let mementos = lift_zod_file(src, FAKE_CID);
        assert!(!mementos.is_empty());
        let m = &mementos[0];
        assert_eq!(m.confidence_basis_points, 5000);
        match &m.predicate {
            IrFormula::Atomic { name, .. } => assert_eq!(name, "opaque"),
            other => panic!("expected opaque, got {:?}", other),
        }
    }
}
