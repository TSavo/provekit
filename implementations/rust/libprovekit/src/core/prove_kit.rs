// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::fmt;

use super::traits::{Catalog, Kit, KitError};
use super::types::{
    domain_claim_from_canonical_bytes, ChainIntegrityFailureWitness, ChainIntegrityWitness, Cid,
    ConformanceDeclaration, Dialect, DomainClaim, Input, Term, Verdict, Witness,
};

const CHAIN_INTEGRITY_SCHEMA_VERSION: u32 = 1;
const PROVEKIT_CONFORMANCE_REASON: &str =
    "discharges claims via chain-integrity verification; no source emission";

/// Built-in kit that discharges claims by walking their premise chain to a root CID.
pub struct ProveKit {
    origin_cid: Cid,
    catalog: Box<dyn Catalog>,
    expect_cycle: bool,
}

impl ProveKit {
    /// Public registry declaration for the built-in `prove` kit.
    pub const CONFORMANCE: ConformanceDeclaration = ConformanceDeclaration::NonCarrier {
        reason: PROVEKIT_CONFORMANCE_REASON,
    };

    /// Build a ProveKit rooted at `origin_cid` using `catalog` for premise lookup.
    pub fn new(origin_cid: Cid, catalog: impl Catalog + 'static) -> Self {
        Self {
            origin_cid,
            catalog: Box::new(catalog),
            expect_cycle: false,
        }
    }

    /// Build a ProveKit with an explicit cycle expectation knob for chain-walk tests.
    pub fn with_cycle_expectation(
        origin_cid: Cid,
        catalog: impl Catalog + 'static,
        expect_cycle: bool,
    ) -> Self {
        Self {
            origin_cid,
            catalog: Box::new(catalog),
            expect_cycle,
        }
    }
}

impl Kit for ProveKit {
    fn dialect(&self) -> Dialect {
        Dialect::Other("prove".to_string())
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        match input {
            Input::Claim(claim) => Ok(claim.clone()),
            _ => Err(KitError::UnsupportedInput {
                dialect: self.dialect(),
                message: "ProveKit transform expects Input::Claim".to_string(),
            }),
        }
    }

    fn prove(&self, claim: DomainClaim) -> Result<DomainClaim, KitError> {
        let mut resolved = claim;
        match walk_premises_to_root_with_failure_steps(
            &resolved,
            &self.origin_cid,
            self.catalog.as_ref(),
            self.expect_cycle,
        ) {
            Ok(walked_steps) => {
                resolved.verdict = Verdict::Proved;
                resolved.witness = Some(Witness::ChainIntegrity(ChainIntegrityWitness {
                    walked_chain_root_cid: self.origin_cid.clone(),
                    walked_steps,
                    schema_version: CHAIN_INTEGRITY_SCHEMA_VERSION,
                }));
            }
            Err(failure) => {
                resolved.verdict = Verdict::Refuted;
                resolved.witness = Some(Witness::ChainIntegrityFailure(
                    ChainIntegrityFailureWitness {
                        walked_chain_root_cid: self.origin_cid.clone(),
                        walked_steps_before_break: failure.walked_steps_before_break,
                        break_kind: failure.breakage.kind_name().to_string(),
                        break_detail: failure.breakage.to_string(),
                        schema_version: CHAIN_INTEGRITY_SCHEMA_VERSION,
                    },
                ));
            }
        }
        Ok(resolved)
    }

    fn parse(&self, _input: &Input) -> Result<Term, KitError> {
        Err(KitError::UnsupportedInput {
            dialect: self.dialect(),
            message: "ProveKit parse is not supported".to_string(),
        })
    }

    fn serialize(&self, _term: &Term) -> Result<Input, KitError> {
        Err(KitError::Serialization(
            "ProveKit serialize is not supported".to_string(),
        ))
    }
}

