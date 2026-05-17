// SPDX-License-Identifier: Apache-2.0
//
// ExamManifestKit: loads ExamManifestMemento via PEP 1.7.0 plugin protocol.
// NonCarrier conformance: produces a DomainClaim wrapping the loaded
// manifest; no target source emission.

use std::path::Path;

use provekit_ir_types::{ExamManifestMemento, IrFormula, IrTerm, Sort};
use serde_json::Value;

use crate::core::primitives::address;
use crate::core::traits::{Kit, KitError};
use crate::core::types::{
    memento_from_parts, Cid, ConformanceDeclaration, Dialect, DomainClaim, DomainKind, Input, Term,
    Verdict,
};

const EXAM_MANIFEST_CONFORMANCE_REASON: &str =
    "loads exam-manifest mementos via PEP 1.7.0; no target source emission";

pub struct ExamManifestKit {}

impl ExamManifestKit {
    pub const CONFORMANCE: ConformanceDeclaration = ConformanceDeclaration::NonCarrier {
        reason: EXAM_MANIFEST_CONFORMANCE_REASON,
    };

    pub fn new() -> Self {
        Self {}
    }

    pub fn load_path(&self, path: impl AsRef<Path>) -> Result<ExamManifestMemento, KitError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|error| {
            KitError::Transformation(format!("read exam manifest {}: {error}", path.display()))
        })?;
        let manifest: ExamManifestMemento = serde_json::from_str(&raw).map_err(|error| {
            KitError::Transformation(format!("parse exam manifest {}: {error}", path.display()))
        })?;
        validate_manifest(&manifest)?;
        Ok(manifest)
    }

    fn manifest_from_input(&self, input: &Input) -> Result<ExamManifestMemento, KitError> {
        let Input::Spec(value) = input else {
            return Err(KitError::UnsupportedInput {
                dialect: self.dialect(),
                message: "ExamManifestKit transform expects Input::Spec".to_string(),
            });
        };

        if let Some(path) = value
            .get("path")
            .and_then(Value::as_str)
            .or_else(|| value.as_str())
        {
            return self.load_path(path);
        }

        if value.get("cid").and_then(Value::as_str).is_some() {
            return Err(KitError::UnsupportedInput {
                dialect: self.dialect(),
                message: "ExamManifestKit CID lookup is provided by the dispatcher catalog path"
                    .to_string(),
            });
        }

        Err(KitError::UnsupportedInput {
            dialect: self.dialect(),
            message: "ExamManifestKit Input::Spec must be a path string or {\"path\": ...}"
                .to_string(),
        })
    }

    fn claim_from_manifest(
        &self,
        input: &Input,
        manifest: ExamManifestMemento,
    ) -> Result<DomainClaim, KitError> {
        let manifest_value = serde_json::to_value(&manifest).map_err(|error| {
            KitError::Transformation(format!("serialize exam manifest payload: {error}"))
        })?;
        let manifest_cid = Cid::parse(manifest.header.cid.clone())
            .map_err(|error| KitError::Transformation(error.to_string()))?;
        let payload = Term::Const {
            value: manifest_value.clone(),
            sort: primitive_sort("ExamManifestMemento"),
        };
        let contract = exam_manifest_contract(&manifest_value, &manifest_cid);

        Ok(DomainClaim {
            domain: DomainKind::Other("exam-manifest".to_string()),
            contract,
            artifacts: vec![manifest_cid.clone()],
            from: vec![address(input)],
            premises: vec![],
            to: manifest_cid,
            witness: None,
            payload: Some(payload),
            verdict: Verdict::Unresolved,
            attestation: None,
        })
    }
}

impl Default for ExamManifestKit {
    fn default() -> Self {
        Self::new()
    }
}

impl Kit for ExamManifestKit {
    fn dialect(&self) -> Dialect {
        Dialect::Other("exam-manifest".to_string())
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        let manifest = self.manifest_from_input(input)?;
        self.claim_from_manifest(input, manifest)
    }

    fn prove(&self, claim: DomainClaim) -> Result<DomainClaim, KitError> {
        Ok(claim)
    }

    fn parse(&self, input: &Input) -> Result<Term, KitError> {
        self.transform(input)?.payload.ok_or_else(|| {
            KitError::Serialization("exam manifest claim missing term payload".to_string())
        })
    }

    fn serialize(&self, term: &Term) -> Result<Input, KitError> {
        match term {
            Term::Const { value, .. } => Ok(Input::Spec(value.clone())),
            _ => Err(KitError::Serialization(
                "ExamManifestKit serialize expects a const manifest term".to_string(),
            )),
        }
    }
}

fn validate_manifest(manifest: &ExamManifestMemento) -> Result<(), KitError> {
    manifest
        .validate()
        .map_err(|error| KitError::Transformation(error.to_string()))?;
    let recomputed = manifest
        .recompute_header_cid()
        .map_err(|error| KitError::Transformation(error.to_string()))?;
    if recomputed != manifest.header.cid {
        return Err(KitError::Transformation(format!(
            "ExamManifestMemento header.cid mismatch: declared {}, recomputed {}",
            manifest.header.cid, recomputed
        )));
    }
    Ok(())
}

fn exam_manifest_contract(manifest_value: &Value, manifest_cid: &Cid) -> crate::core::Contract {
    let pre = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let post = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: "result".to_string(),
            },
            IrTerm::Const {
                value: manifest_value.clone(),
                sort: primitive_sort("ExamManifestMemento"),
            },
        ],
    };

    memento_from_parts(
        "exam-manifest::load".to_string(),
        vec!["source".to_string()],
        vec![primitive_sort("ExamManifestSource")],
        primitive_sort("ExamManifestMemento"),
        pre,
        post,
        Some(manifest_cid.as_str().to_string()),
    )
}

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}
