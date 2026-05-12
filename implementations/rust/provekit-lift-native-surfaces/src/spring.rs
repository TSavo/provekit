// SPDX-License-Identifier: Apache-2.0
//
// Spring (Java) native contract surface lifter — v0
//
// Recognised annotation idioms:
//
//   @PreCondition("expr")  — pre-condition string; opaque atomic, conf ~5000
//   @PostCondition("expr") — post-condition string; opaque atomic, conf ~5000
//   @NotNull               — null-exclusion, conf 10000; emits `≠(self, null)`
//   @Min(value)            — numeric lower bound, conf 10000; emits `≥(self, N)`
//   @Max(value)            — numeric upper bound, conf 10000; emits `≤(self, N)`
//   @Size(min=N)           — collection/string lower-bound, conf 10000
//   @Size(max=N)           — collection/string upper-bound, conf 10000
//   @Size(min=N, max=M)    — both bounds, emits two mementos
//   @Positive              — emits `>(self, 0)`, conf 10000
//   @PositiveOrZero        — emits `≥(self, 0)`, conf 10000
//   @Negative              — emits `<(self, 0)`, conf 10000
//   @NegativeOrZero        — emits `≤(self, 0)`, conf 10000
//
// Target heuristic: the annotated field/parameter name is extracted from the
// next `\w+(\s+\w+)*\s+(\w+)` shape on the same or following line.  If we
// cannot find one we use the placeholder `"_unknown"`.

use regex::Regex;
use std::sync::OnceLock;

use provekit_ir_types::{EvidenceMemento, SourceKind};

use crate::{atomic, build_memento, int_const, make_locator, native_ext, str_const, var};

// ---------------------------------------------------------------------------
// Compiled regexes (thread-safe via OnceLock)
// ---------------------------------------------------------------------------

fn re_precondition() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"@PreCondition\s*\(\s*"([^"]*)"\s*\)"#).unwrap())
}

fn re_postcondition() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"@PostCondition\s*\(\s*"([^"]*)"\s*\)"#).unwrap())
}

fn re_notnull() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@NotNull\b").unwrap())
}

fn re_min() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@Min\s*\(\s*(?:value\s*=\s*)?(-?\d+)\s*\)").unwrap())
}

fn re_max() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@Max\s*\(\s*(?:value\s*=\s*)?(-?\d+)\s*\)").unwrap())
}

fn re_size() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // matches @Size(min=N), @Size(max=N), @Size(min=N, max=M) in any attribute order
    R.get_or_init(|| Regex::new(r"@Size\s*\(([^)]*)\)").unwrap())
}

fn re_positive() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@Positive\b").unwrap())
}

fn re_positive_or_zero() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@PositiveOrZero\b").unwrap())
}

fn re_negative() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@Negative\b").unwrap())
}

fn re_negative_or_zero() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"@NegativeOrZero\b").unwrap())
}

