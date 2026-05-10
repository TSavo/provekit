// SPDX-License-Identifier: Apache-2.0

use super::primitives::{compose, verify_sig};
use super::traits::{Catalog, CoreError, DischargeMode, Domain, Kit, Portfolio};
use super::types::{Boundary, DomainClaim, Input, Truth, Verdict};

/// Named verb: transform an input into an unresolved domain claim.
///
/// Composition: `Kit::parse ; Domain::project ; address`. The faithful term is
/// retained in the claim for later realization/cross-compilation; verification
/// can ignore it once the contract projection is present.
pub fn transform(
    kit: &dyn Kit,
    domain: &dyn Domain,
    input: &Input,
    boundary: &Boundary,
) -> Result<DomainClaim, CoreError> {
    let term = kit.parse(input)?;
    let contract = domain.project(&term, boundary)?;
    let from = vec![super::primitives::address(&term)];
    let to = super::primitives::address(&contract);
    Ok(DomainClaim {
        domain: domain.name(),
        term: Some(term),
        contract,
        from,
        to,
        witness: None,
        verdict: Verdict::Unresolved,
        attestation: None,
    })
}

/// Named verb: prove an input by transforming then discharging in search mode.
///
/// Composition: `transform ; Domain::discharge(Search { portfolio })`.
pub fn prove(
    kit: &dyn Kit,
    domain: &dyn Domain,
    input: &Input,
    boundary: &Boundary,
    portfolio: &dyn Portfolio,
) -> Result<DomainClaim, CoreError> {
    let claim = transform(kit, domain, input, boundary)?;
    Ok(domain.discharge(claim, DischargeMode::Search { portfolio })?)
}

/// Named verb: verify a truth by recomputing, resolving, and checking witness.
///
/// Composition: `address-recompute ; resolve/byte-compare ; check-sig ;
/// Domain::discharge(Check)`. This is the bluepaper's constant-size,
/// complete verification theorem expressed as primitive composition.
pub fn verify(truth: &Truth, domain: &dyn Domain, catalog: &dyn Catalog) -> bool {
    let claim = &truth.0;
    let cid = claim.cid();
    let Some(bytes) = catalog.get(&cid) else {
        return false;
    };
    if bytes != claim.canonical_bytes() {
        return false;
    }
    if !verify_sig(claim) {
        return false;
    }
    match domain.discharge(claim.clone(), DischargeMode::Check) {
        Ok(checked) => checked.verdict == Verdict::Proved && checked.witness.is_some(),
        Err(_) => false,
    }
}

/// Named verb: realize a claim into a target dialect.
///
/// If the faithful term is present, this is just `Kit::serialize`. If only the
/// contract remains, the future path is `dropper/synthesize ; serialize`; the
/// initial pass returns `MissingTerm` for that TODO-stubbed branch.
pub fn realize(
    _domain: &dyn Domain,
    target: &dyn Kit,
    claim: &DomainClaim,
) -> Result<Input, CoreError> {
    let term = claim.term.as_ref().ok_or(CoreError::MissingTerm)?;
    Ok(target.serialize(term)?)
}

/// Named verb: cross-compile through a domain morphism.
///
/// Composition: `transform(src) ; compose(morphism) ; target.serialize`. This
/// is the N+M+N-morphisms shape: kits parse and serialize independently while
/// morphisms are themselves claims/paths.
pub fn cross_compile(
    src: &dyn Kit,
    tgt: &dyn Kit,
    domain: &dyn Domain,
    input: &Input,
    morphism: &DomainClaim,
    boundary: &Boundary,
) -> Result<Input, CoreError> {
    let source_claim = transform(src, domain, input, boundary)?;
    let composed = compose(&source_claim, morphism)?;
    let term = composed.term.as_ref().ok_or(CoreError::MissingTerm)?;
    Ok(tgt.serialize(term)?)
}

/// Named verb: link module claims by folding category composition.
///
/// This is `fold(compose)` over already-built module/domain claims.
pub fn link(claims: &[DomainClaim]) -> Result<DomainClaim, CoreError> {
    let Some((first, rest)) = claims.split_first() else {
        return Err(CoreError::EmptyLink);
    };
    let mut acc = first.clone();
    for claim in rest {
        acc = compose(&acc, claim)?;
    }
    Ok(acc)
}
