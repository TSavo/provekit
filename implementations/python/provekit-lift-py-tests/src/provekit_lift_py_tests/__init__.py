# SPDX-License-Identifier: Apache-2.0
#
# provekit-lift-py-tests : Layer 2 structural lift adapter for Python tests.
#
# Layer 2 sits ABOVE Layer 0 (mechanical assert recognition; the future
# Python Layer 0 will be analogous to the Rust one) and BELOW the
# eventual Layer 3 LLM lift. It recognizes four structural patterns over
# pytest/unittest test syntax that Layer 0 cannot, and emits canonical
# IR mementos with content-addressed BLAKE3-512 hashes.
#
# Patterns:
#   1. Bounded ``for`` loop -> forall-implies.
#   2. Helper function inlined at each call site.
#   3. Multi-assertion characterization conjunction.
#   4. ``@pytest.mark.parametrize`` over a literal list -> enumerated and-conjunction.
#
# Out of scope for v0: ``hypothesis`` (Layer 1 already), ``pytest.raises``,
# fixtures, parametrize over factories, nested loops, conditional bodies.

from .canonicalizer import (
    BLAKE3_512_PREFIX,
    blake3_512_of,
    encode_jcs,
    jcs_hash,
)
from .ir import (
    Bool,
    BridgeDecl,
    CallEdgeDecl,
    ContractDecl,
    EvidenceCertificate,
    EvidenceTerm,
    Int,
    Locus,
    Real,
    Sort,
    String,
    and_,
    atomic,
    bool_const,
    bridge_decl_to_value,
    call_edge_decl_to_value,
    call_edges_to_value,
    connective,
    contract_decl_to_value,
    ctor,
    declarations_to_value,
    eq,
    evidence_to_value,
    exists,
    forall,
    formula_to_value,
    gt,
    gte,
    implies,
    locus_to_value,
    lt,
    lte,
    make_var,
    ne,
    not_,
    num,
    or_,
    str_const,
    subst_var_in_formula,
    subst_var_in_term,
    term_to_value,
)
from .decorators import contract, ContractViolation, collect_module
from .layer2 import LiftWarning, Layer2Output, lift_file_layer2
from .proof_envelope import ProofEnvelopeInput, envelope_body_to_value
from .verifier import (
    verify_project,
    prove_contract,
    HandshakeReport,
    VerifierNotFoundError,
)

__all__ = [
    "BLAKE3_512_PREFIX",
    "Bool",
    "BridgeDecl",
    "CallEdgeDecl",
    "ContractDecl",
    "ContractViolation",
    "EvidenceCertificate",
    "EvidenceTerm",
    "HandshakeReport",
    "Int",
    "Layer2Output",
    "LiftWarning",
    "Locus",
    "ProofEnvelopeInput",
    "Real",
    "Sort",
    "String",
    "VerifierNotFoundError",
    "and_",
    "atomic",
    "blake3_512_of",
    "bool_const",
    "bridge_decl_to_value",
    "call_edge_decl_to_value",
    "call_edges_to_value",
    "collect_module",
    "connective",
    "contract",
    "contract_decl_to_value",
    "ctor",
    "declarations_to_value",
    "encode_jcs",
    "envelope_body_to_value",
    "eq",
    "evidence_to_value",
    "exists",
    "forall",
    "formula_to_value",
    "gt",
    "gte",
    "implies",
    "jcs_hash",
    "lift_file_layer2",
    "locus_to_value",
    "lt",
    "lte",
    "make_var",
    "ne",
    "not_",
    "num",
    "or_",
    "prove_contract",
    "str_const",
    "subst_var_in_formula",
    "subst_var_in_term",
    "term_to_value",
    "verify_project",
]
