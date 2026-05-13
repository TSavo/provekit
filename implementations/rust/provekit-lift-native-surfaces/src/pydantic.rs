// SPDX-License-Identifier: Apache-2.0
//
// pydantic (Python) native contract surface lifter — v0
//
// Recognised annotation idioms:
//
//   pydantic Field keyword args:
//     Field(ge=N)     — emits `≥(self, N)`,  conf 10000
//     Field(gt=N)     — emits `>(self, N)`,   conf 10000
//     Field(le=N)     — emits `≤(self, N)`,   conf 10000
//     Field(lt=N)     — emits `<(self, N)`,   conf 10000
//     Field(min_length=N) — emits `≥(len(self), N)`, conf 10000
//     Field(max_length=N) — emits `≤(len(self), N)`, conf 10000
//     Field(pattern="...")— opaque regex constraint, conf 5000
//
//   deal library decorators:
//     @deal.pre(lambda ...: expr)  — opaque pre, conf 5000
//     @deal.post(lambda ...: expr) — opaque post, conf 5000
//     @deal.ensure(lambda ...: expr) — opaque ensure, conf 5000
//
// Target name heuristic: for Field(...) we look for `name: Type = Field(...)` or
// `name = Field(...)` on the same line; for @deal we look at the def line below.

use regex::Regex;
use std::sync::OnceLock;

use provekit_ir_types::{EvidenceMemento, IrTerm, SourceKind};

use crate::{atomic, build_memento, int_const, make_locator, native_ext, str_const, var};

// ---------------------------------------------------------------------------
// Compiled regexes
// ---------------------------------------------------------------------------

fn re_field() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // Matches `Field(...)` call anywhere on the line; captures the arg list.
    R.get_or_init(|| Regex::new(r"Field\s*\(([^)]*)\)").unwrap())
}

fn re_deal_pre() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@deal\.pre\s*\((.+)\)").unwrap())
}

fn re_deal_post() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@deal\.post\s*\((.+)\)").unwrap())
}

fn re_deal_ensure() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@deal\.ensure\s*\((.+)\)").unwrap())
}

/// Extract the field name from a pydantic Field assignment line.
/// Matches patterns like `name: Type = Field(...)` and `name = Field(...)`.
fn extract_py_field_name(line: &str) -> String {
    let re = Regex::new(r"^\s*([A-Za-z_]\w*)\s*(?::\s*[^=]+)?\s*=\s*Field").unwrap();
    re.captures(line)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "_unknown".to_string())
}

/// Extract a `def` name from a Python function definition line.
fn extract_py_def_name(line: &str) -> String {
    let re = Regex::new(r"def\s+([A-Za-z_]\w*)").unwrap();
    re.captures(line)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "_unknown".to_string())
}

/// Parse Field(...) keyword args: `ge`, `gt`, `le`, `lt`, `min_length`,
/// `max_length`, `pattern`.
struct FieldArgs {
    ge: Option<i64>,
    gt: Option<i64>,
    le: Option<i64>,
    lt: Option<i64>,
    min_length: Option<i64>,
    max_length: Option<i64>,
    pattern: Option<String>,
}

