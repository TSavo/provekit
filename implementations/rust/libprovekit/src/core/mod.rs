// SPDX-License-Identifier: Apache-2.0

//! Core ProvekIt interface.
//!
//! This module lays down the eight primitive operations as Rust traits,
//! structs, and functions. The familiar verbs are intentionally thin
//! compositions over those primitives:
//!
//! - `transform = parse ; project ; address`
//! - `prove = transform ; discharge(Search)`
//! - `verify = address-recompute ; check-signature ; discharge(Check)`
//! - `realize = serialize` when the faithful term is still present
//! - `cross_compile = transform ; compose ; serialize`
//! - `link = fold(compose)`
//!
//! `mint` is `transform` on an [`Input::Spec`]. `pattern_scan` is a catalog
//! filter over [`DomainClaim`]s followed by `discharge` of matched
//! composition obligations. `commit` is `compose(parent, change)` followed by
//! `sign`.

pub mod bind;
pub mod lift_plugin;
pub mod lower_plugin;
pub mod path_executor;
pub mod platform_semantics;
pub mod platform_semantics_loader;
pub mod primitives;
pub mod prove_kit;
pub mod source_transform;
pub mod source_transform_kit;
pub mod stubs;
pub mod traits;
pub mod types;
pub mod verbs;
pub mod walks;

pub use crate::exam_manifest::ExamManifestKit;
pub use bind::{
    bind_result_payload, bind_term_document, concept_bind_result_cid, named_term_document_cid,
    named_term_document_from_bind_payload, strip_realize_sidecar_from_lift_term,
    BindContractWitness, BindError, BindKit, BindLiftEntry, BindOptions, CandidateCluster,
    CandidateClusterManifest, NamedTerm, NamedTermDocument, NamedTermTree, NamedWitness,
};
pub use lift_plugin::{LiftKit, LiftPluginKit, LiftPluginKitError, LiftPluginKitSession};
pub use lower_plugin::{
    LowerKit, RealizeContractPayload, RealizeContractWitness, RealizeRequest, RealizeTransport,
    RealizedSource,
};
pub use path_executor::{execute_path, KitRegistry, PathExecutionChain, PathExecutionError};
pub use source_transform_kit::{
    decode_source_transform_payload, SourceTransformAdapter, SOURCE_TRANSFORM_PAYLOAD_SORT,
};
pub use platform_semantics::{platform_semantics_for_binding, platform_semantics_for_lower_target};
pub use primitives::{
    address, compose, dropper, resolve, sign, verify_sig, ComposeError, SigningKey,
};
pub use prove_kit::ProveKit;
pub use stubs::{CKit, FunctionContractDomain, NoopPortfolio, RustKit};
pub use traits::{
    Canonical, Catalog, CoreError, Domain, DomainError, HashMapCatalog, HashMapInputCatalog,
    InputCatalog, Kit, KitError, Portfolio,
};
pub use types::{
    ArityShape, AritySlot, Attestation, Boundary, ChainIntegrityFailureWitness,
    ChainIntegrityWitness, Cid, CidError, ConformanceDeclaration, Contract, Dialect,
    DivergenceCharacterization, DomainClaim, DomainKind, Formula, Input, LanguageSignature,
    OpCoverageVerdict, OperationSignature, Path, PathAlgebra, PathDocument, PathDocumentError,
    PathError, PathInputBinding, PathInputMaterial, PlatformSemanticComparisonError,
    PlatformSemanticsDeclaration, Refutation, Side, SignatureCatalogError, SlotEvaluation,
    SlotSort, Term, Truth, Verb, Verdict, VerdictCoercionError, Witness,
};
pub use verbs::{cross_compile, link, prove, realize, transform, verify};
pub use walks::{
    assert_concept_tier, assert_concept_tier_with_exam_manifest, walk_premises_to_root,
    walk_premises_to_root_with_failure_steps, ChainBreak, ChainWalkFailure, HubMissingNode,
};
