// Rust source parser for ProvekIt annotations.
//
// Uses `syn` to walk the AST and extract:
//   - #[provekit::implement(target = "...")]
//   - #[provekit::contract(...)]
//   - #[provekit::verify]
//
// Returns a SourceAnnotations struct mapping file positions to
// annotation metadata.

use proc_macro2::Span;
use quote::ToTokens;
use syn::visit::Visit;
use syn::{Attribute, ItemFn, Meta};
use tower_lsp::lsp_types::{Position, Range};

#[derive(Debug, Clone)]
pub struct SourceAnnotations {
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub function_name: String,
    pub kind: AnnotationKind,
    pub target_cid: Option<String>,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum AnnotationKind {
    Implement { target_cid: String },
    Contract,
    Verify,
}

/// Parse a Rust source file and extract all provekit annotations.
pub fn parse_rust_source(text: &str) -> SourceAnnotations {
    let file = match syn::parse_file(text) {
        Ok(f) => f,
        Err(_) => {
            return SourceAnnotations {
                annotations: Vec::new(),
            }
        }
    };

    let mut visitor = AnnotationVisitor {
        annotations: Vec::new(),
    };
    visitor.visit_file(&file);
    SourceAnnotations {
        annotations: visitor.annotations,
    }
}

struct AnnotationVisitor {
    annotations: Vec<Annotation>,
}

impl<'ast> Visit<'ast> for AnnotationVisitor {
    fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
        let function_name = item_fn.sig.ident.to_string();
        let span = item_fn.sig.ident.span();
        let range = span_to_range(span);

        for attr in &item_fn.attrs {
            if let Some(kind) = parse_provekit_attr(attr) {
                let target_cid = match &kind {
                    AnnotationKind::Implement { target_cid } => Some(target_cid.clone()),
                    _ => None,
                };

                self.annotations.push(Annotation {
                    function_name: function_name.clone(),
                    kind,
                    target_cid,
                    range,
                });
            }
        }

        // Continue visiting nested functions
        syn::visit::visit_item_fn(self, item_fn);
    }
}

fn parse_provekit_attr(attr: &Attribute) -> Option<AnnotationKind> {
    // Match #[provekit::implement(...)] or #[implement(...)]
    let path_str = attr.path().to_token_stream().to_string();

    if path_str == "provekit :: implement" || path_str == "implement" {
        if let Meta::List(list) = &attr.meta {
            let inner = list.tokens.to_string();
            // Parse target = "..."
            if let Some(cid) = parse_target_cid(&inner) {
                return Some(AnnotationKind::Implement { target_cid: cid });
            }
        }
    }

    if path_str == "provekit :: contract" || path_str == "contract" {
        return Some(AnnotationKind::Contract);
    }

    if path_str == "provekit :: verify" || path_str == "verify" {
        return Some(AnnotationKind::Verify);
    }

    None
}

fn parse_target_cid(s: &str) -> Option<String> {
    // Very simple parser: looks for target = "..."
    let parts: Vec<&str> = s.split('=').collect();
    if parts.len() >= 2 {
        let value = parts[1].trim().trim_matches(',').trim();
        if value.starts_with('"') && value.ends_with('"') {
            return Some(value[1..value.len() - 1].to_string());
        }
    }
    None
}

fn span_to_range(span: Span) -> Range {
    // proc_macro2::Span gives us byte offsets; we need line/col
    // For a first pass, use line start/end from the span
    let start = span.start();
    let end = span.end();

    Range {
        start: Position {
            line: start.line as u32 - 1,
            character: start.column as u32,
        },
        end: Position {
            line: end.line as u32 - 1,
            character: end.column as u32,
        },
    }
}
