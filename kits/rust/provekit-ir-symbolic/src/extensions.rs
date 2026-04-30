//! Extension authoring + registry for the Rust kit.
//!
//! Per the IR extension protocol
//! (`docs/specs/2026-04-30-ir-extension-protocol.md`): kit authors
//! who want to introduce new sorts, predicates, or term constructors
//! do so through factory functions that register declarations in a
//! process-local registry. The same machinery covers the kit's
//! "built-in" primitives — most of which are not actually owned by
//! the Rust kit; their semantic authority lives in LLVM, the Rust
//! standard library, or whatever deeper layer ships them. The kit
//! BRIDGES to those layers via `primitive_bridge` rather than
//! claiming ownership.
//!
//! Two factory shapes:
//!
//! * `extension_sort` / `extension_predicate` / `extension_ctor` —
//!   kit OWNS the semantics. Use for kit-idiomatic primitives or
//!   user-authored extensions.
//!
//! * `primitive_bridge` — kit REFERENCES a deeper layer's authority.
//!   Use for things owned by LLVM / Rust core / std. The kit emits
//!   IR ctor nodes referencing the bridged name; the verifier
//!   resolves through the protocol's chain to the deeper layer's
//!   signed declaration.
//!
//! Both shapes share a registry. The IR doesn't care whether a
//! ctor name resolves to a kit-owned extension or a bridge to a
//! deeper layer; the verifier walks the registry and follows
//! whichever path the registered declaration carries.