/// Extract `min=N` from a @Size attribute body.
fn parse_size_attr(body: &str) -> (Option<i64>, Option<i64>) {
    let re_min_attr =
        Regex::new(r"min\s*=\s*(-?\d+)").unwrap();
    let re_max_attr =
        Regex::new(r"max\s*=\s*(-?\d+)").unwrap();
    let min = re_min_attr
        .captures(body)
        .and_then(|c| c[1].parse::<i64>().ok());
    let max = re_max_attr
        .captures(body)
        .and_then(|c| c[1].parse::<i64>().ok());
    (min, max)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Walk `source` (Java file text) and emit `EvidenceMemento` records for
/// every recognised Spring annotation idiom.  `source_cid` is the
/// content-address of the source file (caller-supplied, e.g. a fake CID for
/// tests).
pub fn lift_spring_file(source: &str, source_cid: &str) -> Vec<EvidenceMemento> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_no = (line_idx + 1) as u32; // 1-based
        let next_line = lines.get(line_idx + 1).copied().unwrap_or("");
        let target = extract_java_target(line, next_line);
        let locator = make_locator(source_cid, line_no, line_no);

        // @PreCondition("expr")
        if let Some(cap) = re_precondition().captures(line) {
            let expr = &cap[1];
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(expr)]);
            out.push(build_memento(
                5000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @PostCondition("expr")
        if let Some(cap) = re_postcondition().captures(line) {
            let expr = &cap[1];
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic("opaque", vec![str_const(expr)]);
            out.push(build_memento(
                5000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @NotNull
        if re_notnull().is_match(line) {
            let ext = native_ext("spring", &target, line.trim());
            // ≠(self, null)
            let predicate = atomic(
                "≠",
                vec![var("self"), str_const("null")],
            );
            out.push(build_memento(
                10000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @Min(N)
        if let Some(cap) = re_min().captures(line) {
            if let Ok(n) = cap[1].parse::<i64>() {
                let ext = native_ext("spring", &target, line.trim());
                let predicate = atomic("≥", vec![var("self"), int_const(n)]);
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // @Max(N)
        if let Some(cap) = re_max().captures(line) {
            if let Ok(n) = cap[1].parse::<i64>() {
                let ext = native_ext("spring", &target, line.trim());
                let predicate = atomic("≤", vec![var("self"), int_const(n)]);
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // @Size(min=N, max=M)
        if let Some(cap) = re_size().captures(line) {
            let body = &cap[1];
            let (min_val, max_val) = parse_size_attr(body);
            if let Some(n) = min_val {
                let ext = native_ext("spring", &target, line.trim());
                let predicate = atomic("≥", vec![var("self"), int_const(n)]);
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
            if let Some(m) = max_val {
                let ext = native_ext("spring", &target, line.trim());
                let predicate = atomic("≤", vec![var("self"), int_const(m)]);
                out.push(build_memento(
                    10000,
                    ext,
                    predicate,
                    SourceKind::NativeSurface,
                    locator.clone(),
                ));
            }
        }

        // @Positive
        if re_positive_or_zero().is_match(line) {
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic("≥", vec![var("self"), int_const(0)]);
            out.push(build_memento(
                10000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        } else if re_positive().is_match(line) {
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic(">", vec![var("self"), int_const(0)]);
            out.push(build_memento(
                10000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        }

        // @Negative
        if re_negative_or_zero().is_match(line) {
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic("≤", vec![var("self"), int_const(0)]);
            out.push(build_memento(
                10000,
                ext,
                predicate,
                SourceKind::NativeSurface,
                locator.clone(),
            ));
        } else if re_negative().is_match(line) {
            let ext = native_ext("spring", &target, line.trim());
            let predicate = atomic("<", vec![var("self"), int_const(0)]);
            out.push(build_memento(
                10000,
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
// Target name heuristic
// ---------------------------------------------------------------------------

/// Try to extract the field/parameter name from `annotation_line` and the
/// following `next_line`.  Returns `"_unknown"` if nothing obvious is found.
fn extract_java_target(annotation_line: &str, next_line: &str) -> String {
    // Look in the annotation line itself (inline annotation on a field decl).
    // Also look at next line (annotation on its own line above the declaration).
    for candidate in &[annotation_line, next_line] {
        if let Some(name) = java_decl_last_word(candidate) {
            return name;
        }
    }
    "_unknown".to_string()
}

/// Extract the last identifier from a Java-style field or parameter declaration.
/// Patterns like `private int age` → "age", `String name,` → "name".
fn java_decl_last_word(line: &str) -> Option<String> {
    let re = Regex::new(r"\b([A-Za-z_]\w*)\s*[;,=)\{]?\s*$").unwrap();
    // Strip annotations first so "@NotNull int age" → "int age" → "age"
    let stripped = Regex::new(r"@\w+(\([^)]*\))?")
        .unwrap()
        .replace_all(line, "");
    re.captures(stripped.trim())
        .map(|c| c[1].to_string())
        .filter(|s| !is_java_keyword(s))
}

fn is_java_keyword(s: &str) -> bool {
    matches!(
        s,
        "public"
            | "private"
            | "protected"
            | "static"
            | "final"
            | "int"
            | "long"
            | "double"
            | "float"
            | "boolean"
            | "String"
            | "void"
            | "class"
            | "interface"
            | "enum"
            | "return"
            | "null"
            | "new"
    )
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
    fn test_spring_notnull_recognition() {
        let src = "    @NotNull\n    private String email;\n";
        let mementos = lift_spring_file(src, FAKE_CID);
        assert!(
            !mementos.is_empty(),
            "expected at least one memento for @NotNull"
        );
        let m = &mementos[0];
        assert_eq!(m.source_kind, SourceKind::NativeSurface);
        assert_eq!(m.kind, "evidence");
        assert_eq!(m.schema_version, "1");
        assert_eq!(m.confidence_basis_points, 10000);
    }

    #[test]
    fn test_spring_notnull_predicate_shape() {
        let src = "    @NotNull\n    private String email;\n";
        let mementos = lift_spring_file(src, FAKE_CID);
        let m = &mementos[0];
        match &m.predicate {
            IrFormula::Atomic { name, args } => {
                assert_eq!(name, "≠");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Atomic, got {:?}", other),
        }
    }

    #[test]
    fn test_spring_notnull_json_round_trip() {
        let src = "    @NotNull\n    private String email;\n";
        let mementos = lift_spring_file(src, FAKE_CID);
        let m = &mementos[0];
        let json = serde_json::to_string(m).expect("must serialize");
        let back: EvidenceMemento = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(back.cid, m.cid);
        assert_eq!(back.confidence_basis_points, 10000);
        // extension_fields must contain the three mandatory keys
        assert!(back.extension_fields.contains_key("surface_kind"));
        assert!(back.extension_fields.contains_key("target_function_or_field"));
        assert!(back.extension_fields.contains_key("original_text"));
    }

    #[test]
    fn test_spring_min_max() {
        let src = "    @Min(0)\n    @Max(120)\n    private int age;\n";
        let mementos = lift_spring_file(src, FAKE_CID);
        // Should get ≥ for @Min and ≤ for @Max
        let names: Vec<String> = mementos
            .iter()
            .filter_map(|m| match &m.predicate {
                IrFormula::Atomic { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"≥".to_string()), "expected ≥ from @Min");
        assert!(names.contains(&"≤".to_string()), "expected ≤ from @Max");
    }

    #[test]
    fn test_spring_size_both_bounds() {
        let src = "    @Size(min = 1, max = 255)\n    private String username;\n";
        let mementos = lift_spring_file(src, FAKE_CID);
        // Two mementos: ≥ for min, ≤ for max
        assert_eq!(mementos.len(), 2);
    }

    #[test]
    fn test_spring_precondition_opaque() {
        let src = r#"    @PreCondition("x > 0 && y < 100")
    public int compute(int x, int y) { return x + y; }
"#;
        let mementos = lift_spring_file(src, FAKE_CID);
        assert!(!mementos.is_empty());
        let m = &mementos[0];
        assert_eq!(m.confidence_basis_points, 5000);
        match &m.predicate {
            IrFormula::Atomic { name, .. } => assert_eq!(name, "opaque"),
            other => panic!("expected opaque atomic, got {:?}", other),
        }
    }
}
