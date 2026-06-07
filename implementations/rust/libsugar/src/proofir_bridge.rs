// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sugar_ir_types::{Sort, Term};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

pub type ProofirTerm = Term;
pub type ResolvedSort = JsonValue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTerm {
    pub node: ResolvedNode,
    pub sort: ResolvedSort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResolvedNode {
    #[serde(rename = "concept:op-application")]
    OpApplication {
        op_definition_cid: String,
        args: Vec<ResolvedTerm>,
    },
    #[serde(rename = "literal")]
    Literal { value: JsonValue },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    #[error("unknown ProofIR op `{0}`")]
    UnknownOp(String),
    #[error("unknown ProofIR op CID `{0}`")]
    UnknownOpCid(String),
    #[error("arity mismatch: expected {expected}, got {actual}")]
    ArityMismatch { expected: usize, actual: usize },
    #[error("sort mismatch for `{op}` argument {index}: expected {expected}, got {actual}")]
    SortMismatch {
        op: String,
        index: usize,
        expected: ResolvedSort,
        actual: ResolvedSort,
    },
    #[error("malformed ProofIR term")]
    MalformedTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogLoadError {
    #[error("failed to read catalog file `{path}`: {kind:?}")]
    Io {
        path: PathBuf,
        kind: std::io::ErrorKind,
    },
    #[error("failed to parse catalog file `{path}`: {message}")]
    Json { path: PathBuf, message: String },
    #[error("bad cids.tsv row {line}: expected 4 tab-separated fields")]
    BadCidsRow { line: usize },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogIndex {
    ops: BTreeMap<String, CatalogOp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogOp {
    pub name: String,
    pub cid: String,
    pub formal_sorts: Option<Vec<ResolvedSort>>,
    pub return_sort: Option<ResolvedSort>,
}

pub fn proofir_resolve(
    term: &ProofirTerm,
    catalog: &CatalogIndex,
) -> Result<ResolvedTerm, BridgeError> {
    match term {
        Term::Const { value, sort } => Ok(ResolvedTerm {
            node: ResolvedNode::Literal {
                value: value.clone(),
            },
            sort: proofir_sort_to_resolved_sort(sort)?,
        }),
        Term::Ctor { name, args } => {
            let op = catalog
                .op(name)
                .ok_or_else(|| BridgeError::UnknownOp(name.clone()))?;

            if let Some(formal_sorts) = &op.formal_sorts {
                if formal_sorts.len() != args.len() {
                    return Err(BridgeError::ArityMismatch {
                        expected: formal_sorts.len(),
                        actual: args.len(),
                    });
                }
            }

            let mut lifted_args = Vec::with_capacity(args.len());
            for (index, arg) in args.iter().enumerate() {
                let lifted = proofir_resolve(arg, catalog)?;
                if let Some(expected) = op.formal_sorts.as_ref().and_then(|sorts| sorts.get(index))
                {
                    if &lifted.sort != expected {
                        return Err(BridgeError::SortMismatch {
                            op: name.clone(),
                            index,
                            expected: expected.clone(),
                            actual: lifted.sort,
                        });
                    }
                }
                lifted_args.push(lifted);
            }

            let sort = op.return_sort.clone().ok_or(BridgeError::MalformedTerm)?;
            Ok(ResolvedTerm {
                node: ResolvedNode::OpApplication {
                    op_definition_cid: op.cid.clone(),
                    args: lifted_args,
                },
                sort,
            })
        }
        Term::Var { .. } | Term::Lambda { .. } | Term::Let { .. } => {
            Err(BridgeError::MalformedTerm)
        }
    }
}

pub fn proofir_unresolve(
    term: &ResolvedTerm,
    catalog: &CatalogIndex,
) -> Result<ProofirTerm, BridgeError> {
    match &term.node {
        ResolvedNode::Literal { value } => Ok(Term::Const {
            value: value.clone(),
            sort: resolved_sort_to_proofir_sort(&term.sort)?,
        }),
        ResolvedNode::OpApplication {
            op_definition_cid,
            args,
        } => {
            let op = catalog
                .op_by_definition_cid(op_definition_cid)
                .ok_or_else(|| BridgeError::UnknownOpCid(op_definition_cid.clone()))?;
            let args = args
                .iter()
                .map(|arg| proofir_unresolve(arg, catalog))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(Term::Ctor {
                name: op.name.clone(),
                args,
            })
        }
    }
}

impl CatalogIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_op(
        &mut self,
        name: impl Into<String>,
        cid: impl Into<String>,
        formal_sorts: Option<Vec<ResolvedSort>>,
        return_sort: Option<ResolvedSort>,
    ) {
        let name = name.into();
        self.ops.insert(
            name.clone(),
            CatalogOp {
                name,
                cid: cid.into(),
                formal_sorts,
                return_sort,
            },
        );
    }

    pub fn op_definition_cid(&self, name: &str) -> Option<&str> {
        self.op(name).map(|op| op.cid.as_str())
    }

    pub fn get(&self, cid: &str) -> Option<&CatalogOp> {
        self.op_by_definition_cid(cid)
    }

    pub fn from_catalog_root(root: impl AsRef<Path>) -> Result<Self, CatalogLoadError> {
        let root = resolve_existing_path(root.as_ref());
        let index_path = root.join("index.json");
        let raw_index: RawCatalogIndex = read_json(&index_path)?;
        let mut catalog = CatalogIndex::new();

        for entry in raw_index.entries.values() {
            if entry.kind != "algorithm" {
                continue;
            }
            let envelope_path = root.join(&entry.path);
            let op = read_catalog_op(&envelope_path, &entry.name, &entry.cid)?;
            catalog.ops.insert(entry.name.clone(), op);
        }

        Ok(catalog)
    }

    pub fn from_cids_tsv(path: impl AsRef<Path>) -> Result<Self, CatalogLoadError> {
        let path = resolve_existing_path(path.as_ref());
        let text = read_to_string(&path)?;
        let mut catalog = CatalogIndex::new();

        for (line_index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(CatalogLoadError::BadCidsRow {
                    line: line_index + 1,
                });
            }

            let name = fields[1];
            let cid = fields[2];
            let payload_path = resolve_catalog_payload_path(&path, fields[3]);
            let op = read_catalog_op(&payload_path, name, cid)?;
            catalog.ops.insert(name.to_string(), op);
        }

        Ok(catalog)
    }

    fn op(&self, name: &str) -> Option<&CatalogOp> {
        self.ops.get(name)
    }

    fn op_by_definition_cid(&self, cid: &str) -> Option<&CatalogOp> {
        self.ops.values().find(|op| op.cid == cid)
    }
}

#[derive(Debug, Deserialize)]
struct RawCatalogIndex {
    entries: BTreeMap<String, RawCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct RawCatalogEntry {
    cid: String,
    kind: String,
    name: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RawEnvelope {
    memento: RawMemento,
}

#[derive(Debug, Deserialize)]
struct RawMemento {
    #[serde(default)]
    formal_sorts: Option<Vec<ResolvedSort>>,
    #[serde(default)]
    return_sort: Option<ResolvedSort>,
}

fn read_catalog_op(path: &Path, name: &str, cid: &str) -> Result<CatalogOp, CatalogLoadError> {
    let envelope: RawEnvelope = read_json(path)?;
    Ok(CatalogOp {
        name: name.to_string(),
        cid: cid.to_string(),
        formal_sorts: envelope
            .memento
            .formal_sorts
            .map(|sorts| sorts.into_iter().map(normalize_sort_value).collect()),
        return_sort: envelope.memento.return_sort.map(normalize_sort_value),
    })
}

fn proofir_sort_to_resolved_sort(sort: &Sort) -> Result<ResolvedSort, BridgeError> {
    match sort {
        Sort::Primitive { name } => Ok(json!({
            "args": [],
            "kind": "ctor",
            "name": name,
        })),
        Sort::Function { .. }
        | Sort::Dependent { .. }
        | Sort::Float { .. }
        | Sort::Region { .. } => serde_json::to_value(sort).map_err(|_| BridgeError::MalformedTerm),
    }
}

fn resolved_sort_to_proofir_sort(sort: &ResolvedSort) -> Result<Sort, BridgeError> {
    let mut sort = sort.clone();
    if let JsonValue::Object(object) = &mut sort {
        if object.get("kind") == Some(&JsonValue::String("ctor".to_string())) {
            let name = object
                .get("name")
                .and_then(JsonValue::as_str)
                .ok_or(BridgeError::MalformedTerm)?
                .to_string();
            let has_args = object
                .get("args")
                .and_then(JsonValue::as_array)
                .is_some_and(|args| !args.is_empty());
            if has_args {
                return Err(BridgeError::MalformedTerm);
            }

            return Ok(Sort::Primitive { name });
        }
    }

    serde_json::from_value(sort).map_err(|_| BridgeError::MalformedTerm)
}

fn normalize_sort_value(sort: ResolvedSort) -> ResolvedSort {
    let JsonValue::Object(mut object) = sort else {
        return sort;
    };

    if object.get("kind") == Some(&JsonValue::String("primitive".to_string())) {
        object.insert("kind".to_string(), JsonValue::String("ctor".to_string()));
        object
            .entry("args".to_string())
            .or_insert_with(|| JsonValue::Array(Vec::new()));
    }

    JsonValue::Object(object)
}

fn read_json<T>(path: &Path) -> Result<T, CatalogLoadError>
where
    T: for<'de> Deserialize<'de>,
{
    let text = read_to_string(path)?;
    serde_json::from_str(&text).map_err(|source| CatalogLoadError::Json {
        path: path.to_path_buf(),
        message: source.to_string(),
    })
}

fn read_to_string(path: &Path) -> Result<String, CatalogLoadError> {
    std::fs::read_to_string(path).map_err(|source| CatalogLoadError::Io {
        path: path.to_path_buf(),
        kind: source.kind(),
    })
}

fn resolve_existing_path(path: &Path) -> PathBuf {
    if path.exists() || path.is_absolute() {
        return path.to_path_buf();
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join(path);
            if candidate.exists() {
                return candidate;
            }
        }
    }

    path.to_path_buf()
}

fn resolve_catalog_payload_path(cids_path: &Path, payload_path: &str) -> PathBuf {
    let raw = PathBuf::from(payload_path);
    if raw.exists() || raw.is_absolute() {
        return raw;
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join(&raw);
            if candidate.exists() {
                return candidate;
            }
        }
    }

    for ancestor in cids_path.ancestors() {
        let candidate = ancestor.join(&raw);
        if candidate.exists() {
            return candidate;
        }
    }

    cids_path
        .parent()
        .map(|parent| parent.join(&raw))
        .unwrap_or(raw)
}
