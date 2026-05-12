// SPDX-License-Identifier: Apache-2.0
//
// Attribute lifter.
//
// Reads `#[requires(...)]` / `#[ensures(...)]` attributes (and their
// `cfg_attr(any(), requires(...))` no-op-gated form) from a syn fn,
// returning the predicate text. The text round-trips into the
// rewritten output verbatim so the next pass re-lifts it identically.
//
// Also reads `// concept: <name>` line comments above a function so
// the second pass can pick up the human-supplied concept name.

use proc_macro2::TokenTree;

#[derive(Debug, Clone, Default)]
pub struct ExtractedContract {
    pub pre: Option<String>,
    pub post: Option<String>,
}

pub fn extract_contract_attrs(attrs: &[syn::Attribute]) -> ExtractedContract {
    let mut out = ExtractedContract::default();
    for attr in attrs {
        // Case 1: bare `#[requires(...)]` / `#[ensures(...)]`.
        if let Some(name) = attr.path().get_ident().map(|i| i.to_string()) {
            if let Some(text) = parens_tokens_to_string(&attr.meta) {
                match name.as_str() {
                    "requires" => {
                        if out.pre.is_none() {
                            out.pre = Some(text);
                        }
                    }
                    "ensures" => {
                        if out.post.is_none() {
                            out.post = Some(text);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Case 2: `#[cfg_attr(any(), requires(...))]` /
        //          `#[cfg_attr(any(), ensures(...))]`.
        //
        // We only honor the form whose predicate is exactly `any()` so
        // the smoke test's "make annotations inert under rustc" trick
        // is what gets lifted, but a genuine conditional contract
        // (`cfg_attr(test, ...)`) is NOT picked up.
        if attr.path().is_ident("cfg_attr") {
            if let Some((pred, kind, text)) = parse_cfg_attr_contract(attr) {
                if pred == "any" {
                    match kind.as_str() {
                        "requires" => {
                            if out.pre.is_none() {
                                out.pre = Some(text);
                            }
                        }
                        "ensures" => {
                            if out.post.is_none() {
                                out.post = Some(text);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

/// Extract `<predicate-ident>` and the inner contract `kind`+`text` from
/// `#[cfg_attr(<predicate>(), <kind>(<text>))]`.
fn parse_cfg_attr_contract(attr: &syn::Attribute) -> Option<(String, String, String)> {
    // Pull token-stream of the `(...)` body.
    let list = match &attr.meta {
        syn::Meta::List(l) => l,
        _ => return None,
    };
    let mut iter = list.tokens.clone().into_iter().peekable();

    // First token: predicate ident, e.g. `any`.
    let pred_ident = match iter.next()? {
        TokenTree::Ident(i) => i.to_string(),
        _ => return None,
    };
    // Optional `()` after predicate.
    if let Some(TokenTree::Group(_)) = iter.peek() {
        iter.next();
    }
    // Comma.
    match iter.next()? {
        TokenTree::Punct(p) if p.as_char() == ',' => {}
        _ => return None,
    }
    // Kind ident: `requires` / `ensures`.
    let kind = match iter.next()? {
        TokenTree::Ident(i) => i.to_string(),
        _ => return None,
    };
    // Parenthesized body.
    let body = match iter.next()? {
        TokenTree::Group(g) => {
            // Strip outer parens by taking the stream.
            g.stream().to_string()
        }
        _ => return None,
    };

    Some((pred_ident, kind, normalize_whitespace(&body)))
}

fn parens_tokens_to_string(meta: &syn::Meta) -> Option<String> {
    match meta {
        syn::Meta::List(l) => Some(normalize_whitespace(&l.tokens.to_string())),
        _ => None,
    }
}

fn normalize_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Scan the source for `// concept: <name>` immediately above the
/// function declaration line. Returns the trimmed `<name>`.
///
/// Format contract (documented in the report):
///
///     // concept: <name>
///     fn the_function(...) { ... }
///
/// A blank line or non-`// concept:` comment in between breaks the
/// match. The `<name>` is the bare token sequence (no leading
/// `concept:` prefix); whitespace is trimmed.
pub fn extract_concept_annotation(src: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {}(", fn_name);
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&needle) {
            // Walk upward across substrate-emitted attribute and comment
            // lines until either the concept annotation is found or a
            // line that breaks the substrate-emitted block is reached
            // (a blank line, a doc comment, ordinary code).
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if let Some(rest) = prev.strip_prefix("// concept:") {
                    let trimmed = rest.trim().to_string();
                    // Substrate-placeholder names are NOT human input.
                    // The lifter must treat them as "still unnamed" so
                    // the next pass surfaces the cluster again, ready
                    // for a real human edit.
                    if trimmed.starts_with("UNNAMED-CONCEPT-") {
                        return None;
                    }
                    return Some(trimmed);
                }
                if prev.starts_with("#[") || prev.starts_with("#![") {
                    j -= 1;
                    continue;
                }
                if prev.starts_with("// substrate-origin:")
                    || prev.starts_with("// memento-cid:")
                    || prev.starts_with("// witness-inherited-from:")
                {
                    j -= 1;
                    continue;
                }
                break;
            }
            return None;
        }
    }
    None
}
