// Shared LSP annotation wire types.
//
// Host-language parsing lives in per-language LSP kits. This module only
// defines the normalized annotation shape consumed by the language-agnostic
// LSP coordinator.

use tower_lsp::lsp_types::Range;

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