use crate::types::{IrTerm, Sort};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// -- Shared types --------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SemanticDeclaration {
    SmtLibTheory {
        theory: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
    AxiomSet {
        axioms: Vec<serde_json::Value>,
    },
    ProofAssistant {
        system: String,
        identifier: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        proof_cid: Option<String>,
    },
    NaturalLanguage {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SortRef {
    Named(String),
    Sort(Sort),
}

impl SortRef {
    fn to_sort(&self) -> Sort {
        match self {
            SortRef::Named(name) => crate::types::sorts::primitive(name),
            SortRef::Sort(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "introduces", rename_all = "lowercase")]
pub enum ExtensionDeclaration {
    Sort {
        name: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        params: Vec<SortParam>,
        semantics: Vec<SemanticDeclaration>,
        compilers: Vec<String>,
    },
    Predicate {
        name: String,
        arg_sorts: Vec<SortRef>,
        semantics: Vec<SemanticDeclaration>,
        compilers: Vec<String>,
    },
    Ctor {
        name: String,
        arg_sorts: Vec<SortRef>,
        return_sort: SortRef,
        semantics: Vec<SemanticDeclaration>,
        compilers: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SortParam {
    pub name: String,
    pub param_sort: String,
}

impl ExtensionDeclaration {
    pub fn name(&self) -> &str {
        match self {
            ExtensionDeclaration::Sort { name, .. } => name,
            ExtensionDeclaration::Predicate { name, .. } => name,
            ExtensionDeclaration::Ctor { name, .. } => name,
        }
    }
}

// -- Bridge declaration --------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrimitiveBridgeDeclaration {
    pub ir_name: String,
    pub ir_arg_sorts: Vec<SortRef>,
    pub ir_return_sort: SortRef,
    pub source_layer: String,
    pub target_contract_cid: String,
    pub target_layer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// -- Errors --------------------------------------------------------

#[derive(Debug)]
pub enum RegistryError {
    Collision { name: String, kind: &'static str },
    BridgeCollision { ir_name: String },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Collision { name, kind } => write!(
                f,
                "extension {kind} \"{name}\" already registered with a different declaration"
            ),
            RegistryError::BridgeCollision { ir_name } => write!(
                f,
                "primitive bridge \"{ir_name}\" already registered with a different target"
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug)]
pub struct UnresolvedExtensionError {
    pub name: String,
    pub kind: &'static str,
    pub reason: &'static str,
}

impl std::fmt::Display for UnresolvedExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "extension {} \"{}\" did not resolve: {}",
            self.kind, self.name, self.reason
        )
    }
}

impl std::error::Error for UnresolvedExtensionError {}

// -- Registry state ------------------------------------------------

struct RegistryState {
    extensions_by_name: HashMap<String, ExtensionDeclaration>,
    bridges_by_name: HashMap<String, PrimitiveBridgeDeclaration>,
}

fn registry() -> &'static Mutex<RegistryState> {
    static REGISTRY: OnceLock<Mutex<RegistryState>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(RegistryState {
            extensions_by_name: HashMap::new(),
            bridges_by_name: HashMap::new(),
        })
    })
}

pub fn register_extension_declaration(decl: ExtensionDeclaration) -> Result<(), RegistryError> {
    let mut r = registry().lock().unwrap();
    if let Some(existing) = r.extensions_by_name.get(decl.name()) {
        if existing != &decl {
            return Err(RegistryError::Collision {
                name: decl.name().to_string(),
                kind: match &decl {
                    ExtensionDeclaration::Sort { .. } => "sort",
                    ExtensionDeclaration::Predicate { .. } => "predicate",
                    ExtensionDeclaration::Ctor { .. } => "ctor",
                },
            });
        }
        return Ok(());
    }
    r.extensions_by_name.insert(decl.name().to_string(), decl);
    Ok(())
}

pub fn lookup_extension(name: &str) -> Option<ExtensionDeclaration> {
    registry()
        .lock()
        .unwrap()
        .extensions_by_name
        .get(name)
        .cloned()
}

pub fn list_extensions() -> Vec<ExtensionDeclaration> {
    registry()
        .lock()
        .unwrap()
        .extensions_by_name
        .values()
        .cloned()
        .collect()
}

pub fn register_primitive_bridge(
    decl: PrimitiveBridgeDeclaration,
) -> Result<(), RegistryError> {
    let mut r = registry().lock().unwrap();
    if let Some(existing) = r.bridges_by_name.get(&decl.ir_name) {
        if existing != &decl {
            return Err(RegistryError::BridgeCollision {
                ir_name: decl.ir_name.clone(),
            });
        }
        return Ok(());
    }
    r.bridges_by_name.insert(decl.ir_name.clone(), decl);
    Ok(())
}

pub fn lookup_bridge(ir_name: &str) -> Option<PrimitiveBridgeDeclaration> {
    registry()
        .lock()
        .unwrap()
        .bridges_by_name
        .get(ir_name)
        .cloned()
}

pub fn list_bridges() -> Vec<PrimitiveBridgeDeclaration> {
    registry()
        .lock()
        .unwrap()
        .bridges_by_name
        .values()
        .cloned()
        .collect()
}

pub fn _reset_registry() {
    let mut r = registry().lock().unwrap();
    r.extensions_by_name.clear();
    r.bridges_by_name.clear();
}

// -- Authoring API: extensions -------------------------------------

pub struct ExtensionSortInput {
    pub name: String,
    pub params: Vec<SortParam>,
    pub semantics: Vec<SemanticDeclaration>,
    pub compilers: Vec<String>,
}

pub fn extension_sort(input: ExtensionSortInput) -> Sort {
    let decl = ExtensionDeclaration::Sort {
        name: input.name.clone(),
        params: input.params,
        semantics: input.semantics,
        compilers: input.compilers,
    };
    register_extension_declaration(decl).expect("extension_sort: registration failed");
    crate::types::sorts::primitive(&input.name)
}

pub struct ExtensionPredicateInput {
    pub name: String,
    pub arg_sorts: Vec<SortRef>,
    pub semantics: Vec<SemanticDeclaration>,
    pub compilers: Vec<String>,
}

pub fn extension_predicate(input: ExtensionPredicateInput) -> ExtensionPredicateBuilder {
    let decl = ExtensionDeclaration::Predicate {
        name: input.name.clone(),
        arg_sorts: input.arg_sorts.clone(),
        semantics: input.semantics,
        compilers: input.compilers,
    };
    register_extension_declaration(decl).expect("extension_predicate: registration failed");
    ExtensionPredicateBuilder { name: input.name }
}

pub struct ExtensionPredicateBuilder {
    name: String,
}

impl ExtensionPredicateBuilder {
    pub fn call(&self, args: Vec<IrTerm>) -> crate::types::IrFormula {
        crate::types::IrFormula::atomic(&self.name, args)
    }
}

pub struct ExtensionCtorInput {
    pub name: String,
    pub arg_sorts: Vec<SortRef>,
    pub return_sort: SortRef,
    pub semantics: Vec<SemanticDeclaration>,
    pub compilers: Vec<String>,
}

pub fn extension_ctor(input: ExtensionCtorInput) -> ExtensionCtorBuilder {
    let decl = ExtensionDeclaration::Ctor {
        name: input.name.clone(),
        arg_sorts: input.arg_sorts.clone(),
        return_sort: input.return_sort.clone(),
        semantics: input.semantics,
        compilers: input.compilers,
    };
    register_extension_declaration(decl).expect("extension_ctor: registration failed");
    let return_sort = input.return_sort.to_sort();
    ExtensionCtorBuilder {
        name: input.name,
        return_sort,
    }
}

pub struct ExtensionCtorBuilder {
    name: String,
    return_sort: Sort,
}

impl ExtensionCtorBuilder {
    pub fn call(&self, args: Vec<IrTerm>) -> IrTerm {
        IrTerm::ctor(&self.name, args, self.return_sort.clone())
    }
}

// -- Authoring API: primitive bridges ------------------------------

pub struct PrimitiveBridgeInput {
    pub ir_name: String,
    pub ir_arg_sorts: Vec<SortRef>,
    pub ir_return_sort: SortRef,
    pub source_layer: String,
    pub target_contract_cid: String,
    pub target_layer: String,
    pub notes: Option<String>,
}

pub fn primitive_bridge(input: PrimitiveBridgeInput) -> PrimitiveBridgeBuilder {
    let decl = PrimitiveBridgeDeclaration {
        ir_name: input.ir_name.clone(),
        ir_arg_sorts: input.ir_arg_sorts.clone(),
        ir_return_sort: input.ir_return_sort.clone(),
        source_layer: input.source_layer,
        target_contract_cid: input.target_contract_cid,
        target_layer: input.target_layer,
        notes: input.notes,
    };
    register_primitive_bridge(decl).expect("primitive_bridge: registration failed");
    PrimitiveBridgeBuilder {
        name: input.ir_name,
        return_sort: input.ir_return_sort.to_sort(),
    }
}

pub struct PrimitiveBridgeBuilder {
    name: String,
    return_sort: Sort,
}

impl PrimitiveBridgeBuilder {
    pub fn call(&self, args: Vec<IrTerm>) -> IrTerm {
        IrTerm::ctor(&self.name, args, self.return_sort.clone())
    }
}
