// SPDX-License-Identifier: Apache-2.0
//
// Source-locus metadata for arrivals, contracts, and other mementos.
// Per #372 part 1.
//
// Locus is `(file, line, col)` plus an optional byte-offset range.
// Required for downstream developer-feedback paths ("compile error
// at <file>:<line>: missing edge") and for the substrate's
// cross-reference between mementos and source.
//
// We use proc-macro2's `span-locations` feature, which gives us
// `Span::start() -> LineColumn` after parsing source through syn.
// The file path is supplied by the caller (the parser doesn't know
// where the bytes came from).

use std::sync::Arc;

use proc_macro2::Span;
use provekit_canonicalizer::Value;

/// One source location. `file` is whatever the caller passed in; if
/// None, the locus is from in-memory source or untracked input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Locus {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

impl Locus {
    /// Build from a syn::Span and an optional file path.
    pub fn from_span(span: Span, file: Option<&str>) -> Self {
        let start = span.start();
        Self {
            file: file.map(|s| s.to_string()),
            line: start.line,
            col: start.column,
        }
    }

    /// Empty/unknown locus.
    pub fn unknown() -> Self {
        Self::default()
    }

    pub fn is_unknown(&self) -> bool {
        self.file.is_none() && self.line == 0 && self.col == 0
    }

    pub fn to_value(&self) -> Arc<Value> {
        Value::object([
            (
                "file",
                match &self.file {
                    Some(p) => Value::string(p.clone()),
                    None => Value::null(),
                },
            ),
            ("line", Value::integer(self.line as i64)),
            ("col", Value::integer(self.col as i64)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locus_extracts_line_col_from_span() {
        // Parse a small source string and extract the locus of the
        // function ident.
        let src = "fn foo() {}\nfn bar() {}\n";
        let file: syn::File = syn::parse_str(src).unwrap();
        let bar_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "bar" => Some(f),
                _ => None,
            })
            .unwrap();
        let span = syn::spanned::Spanned::span(&bar_fn.sig.ident);
        let locus = Locus::from_span(span, Some("test.rs"));
        assert_eq!(locus.file.as_deref(), Some("test.rs"));
        // `bar` is on line 2.
        assert_eq!(locus.line, 2);
    }

    #[test]
    fn locus_unknown_round_trips_through_canonical() {
        let l = Locus::unknown();
        assert!(l.is_unknown());
        let v = l.to_value();
        // Non-empty Value (object with file=null, line=0, col=0).
        let bytes = provekit_canonicalizer::encode_jcs(&v);
        assert!(bytes.contains("\"file\":null"));
        assert!(bytes.contains("\"line\":0"));
    }
}
