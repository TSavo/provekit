// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-verus
//
// STRATEGIC POSITIONING
//
// ProvekIt consumes verus's existing annotations; we sit beneath, not
// against. Verus is a Rust-embedded verifier (https://github.com/verus-lang/verus)
// that ships its own syntax-extension layer: `requires`, `ensures`,
// `decreases`, `invariant` clauses appear inside `verus! { ... }` blocks
// and are NOT standard Rust expressions. Parsing them requires verus's
// own pre-processor.
//
// V0 DECISION
//
// Detect every `verus! { ... }` macro invocation in the file and emit a
// structured "skipped" warning per top-level item inside it. Lift count
// is 0. The warning carries enough context (item name + reason code) for
// a future v1.2 lifter to slot in and convert the same items.
//
// Documented gap, by design: an honest "we saw it, we did not lift it"
// log is the right shape for a v0. Pretending coverage we don't have
// pollutes the lattice.

use provekit_ir_symbolic::ContractDecl;

#[derive(Debug, Clone)]
pub struct LiftWarning {
    pub source_path: String,
    pub item_name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct AdapterOutput {
    pub decls: Vec<ContractDecl>,
    pub warnings: Vec<LiftWarning>,
    pub seen: usize,
    pub lifted: usize,
}

pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    walk_items(&file.items, source_path, &mut out);
    out
}

fn walk_items(items: &[syn::Item], source_path: &str, out: &mut AdapterOutput) {
    for item in items {
        match item {
            syn::Item::Macro(m) => visit_macro(&m.mac, source_path, out),
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, out);
                }
            }
            syn::Item::Fn(f) => {
                walk_block(&f.block, source_path, out);
            }
            _ => {}
        }
    }
}

fn walk_block(block: &syn::Block, source_path: &str, out: &mut AdapterOutput) {
    for stmt in &block.stmts {
        if let syn::Stmt::Macro(m) = stmt {
            visit_macro(&m.mac, source_path, out);
        }
        if let syn::Stmt::Item(item) = stmt {
            walk_items(std::slice::from_ref(item), source_path, out);
        }
    }
}

fn visit_macro(mac: &syn::Macro, source_path: &str, out: &mut AdapterOutput) {
    let path = path_to_string(&mac.path);
    if path != "verus" {
        return;
    }
    out.seen += 1;
    // Best-effort: scan tokens for top-level fn/spec/proof declarations
    // so the warning carries a real item name. Verus's syntax inside a
    // `verus! { ... }` block is NOT standard Rust, so syn cannot parse
    // it; we walk the TokenStream looking for `fn <ident>` patterns.
    let names = scan_item_names(mac.tokens.clone());
    if names.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: "<verus block>".into(),
            reason: "verus! { ... } block detected; v0 does not lift verus syntax (gap documented in README; revisit in v1.2)".into(),
        });
    } else {
        for n in names {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: n,
                reason: "verus item skipped; v0 does not lift verus syntax (gap documented in README; revisit in v1.2)".into(),
            });
        }
    }
}

/// Walk the TokenStream and collect identifiers immediately following
/// the keywords `fn`, `spec`, `proof`. Best-effort: this is for
/// diagnostic naming, not for translation.
fn scan_item_names(ts: proc_macro2::TokenStream) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut prev_was_keyword = false;
    let mut last_keyword: Option<String> = None;
    for tt in ts {
        match tt {
            proc_macro2::TokenTree::Ident(id) => {
                let s = id.to_string();
                if prev_was_keyword {
                    let label = match last_keyword.as_deref() {
                        Some("spec") => format!("spec fn {s}"),
                        Some("proof") => format!("proof fn {s}"),
                        _ => format!("fn {s}"),
                    };
                    out.push(label);
                    prev_was_keyword = false;
                    last_keyword = None;
                } else if s == "fn" {
                    prev_was_keyword = true;
                    if last_keyword.is_none() {
                        last_keyword = Some("fn".into());
                    }
                } else if s == "spec" || s == "proof" {
                    last_keyword = Some(s);
                } else {
                    last_keyword = None;
                }
            }
            proc_macro2::TokenTree::Group(g) => {
                // Recurse one level so we catch fn names inside braces.
                let inner = scan_item_names(g.stream());
                out.extend(inner);
                prev_was_keyword = false;
                last_keyword = None;
            }
            _ => {
                prev_was_keyword = false;
            }
        }
    }
    out
}

fn path_to_string(p: &syn::Path) -> String {
    let mut s = String::new();
    for (i, seg) in p.segments.iter().enumerate() {
        if i > 0 {
            s.push_str("::");
        }
        s.push_str(&seg.ident.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> syn::File {
        syn::parse_file(src).unwrap()
    }

    #[test]
    fn detects_verus_block_and_warns() {
        let src = r#"
            verus! {
                spec fn nonneg(x: int) -> bool {
                    x >= 0
                }

                proof fn lemma_add(a: int, b: int)
                    requires a >= 0, b >= 0
                    ensures a + b >= 0
                {
                }

                fn double(x: u32) -> (r: u32)
                    requires x < 1000
                    ensures r == 2 * x
                {
                    x + x
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0, "verus v0 lifts nothing");
        assert_eq!(out.seen, 1, "one verus block seen");
        assert!(
            !out.warnings.is_empty(),
            "expected at least one structured warning per item"
        );
        // We should have named at least one of the inner items.
        let any_named = out.warnings.iter().any(|w| {
            w.item_name.contains("nonneg")
                || w.item_name.contains("lemma_add")
                || w.item_name.contains("double")
        });
        assert!(any_named, "warnings should reference inner items: {:?}", out.warnings);
    }

    #[test]
    fn ignores_non_verus_macros() {
        let src = r#"
            other_macro! { stuff }

            verus! { fn f() {} }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        // Only the verus block contributes to seen/warnings.
        assert_eq!(out.seen, 1);
    }
}
