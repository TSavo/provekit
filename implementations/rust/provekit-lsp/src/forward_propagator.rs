/// ForwardPropagator — accumulate posts and emit implication-check diagnostics.
/// Per: docs/lsp/forward-propagation-floor-v1.md

#[derive(Debug, Clone)]
pub struct Post {
    pub constraints: Vec<String>,
    pub is_top: bool,
}

impl Post {
    pub fn top() -> Self {
        Self { constraints: vec![], is_top: true }
    }

    pub fn of(constraint: impl Into<String>) -> Self {
        Self { constraints: vec![constraint.into()], is_top: false }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct ForwardPropagator {
    seed_catalog: std::collections::HashMap<String, Post>,
}

impl ForwardPropagator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_to_catalog(&mut self, callee_id: &str, _pre: Post, post: Post) {
        self.seed_catalog.insert(callee_id.to_string(), post);
    }

    pub fn check_callsite(&self, callee_id: &str, current_post: &Post) -> Option<DiagnosticResult> {
        if current_post.is_top {
            return None;
        }
        let callee_pre = self.seed_catalog.get(callee_id)?;
        for constraint in &current_post.constraints {
            if !callee_pre.constraints.contains(constraint) {
                return Some(DiagnosticResult {
                    code: "implication-failed".to_string(),
                    message: format!(
                        "post does not imply callee pre: {}",
                        callee_pre.constraints.join(" && ")
                    ),
                });
            }
        }
        None
    }
}