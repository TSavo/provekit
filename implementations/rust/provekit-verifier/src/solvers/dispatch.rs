// SPDX-License-Identifier: Apache-2.0
//
// Per-fragment dispatch: walk the IR-formula JSON to detect which
// theory dominates, return the matching solver name from the dispatch
// config.
//
// Heuristics (first match wins):
//
//   strings           - any atomic predicate or term whose name is
//                       a known string predicate (length, matches,
//                       contains, prefix-of, suffix-of, str.++).
//   bitvectors        - any sort whose name starts with `BitVec`,
//                       `bv`, `BV`, or any atomic over bitvector
//                       operators (bvadd, bvand, bvshl, ...).
//   linear-arithmetic - default for anything else with arithmetic
//                       atoms over Int/Real (`>`, `<`, `=`, `+`,
//                       `-`, `*` with constant factor).
//   default           - everything else.
//
// The walker is deliberately conservative: if we can't classify, we
// return "default". The dispatch config maps these tags to solver
// names; if the matching tag is missing we fall back to "default";
// if "default" is missing we return None (caller treats as a config
// error and reports Undecidable).

use serde_json::Value as Json;

use crate::solvers::DispatchConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaTheory {
    Strings,
    Bitvectors,
    LinearArithmetic,
    Default,
}

impl FormulaTheory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strings => "strings",
            Self::Bitvectors => "bitvectors",
            Self::LinearArithmetic => "linear-arithmetic",
            Self::Default => "default",
        }
    }
}

const STRING_OPS: &[&str] = &[
    "length",
    "matches",
    "contains",
    "prefix-of",
    "suffix-of",
    "str.++",
    "str.len",
    "str.indexof",
];

const BV_OPS: &[&str] = &[
    "bvadd", "bvsub", "bvmul", "bvand", "bvor", "bvxor", "bvnot", "bvshl", "bvlshr", "bvashr",
    "bvult", "bvule", "bvugt", "bvuge", "bvslt", "bvsle", "bvsgt", "bvsge",
];

pub fn classify(formula: &Json) -> FormulaTheory {
    let mut theory = FormulaTheory::Default;
    walk(formula, &mut theory);
    theory
}

fn walk(v: &Json, theory: &mut FormulaTheory) {
    // Strings dominates over BV, BV dominates over LIA, LIA dominates
    // over Default. Once we set a higher-precedence theory we keep it.
    match v {
        Json::Object(map) => {
            if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                if STRING_OPS.contains(&name) {
                    *theory = FormulaTheory::Strings;
                    return;
                }
                if BV_OPS.contains(&name) {
                    if *theory != FormulaTheory::Strings {
                        *theory = FormulaTheory::Bitvectors;
                    }
                }
                if matches!(name, ">" | "<" | ">=" | "<=" | "=" | "+" | "-" | "*") {
                    if *theory == FormulaTheory::Default {
                        *theory = FormulaTheory::LinearArithmetic;
                    }
                }
            }
            // Detect bitvector sorts: {"kind":"primitive","name":"BitVec"} or BV<n>.
            if let Some(sort) = map.get("sort") {
                if let Some(srt_obj) = sort.as_object() {
                    if let Some(srt_name) = srt_obj.get("name").and_then(|v| v.as_str()) {
                        if srt_name == "String" {
                            *theory = FormulaTheory::Strings;
                            return;
                        }
                        if srt_name.starts_with("BitVec")
                            || srt_name.starts_with("bv")
                            || srt_name.starts_with("BV")
                        {
                            if *theory != FormulaTheory::Strings {
                                *theory = FormulaTheory::Bitvectors;
                            }
                        }
                    }
                }
            }
            for (_, child) in map {
                if *theory == FormulaTheory::Strings {
                    return;
                }
                walk(child, theory);
            }
        }
        Json::Array(arr) => {
            for child in arr {
                if *theory == FormulaTheory::Strings {
                    return;
                }
                walk(child, theory);
            }
        }
        _ => {}
    }
}

/// Apply the dispatch config: classify the formula, look up the named
/// solver. Returns `None` if neither the matching tag nor `default` is
/// configured.
pub fn dispatch_for_formula<'a>(
    formula: &Json,
    dispatch: &'a DispatchConfig,
) -> Option<&'a str> {
    let t = classify(formula);
    let by_theory = match t {
        FormulaTheory::Strings => dispatch.strings.as_deref(),
        FormulaTheory::Bitvectors => dispatch.bitvectors.as_deref(),
        FormulaTheory::LinearArithmetic => dispatch.linear_arithmetic.as_deref(),
        FormulaTheory::Default => None,
    };
    by_theory.or(dispatch.default.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classify_lia() {
        let f = json!({
            "kind": "atomic",
            "name": ">",
            "args": [{"kind":"var","name":"n"}, {"kind":"const","value":0}]
        });
        assert_eq!(classify(&f), FormulaTheory::LinearArithmetic);
    }

    #[test]
    fn classify_strings_by_op() {
        let f = json!({
            "kind": "atomic",
            "name": "length",
            "args": [{"kind":"var","name":"s"}]
        });
        assert_eq!(classify(&f), FormulaTheory::Strings);
    }

    #[test]
    fn classify_strings_by_sort() {
        let f = json!({
            "kind": "forall",
            "name": "s",
            "sort": {"kind":"primitive","name":"String"},
            "body": {"kind":"atomic","name":"=","args":[]}
        });
        assert_eq!(classify(&f), FormulaTheory::Strings);
    }

    #[test]
    fn classify_bitvectors() {
        let f = json!({
            "kind": "atomic",
            "name": "bvadd",
            "args": [{"kind":"var","name":"x"}, {"kind":"var","name":"y"}]
        });
        assert_eq!(classify(&f), FormulaTheory::Bitvectors);
    }

    #[test]
    fn dispatch_picks_solver() {
        let d = DispatchConfig {
            strings: Some("cvc5".into()),
            bitvectors: Some("bitwuzla".into()),
            linear_arithmetic: Some("z3".into()),
            default: Some("z3".into()),
        };
        let f = json!({"kind":"atomic","name":"length","args":[]});
        assert_eq!(dispatch_for_formula(&f, &d), Some("cvc5"));
        let f = json!({"kind":"atomic","name":"bvadd","args":[]});
        assert_eq!(dispatch_for_formula(&f, &d), Some("bitwuzla"));
        let f = json!({"kind":"atomic","name":">","args":[]});
        assert_eq!(dispatch_for_formula(&f, &d), Some("z3"));
        let f = json!({"kind":"atomic","name":"unknown","args":[]});
        assert_eq!(dispatch_for_formula(&f, &d), Some("z3")); // via default
    }
}