fn parse_field_args(body: &str) -> FieldArgs {
    let int_arg = |name: &str| -> Option<i64> {
        let pattern = format!(r"(?:^|,)\s*{}\s*=\s*(-?\d+)", name);
        Regex::new(&pattern)
            .unwrap()
            .captures(body)
            .and_then(|c| c[1].parse().ok())
    };
    let str_arg = |name: &str| -> Option<String> {
        let pattern = format!(r#"(?:^|,)\s*{}\s*=\s*"([^"]*)""#, name);
        Regex::new(&pattern)
            .unwrap()
            .captures(body)
            .map(|c| c[1].to_string())
    };
    FieldArgs {
        ge: int_arg("ge"),
        gt: int_arg("gt"),
        le: int_arg("le"),
        lt: int_arg("lt"),
        min_length: int_arg("min_length"),
        max_length: int_arg("max_length"),
        pattern: str_arg("pattern"),
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Walk `source` (Python file text) and emit `EvidenceMemento` records for
/// every recognised pydantic/deal annotation idiom.
pub fn lift_pydantic_file(source: &str, source_cid: &str) -> Vec<EvidenceMemento> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let next_line = lines.get(line_idx + 1).copied().unwrap_or("");
        let locator = make_locator(source_cid, line_no, line_no);

        // pydantic Field(...)
        if let Some(cap) = re_field().captures(line) {
            let body = &cap[1];
            let field_name = extract_py_field_name(line);
            let args = parse_field_args(body);

            let emit_int = |pred_name: &str, n: i64, subject: Vec<IrTerm>| {
                let ext = native_ext("pydantic", &field_name, line.trim());
                let predicate = atomic(pred_name, {
                    let mut v = subject;
                    v.push(int_const(n));
                    v
                });
                build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                )
            };

            if let Some(n) = args.ge {
                out.push(emit_int("≥", n, vec![var("self")]));
            }
            if let Some(n) = args.gt {
                out.push(emit_int(">", n, vec![var("self")]));
            }
            if let Some(n) = args.le {
                out.push(emit_int("≤", n, vec![var("self")]));
            }
            if let Some(n) = args.lt {
                out.push(emit_int("<", n, vec![var("self")]));
            }
            if let Some(n) = args.min_length {
                out.push(emit_int(
                    "≥",
                    n,
                    vec![IrTerm::Ctor {
                        name: "len".to_string(),
                        args: vec![var("self")],
                    }],
                ));
            }
            if let Some(n) = args.max_length {
                out.push(emit_int(
                    "≤",
                    n,
                    vec![IrTerm::Ctor {
                        name: "len".to_string(),
                        args: vec![var("self")],
                    }],
                ));
            }
            if let Some(pat) = args.pattern {
                let ext = native_ext("pydantic", &field_name, line.trim());
                let predicate = atomic("opaque", vec![str_const(pat)]);
                out.push(build_memento(
                    5000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // @deal.pre(...)
        if let Some(cap) = re_deal_pre().captures(line) {
            let lambda_text = cap[1].trim().to_string();
            let target = extract_py_def_name(next_line);
            let ext = native_ext("pydantic-deal", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(&lambda_text)]);
            out.push(build_memento(
                5000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @deal.post(...)
        if let Some(cap) = re_deal_post().captures(line) {
            let lambda_text = cap[1].trim().to_string();
            let target = extract_py_def_name(next_line);
            let ext = native_ext("pydantic-deal", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(&lambda_text)]);
            out.push(build_memento(
                5000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @deal.ensure(...)
        if let Some(cap) = re_deal_ensure().captures(line) {
            let lambda_text = cap[1].trim().to_string();
            let target = extract_py_def_name(next_line);
            let ext = native_ext("pydantic-deal", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(&lambda_text)]);
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
    fn test_pydantic_field_ge_recognition() {
        let src = "    age: int = Field(ge=0, le=120)\n";
        let mementos = lift_pydantic_file(src, FAKE_CID);
        assert!(
            !mementos.is_empty(),
            "expected mementos for Field(ge=, le=)"
        );
        assert_eq!(mementos[0].source_kind, SourceKind::NativeSurface);
        assert_eq!(mementos[0].kind, "evidence");
    }

    #[test]
    fn test_pydantic_field_predicate_shape() {
        let src = "    age: int = Field(ge=0, le=120)\n";
        let mementos = lift_pydantic_file(src, FAKE_CID);
        // Should emit ≥(self, 0) and ≤(self, 120)
        assert_eq!(mementos.len(), 2);
        let names: Vec<String> = mementos
            .iter()
            .filter_map(|m| match &m.predicate {
                IrFormula::Atomic { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"≥".to_string()));
        assert!(names.contains(&"≤".to_string()));
    }

    #[test]
    fn test_pydantic_field_json_round_trip() {
        let src = "    score: float = Field(ge=0.0, le=1.0)\n    age: int = Field(ge=0)\n";
        let mementos = lift_pydantic_file(src, FAKE_CID);
        // age line: ge=0 -> one memento
        let m = mementos.iter().find(|m| {
            m.extension_fields
                .get("target_function_or_field")
                .and_then(|v| v.as_str())
                == Some("age")
        });
        if let Some(m) = m {
            let json = serde_json::to_string(m).expect("must serialize");
            let back: EvidenceMemento = serde_json::from_str(&json).expect("must deserialize");
            assert_eq!(back.cid, m.cid);
            assert!(back.extension_fields.contains_key("surface_name"));
            assert!(back
                .extension_fields
                .contains_key("target_function_or_field"));
            assert!(back.extension_fields.contains_key("original_text"));
        }
    }

    #[test]
    fn test_pydantic_deal_pre_opaque() {
        let src = "@deal.pre(lambda x: x > 0)\ndef square(x):\n    return x * x\n";
        let mementos = lift_pydantic_file(src, FAKE_CID);
        assert!(!mementos.is_empty());
        let m = &mementos[0];
        assert_eq!(m.confidence_basis_points, 5000);
        match &m.predicate {
            IrFormula::Atomic { name, .. } => assert_eq!(name, "opaque"),
            other => panic!("expected opaque, got {:?}", other),
        }
    }

    #[test]
    fn test_pydantic_min_max_length() {
        let src = "    username: str = Field(min_length=3, max_length=50)\n";
        let mementos = lift_pydantic_file(src, FAKE_CID);
        assert_eq!(mementos.len(), 2);
        // Both should wrap in len(self)
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
}
