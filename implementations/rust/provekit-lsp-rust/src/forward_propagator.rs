// SPDX-License-Identifier: Apache-2.0
//
// Thin forward-propagation core for the Rust LSP plugin.
//
// This module intentionally models the v1.0.0 floor as a small statement IR.
// Parser-specific lowering can stay separate while the implication loop,
// branch merge, diagnostic shape, and top fallback remain easy to test.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

const PROTOCOL_CATALOG_CID: &str = "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    constraints: Vec<String>,
    is_top: bool,
}

impl Post {
    pub fn known<const N: usize>(constraints: [&str; N]) -> Self {
        Self::from_strings(constraints.into_iter().map(String::from), false)
    }

    pub fn empty() -> Self {
        Self {
            constraints: Vec::new(),
            is_top: false,
        }
    }

    pub fn top() -> Self {
        Self {
            constraints: Vec::new(),
            is_top: true,
        }
    }

    pub fn is_top(&self) -> bool {
        self.is_top
    }

    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    fn from_strings<I>(constraints: I, is_top: bool) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        if is_top {
            return Self::top();
        }
        let mut set = BTreeSet::new();
        for constraint in constraints {
            if !constraint.is_empty() {
                set.insert(constraint);
            }
        }
        Self {
            constraints: set.into_iter().collect(),
            is_top: false,
        }
    }

    fn combine(&self, next: &Post) -> Post {
        if self.is_top || next.is_top {
            return Post::top();
        }
        Post::from_strings(
            self.constraints
                .iter()
                .cloned()
                .chain(next.constraints.iter().cloned()),
            false,
        )
    }

    fn branch_merge(&self, other: &Post) -> Post {
        if self.is_top || other.is_top {
            return Post::top();
        }
        let other_constraints: BTreeSet<_> = other.constraints.iter().collect();
        Post::from_strings(
            self.constraints
                .iter()
                .filter(|constraint| other_constraints.contains(constraint))
                .cloned(),
            false,
        )
    }

    fn cid(&self) -> String {
        if self.is_top {
            return cid_for_bytes(b"post:top");
        }
        let payload = format!("post:known:{}", self.constraints.join("\n"));
        cid_for_bytes(payload.as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Reset,
    Assign {
        post: Post,
    },
    Call {
        callee_id: String,
        range: LspRange,
    },
    IfElse {
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
    },
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

impl LspRange {
    pub fn single_line(line: u32, start_character: u32, end_character: u32) -> Self {
        Self {
            start: LspPosition {
                line,
                character: start_character,
            },
            end: LspPosition {
                line,
                character: end_character,
            },
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "start": {
                "line": self.start.line,
                "character": self.start.character,
            },
            "end": {
                "line": self.end.line,
                "character": self.end.character,
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineEntry {
    pub callee_id: String,
    pub pre: Option<Post>,
    pub post: Option<Post>,
    pub contract_name: String,
    pub member_cid: String,
    pub contract_cid: String,
    pub attestation_cid: String,
    pub pre_cid: String,
    pub post_cid: String,
    pub signer: String,
    pub signer_role: String,
    pub baseline_catalog_cid: String,
    pub baseline_contract_set_cid: String,
    pub baseline_index_cid: String,
    pub protocol_catalog_cid: String,
}

impl BaselineEntry {
    pub fn new(callee_id: impl Into<String>, pre: Option<Post>, post: Option<Post>) -> Self {
        let callee_id = callee_id.into();
        let contract_name = format!("rust_baseline_{}", sanitize_identifier(&callee_id));
        let pre_cid = pre
            .as_ref()
            .map(Post::cid)
            .unwrap_or_else(|| cid_for_bytes(format!("{callee_id}:pre:none").as_bytes()));
        let post_cid = post
            .as_ref()
            .map(Post::cid)
            .unwrap_or_else(|| cid_for_bytes(format!("{callee_id}:post:none").as_bytes()));
        let seed = format!("{callee_id}|{pre_cid}|{post_cid}");

        Self {
            callee_id,
            pre,
            post,
            contract_name,
            member_cid: cid_for_bytes(format!("member:{seed}").as_bytes()),
            contract_cid: cid_for_bytes(format!("contract:{seed}").as_bytes()),
            attestation_cid: cid_for_bytes(format!("attestation:{seed}").as_bytes()),
            pre_cid,
            post_cid,
            signer: "ed25519:foundation-v0".into(),
            signer_role: "foundation-baseline".into(),
            baseline_catalog_cid: cid_for_bytes(format!("baseline-catalog:{seed}").as_bytes()),
            baseline_contract_set_cid: cid_for_bytes(
                format!("baseline-contract-set:{seed}").as_bytes(),
            ),
            baseline_index_cid: cid_for_bytes(format!("baseline-index:{seed}").as_bytes()),
            protocol_catalog_cid: PROTOCOL_CATALOG_CID.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticData {
    pub schema_version: u32,
    pub kind: String,
    pub callee: String,
    pub callee_contract_cid: String,
    pub callee_attestation_cid: String,
    pub callee_pre_cid: String,
    pub callee_post_cid: String,
    pub current_post_cid: String,
    pub missing_conjuncts: Vec<String>,
    pub signer: String,
    pub signer_role: String,
    pub baseline_catalog_cid: String,
    pub baseline_contract_set_cid: String,
    pub baseline_index_cid: String,
    pub protocol_catalog_cid: String,
}

impl DiagnosticData {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.schema_version,
            "kind": self.kind,
            "callee": self.callee,
            "callee_contract_cid": self.callee_contract_cid,
            "callee_attestation_cid": self.callee_attestation_cid,
            "callee_pre_cid": self.callee_pre_cid,
            "callee_post_cid": self.callee_post_cid,
            "current_post_cid": self.current_post_cid,
            "missing_conjuncts": self.missing_conjuncts,
            "signer": self.signer,
            "signer_role": self.signer_role,
            "baseline_catalog_cid": self.baseline_catalog_cid,
            "baseline_contract_set_cid": self.baseline_contract_set_cid,
            "baseline_index_cid": self.baseline_index_cid,
            "protocol_catalog_cid": self.protocol_catalog_cid,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LspDiagnostic {
    pub range: LspRange,
    pub severity: u8,
    pub source: String,
    pub code: String,
    pub message: String,
    pub data: DiagnosticData,
}

impl LspDiagnostic {
    pub fn to_lsp_json(&self) -> serde_json::Value {
        serde_json::json!({
            "range": self.range.to_json(),
            "severity": self.severity,
            "source": self.source,
            "code": self.code,
            "message": self.message,
            "data": self.data.to_json(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ForwardPropagator {
    index: BTreeMap<String, BaselineEntry>,
}

impl ForwardPropagator {
    pub fn new<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = BaselineEntry>,
    {
        let index = entries
            .into_iter()
            .map(|entry| (entry.callee_id.clone(), entry))
            .collect();
        Self { index }
    }

    pub fn floor_v1_seed_index() -> Self {
        Self::new([BaselineEntry::new(
            "checkPositive",
            Some(Post::known(["x > 0"])),
            Some(Post::known(["returns true"])),
        )])
    }

    pub fn lower_floor_source(source: &str) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        let mut brace_depth = 0i32;
        let mut top_block_depth: Option<i32> = None;

        for (line_idx, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            let is_function_definition = trimmed.starts_with("fn ");
            if is_function_definition {
                stmts.push(Stmt::Reset);
                top_block_depth = None;
            }

            if starts_top_fallback_block(trimmed) {
                let opens = line.matches('{').count() as i32;
                let closes = line.matches('}').count() as i32;
                top_block_depth = Some(brace_depth + opens - closes);
                if top_block_depth == Some(brace_depth) {
                    top_block_depth = Some(brace_depth + 1);
                }
            }

            if !is_function_definition {
                for (start, arg) in check_positive_calls(line) {
                    let range = LspRange::single_line(
                        line_idx as u32,
                        start as u32,
                        (start + "checkPositive".len()) as u32,
                    );

                    if top_block_depth.is_some() {
                        stmts.push(Stmt::Unsupported);
                    } else {
                        stmts.push(Stmt::Assign {
                            post: post_for_check_positive_arg(&arg),
                        });
                    }

                    stmts.push(Stmt::Call {
                        callee_id: "checkPositive".into(),
                        range,
                    });
                }
            }

            brace_depth += line.matches('{').count() as i32;
            brace_depth -= line.matches('}').count() as i32;
            if let Some(depth) = top_block_depth {
                if brace_depth < depth {
                    top_block_depth = None;
                }
            }
        }

        stmts
    }

    pub fn emit_diagnostics(&self, function_body: &[Stmt]) -> Vec<LspDiagnostic> {
        let mut diagnostics = Vec::new();
        let _ = self.walk_block(function_body, Post::empty(), &mut diagnostics);
        diagnostics
    }

    pub fn check_callsite(
        &self,
        callee_id: &str,
        current_post: &Post,
        range: LspRange,
    ) -> Option<LspDiagnostic> {
        if current_post.is_top() {
            return None;
        }

        let entry = self.index.get(callee_id)?;
        let pre = entry.pre.as_ref()?;
        let current_constraints: BTreeSet<_> = current_post.constraints().iter().collect();
        let missing_conjuncts: Vec<String> = pre
            .constraints()
            .iter()
            .filter(|constraint| !current_constraints.contains(constraint))
            .cloned()
            .collect();

        if missing_conjuncts.is_empty() {
            return None;
        }

        Some(LspDiagnostic {
            range,
            severity: 1,
            source: "provekit".into(),
            code: "implication-failed".into(),
            message: "callee precondition not established at this callsite".into(),
            data: DiagnosticData {
                schema_version: 1,
                kind: "provekit.lsp.implication_failed".into(),
                callee: entry.callee_id.clone(),
                callee_contract_cid: entry.contract_cid.clone(),
                callee_attestation_cid: entry.attestation_cid.clone(),
                callee_pre_cid: entry.pre_cid.clone(),
                callee_post_cid: entry.post_cid.clone(),
                current_post_cid: current_post.cid(),
                missing_conjuncts,
                signer: entry.signer.clone(),
                signer_role: entry.signer_role.clone(),
                baseline_catalog_cid: entry.baseline_catalog_cid.clone(),
                baseline_contract_set_cid: entry.baseline_contract_set_cid.clone(),
                baseline_index_cid: entry.baseline_index_cid.clone(),
                protocol_catalog_cid: entry.protocol_catalog_cid.clone(),
            },
        })
    }

    fn walk_block(
        &self,
        body: &[Stmt],
        start_post: Post,
        diagnostics: &mut Vec<LspDiagnostic>,
    ) -> Post {
        let mut current_post = start_post;

        for stmt in body {
            match stmt {
                Stmt::Reset => {
                    current_post = Post::empty();
                }
                Stmt::Assign { post } => {
                    current_post = current_post.combine(post);
                }
                Stmt::Call { callee_id, range } => {
                    if let Some(diagnostic) =
                        self.check_callsite(callee_id, &current_post, range.clone())
                    {
                        diagnostics.push(diagnostic);
                    }
                    current_post = match self
                        .index
                        .get(callee_id)
                        .and_then(|entry| entry.post.as_ref())
                    {
                        Some(post) => current_post.combine(post),
                        None if self.index.contains_key(callee_id) => current_post,
                        None => Post::top(),
                    };
                }
                Stmt::IfElse {
                    then_branch,
                    else_branch,
                } => {
                    let then_post = self.walk_block(then_branch, current_post.clone(), diagnostics);
                    let else_post = self.walk_block(else_branch, current_post.clone(), diagnostics);
                    current_post = then_post.branch_merge(&else_post);
                }
                Stmt::Unsupported => {
                    current_post = Post::top();
                }
            }
        }

        current_post
    }
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn starts_top_fallback_block(trimmed: &str) -> bool {
    trimmed.starts_with("for ")
        || trimmed.starts_with("for(")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("while(")
        || trimmed.starts_with("loop ")
        || trimmed.starts_with("loop{")
}

fn check_positive_calls(line: &str) -> Vec<(usize, String)> {
    let mut calls = Vec::new();
    let mut search_from = 0usize;
    while let Some(relative_start) = line[search_from..].find("checkPositive(") {
        let start = search_from + relative_start;
        let args_start = start + "checkPositive(".len();
        if let Some(relative_end) = line[args_start..].find(')') {
            let end = args_start + relative_end;
            calls.push((start, line[args_start..end].trim().to_string()));
            search_from = end + 1;
        } else {
            break;
        }
    }
    calls
}

fn post_for_check_positive_arg(arg: &str) -> Post {
    match arg.parse::<i64>() {
        Ok(value) if value > 0 => Post::known(["x > 0"]),
        Ok(_) => Post::known(["x <= 0"]),
        Err(_) => Post::top(),
    }
}

fn cid_for_bytes(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut reader = hasher.finalize_xof();
    let mut out = [0u8; 64];
    reader.fill(&mut out);

    let mut cid = String::from("blake3-512:");
    for byte in out {
        let _ = write!(&mut cid, "{byte:02x}");
    }
    cid
}