/// Structural reason a premise walk failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainBreak {
    /// The walk encountered a CID it had already visited.
    CycleDetected { cid: Cid },
    /// A premise CID was not present in the catalog.
    PremiseNotInCatalog { cid: Cid },
    /// No premise path reached the configured origin CID.
    OriginUnreachable,
    /// A catalog entry could not be decoded as a DomainClaim.
    DeserializationFailed { cid: Cid, detail: String },
}

impl ChainBreak {
    /// Return the stable variant name serialized into failure witnesses.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::CycleDetected { .. } => "CycleDetected",
            Self::PremiseNotInCatalog { .. } => "PremiseNotInCatalog",
            Self::OriginUnreachable => "OriginUnreachable",
            Self::DeserializationFailed { .. } => "DeserializationFailed",
        }
    }
}

impl fmt::Display for ChainBreak {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected { cid } => {
                write!(formatter, "cycle detected at premise {cid}")
            }
            Self::PremiseNotInCatalog { cid } => {
                write!(formatter, "premise {cid} is not present in the catalog")
            }
            Self::OriginUnreachable => {
                formatter.write_str("origin is unreachable from claim premises")
            }
            Self::DeserializationFailed { cid, detail } => {
                write!(formatter, "premise {cid} could not be decoded: {detail}")
            }
        }
    }
}

impl std::error::Error for ChainBreak {}

struct ChainWalkFailure {
    breakage: ChainBreak,
    walked_steps_before_break: Vec<Cid>,
}

/// Walk `claim.premises` recursively until `origin_cid` is reached.
pub fn walk_premises_to_root(
    claim: &DomainClaim,
    origin_cid: &Cid,
    catalog: &dyn Catalog,
    expect_cycle: bool,
) -> Result<Vec<Cid>, ChainBreak> {
    walk_premises_to_root_with_failure_steps(claim, origin_cid, catalog, expect_cycle)
        .map_err(|failure| failure.breakage)
}

fn walk_premises_to_root_with_failure_steps(
    claim: &DomainClaim,
    origin_cid: &Cid,
    catalog: &dyn Catalog,
    expect_cycle: bool,
) -> Result<Vec<Cid>, ChainWalkFailure> {
    let _ = expect_cycle;
    let mut visited = HashSet::new();
    let mut walked_steps = Vec::new();
    match walk_claim(claim, origin_cid, catalog, &mut visited, &mut walked_steps) {
        Ok(true) => Ok(walked_steps),
        Ok(false) => Err(ChainWalkFailure {
            breakage: ChainBreak::OriginUnreachable,
            walked_steps_before_break: walked_steps,
        }),
        Err(breakage) => Err(ChainWalkFailure {
            breakage,
            walked_steps_before_break: walked_steps,
        }),
    }
}

fn walk_claim(
    claim: &DomainClaim,
    origin_cid: &Cid,
    catalog: &dyn Catalog,
    visited: &mut HashSet<Cid>,
    walked_steps: &mut Vec<Cid>,
) -> Result<bool, ChainBreak> {
    if claim.cid() == *origin_cid {
        return Ok(true);
    }

    let mut reached_origin = false;
    for premise_cid in &claim.premises {
        if !visited.insert(premise_cid.clone()) {
            return Err(ChainBreak::CycleDetected {
                cid: premise_cid.clone(),
            });
        }
        if !catalog.contains(premise_cid) {
            return Err(ChainBreak::PremiseNotInCatalog {
                cid: premise_cid.clone(),
            });
        }
        walked_steps.push(premise_cid.clone());
        let bytes = catalog
            .get(premise_cid)
            .ok_or_else(|| ChainBreak::PremiseNotInCatalog {
                cid: premise_cid.clone(),
            })?;
        let premise_claim = domain_claim_from_canonical_bytes(&bytes).map_err(|detail| {
            ChainBreak::DeserializationFailed {
                cid: premise_cid.clone(),
                detail,
            }
        })?;
        if walk_claim(&premise_claim, origin_cid, catalog, visited, walked_steps)? {
            reached_origin = true;
        }
    }

    Ok(reached_origin)
}
