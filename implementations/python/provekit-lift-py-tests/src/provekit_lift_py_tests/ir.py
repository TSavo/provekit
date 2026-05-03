# SPDX-License-Identifier: Apache-2.0
#
# Minimal Python IR shape mirroring provekit-ir-symbolic.
#
# Three formula kinds (atomic / connective / quantifier) and three term
# kinds (var / const / ctor). Sort is a primitive name. ContractDecl is
# an emit-time record carrying name, optional pre/post/inv, and an
# outBinding.
#
# Locked IR-JSON shape per protocol/specs/2026-04-30-ir-formal-grammar.md.
# Insertion-order serialization that the canonicalizer's JCS pass re-sorts
# before hashing. We emit canonical Value trees directly (skipping the
# kit's insertion-order JSON string), since downstream hashing is what
# matters.

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Tuple, Union

from .canonicalizer import Value, varr, vint, vobj, vstr, vnull


# Sort ----------------------------------------------------------------------


@dataclass(frozen=True)
class Sort:
    name: str  # "Int" / "Real" / "String" / "Bool"


def Int() -> Sort:
    return Sort("Int")


def Real() -> Sort:
    return Sort("Real")


def String() -> Sort:
    return Sort("String")


def Bool() -> Sort:
    return Sort("Bool")


# Term ----------------------------------------------------------------------


@dataclass(frozen=True)
class _Var:
    name: str


@dataclass(frozen=True)
class _ConstInt:
    value: int
    sort: Sort


@dataclass(frozen=True)
class _ConstStr:
    value: str
    sort: Sort


@dataclass(frozen=True)
class _ConstBool:
    value: bool
    sort: Sort


@dataclass(frozen=True)
class _Ctor:
    name: str
    args: Tuple["Term", ...]


Term = Union[_Var, _ConstInt, _ConstStr, _ConstBool, _Ctor]


def make_var(name: str) -> Term:
    return _Var(name)


def num(n: int) -> Term:
    return _ConstInt(int(n), Int())


def str_const(s: str) -> Term:
    return _ConstStr(s, String())


def bool_const(b: bool) -> Term:
    return _ConstBool(bool(b), Bool())


def ctor(name: str, args: List[Term]) -> Term:
    return _Ctor(name, tuple(args))


# Formula -------------------------------------------------------------------


@dataclass(frozen=True)
class _Atomic:
    name: str
    args: Tuple[Term, ...]


@dataclass(frozen=True)
class _Connective:
    kind: str  # and / or / not / implies
    operands: Tuple["Formula", ...]


@dataclass(frozen=True)
class _Quantifier:
    kind: str  # forall / exists
    name: str
    sort: Sort
    body: "Formula"


Formula = Union[_Atomic, _Connective, _Quantifier]


def atomic(name: str, args: List[Term]) -> Formula:
    return _Atomic(name, tuple(args))


# Atomic predicate names use the Unicode glyphs >=, <=, !=. Cross-language
# hash agreement depends on UTF-8 verbatim emission for U+0080+.
def gt(a: Term, b: Term) -> Formula:
    return atomic(">", [a, b])


def gte(a: Term, b: Term) -> Formula:
    return atomic("≥", [a, b])


def lt(a: Term, b: Term) -> Formula:
    return atomic("<", [a, b])


def lte(a: Term, b: Term) -> Formula:
    return atomic("≤", [a, b])


def eq(a: Term, b: Term) -> Formula:
    return atomic("=", [a, b])


def ne(a: Term, b: Term) -> Formula:
    return atomic("≠", [a, b])


def connective(kind: str, operands: List[Formula]) -> Formula:
    return _Connective(kind, tuple(operands))


def not_(a: Formula) -> Formula:
    return connective("not", [a])


def implies(a: Formula, b: Formula) -> Formula:
    return connective("implies", [a, b])


def and_(operands: List[Formula]) -> Formula:
    return connective("and", operands)


def or_(operands: List[Formula]) -> Formula:
    return connective("or", operands)


def forall(name: str, sort: Sort, body: Formula) -> Formula:
    return _Quantifier("forall", name, sort, body)


def exists(name: str, sort: Sort, body: Formula) -> Formula:
    return _Quantifier("exists", name, sort, body)


# EvidenceTerm --------------------------------------------------------------
#
# Mirrors implementations/rust/provekit-ir-symbolic/src/lib.rs (EvidenceTerm
# / EvidenceCertificate) and the spec at
# protocol/specs/2026-04-30-ir-formal-grammar.md (EvidenceTerm grammar).
#
# Locked key orders (canonicalizer's JCS pass re-sorts to alphabetical
# before hashing; insertion order recorded here mirrors Rust's
# Value::object call order in serialize.rs):
#   evidence:    {kind: "evidence", proofType, certificate}
#   certificate: {tool, version, formulaHash, proofData}
#
# proofType is one of "smt-lib" | "coq" | "custom".


@dataclass(frozen=True)
class EvidenceCertificate:
    tool: str
    version: str
    formula_hash: str
    proof_data: str


@dataclass(frozen=True)
class EvidenceTerm:
    proof_type: str  # "smt-lib" | "coq" | "custom"
    certificate: EvidenceCertificate


def evidence_to_value(e: EvidenceTerm) -> Value:
    return vobj([
        ("kind", vstr("evidence")),
        ("proofType", vstr(e.proof_type)),
        ("certificate", vobj([
            ("tool", vstr(e.certificate.tool)),
            ("version", vstr(e.certificate.version)),
            ("formulaHash", vstr(e.certificate.formula_hash)),
            ("proofData", vstr(e.certificate.proof_data)),
        ])),
    ])


# ContractDecl --------------------------------------------------------------


@dataclass
class ContractDecl:
    name: str
    pre: Optional[Formula] = None
    post: Optional[Formula] = None
    inv: Optional[Formula] = None
    out_binding: str = "out"
    evidence: Optional[EvidenceTerm] = None


# To-Value (canonicalizer Value tree) --------------------------------------


def sort_to_value(s: Sort) -> Value:
    return vobj([("kind", vstr("primitive")), ("name", vstr(s.name))])


def term_to_value(t: Term) -> Value:
    if isinstance(t, _Var):
        return vobj([("kind", vstr("var")), ("name", vstr(t.name))])
    if isinstance(t, _ConstInt):
        return vobj([
            ("kind", vstr("const")),
            ("value", vint(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _ConstStr):
        return vobj([
            ("kind", vstr("const")),
            ("value", vstr(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _ConstBool):
        from .canonicalizer import vbool
        return vobj([
            ("kind", vstr("const")),
            ("value", vbool(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _Ctor):
        return vobj([
            ("kind", vstr("ctor")),
            ("name", vstr(t.name)),
            ("args", varr([term_to_value(a) for a in t.args])),
        ])
    raise TypeError(f"unknown Term: {type(t)!r}")


def formula_to_value(f: Formula) -> Value:
    if isinstance(f, _Atomic):
        return vobj([
            ("kind", vstr("atomic")),
            ("name", vstr(f.name)),
            ("args", varr([term_to_value(a) for a in f.args])),
        ])
    if isinstance(f, _Connective):
        return vobj([
            ("kind", vstr(f.kind)),
            ("operands", varr([formula_to_value(o) for o in f.operands])),
        ])
    if isinstance(f, _Quantifier):
        return vobj([
            ("kind", vstr(f.kind)),
            ("name", vstr(f.name)),
            ("sort", sort_to_value(f.sort)),
            ("body", formula_to_value(f.body)),
        ])
    raise TypeError(f"unknown Formula: {type(f)!r}")


# Variable substitution (used by helper-inlining and parametrize patterns)


def subst_var_in_term(t: Term, formal: str, actual: Term) -> Term:
    if isinstance(t, _Var):
        return actual if t.name == formal else t
    if isinstance(t, _Ctor):
        return _Ctor(t.name, tuple(subst_var_in_term(a, formal, actual) for a in t.args))
    return t  # const variants are inert


def subst_var_in_formula(f: Formula, formal: str, actual: Term) -> Formula:
    if isinstance(f, _Atomic):
        return _Atomic(f.name, tuple(subst_var_in_term(a, formal, actual) for a in f.args))
    if isinstance(f, _Connective):
        return _Connective(f.kind, tuple(subst_var_in_formula(o, formal, actual) for o in f.operands))
    if isinstance(f, _Quantifier):
        # Don't substitute under a shadowing binder.
        if f.name == formal:
            return f
        return _Quantifier(f.kind, f.name, f.sort, subst_var_in_formula(f.body, formal, actual))
    raise TypeError(f"unknown Formula: {type(f)!r}")


# BridgeDecl ----------------------------------------------------------------
#
# Cross-bundle bridge declaration per
# protocol/specs/2026-04-30-ir-formal-grammar.md §BridgeDeclaration. The
# shape mirrors `provekit-ir-types::Declaration::Bridge` (the codegen-derived
# Rust struct) and the TS `BridgeSpec` shape. The `sourceContractCid` +
# `targetProofCid` fields make cross-bundle witness pinning hash-bounded
# (no implicit lookup): the verifier loads the named target proof bundle
# by CID and checks the contract inside it.
#
# Locked key order (per spec line 274-275):
#   kind, name, sourceSymbol, sourceLayer, sourceContractCid,
#   targetContractCid, targetProofCid, targetLayer, [notes?]
#
# `notes` is OMITTED entirely when None (never emitted as null). This is
# the byte-equality rule that keeps the four kits in sync (spec line
# 347-350).


@dataclass(frozen=True)
class BridgeDecl:
    name: str
    source_symbol: str
    source_layer: str
    source_contract_cid: str
    target_contract_cid: str
    target_proof_cid: str
    target_layer: str
    notes: Optional[str] = None


def bridge_decl_to_value(b: BridgeDecl) -> Value:
    pairs: List[Tuple[str, Value]] = [
        ("kind", vstr("bridge")),
        ("name", vstr(b.name)),
        ("sourceSymbol", vstr(b.source_symbol)),
        ("sourceLayer", vstr(b.source_layer)),
        ("sourceContractCid", vstr(b.source_contract_cid)),
        ("targetContractCid", vstr(b.target_contract_cid)),
        ("targetProofCid", vstr(b.target_proof_cid)),
        ("targetLayer", vstr(b.target_layer)),
    ]
    if b.notes is not None:
        pairs.append(("notes", vstr(b.notes)))
    return vobj(pairs)


def contract_decl_to_value(d: ContractDecl) -> Value:
    """Emit a contract declaration as a canonicalizer Value.

    Mirrors the Rust `marshal_declarations` shape, but as a Value tree so
    the JCS pass produces byte-equal output to Rust's value-tree path.
    Locked key order: kind, name, outBinding, [pre?], [post?], [inv?],
    [evidence?].
    """
    pairs: List[Tuple[str, Value]] = [
        ("kind", vstr("contract")),
        ("name", vstr(d.name)),
        ("outBinding", vstr(d.out_binding)),
    ]
    if d.pre is not None:
        pairs.append(("pre", formula_to_value(d.pre)))
    if d.post is not None:
        pairs.append(("post", formula_to_value(d.post)))
    if d.inv is not None:
        pairs.append(("inv", formula_to_value(d.inv)))
    if d.evidence is not None:
        pairs.append(("evidence", evidence_to_value(d.evidence)))
    return vobj(pairs)


def declarations_to_value(
    decls: List[Union[ContractDecl, BridgeDecl]],
) -> Value:
    """Emit a mixed list of contract/bridge declarations as a Value array.

    Matches Rust's `marshal_declarations` (insertion-order JSON in Rust;
    canonicalizer's JCS pass re-sorts keys before hashing).
    """
    items: List[Value] = []
    for d in decls:
        if isinstance(d, ContractDecl):
            items.append(contract_decl_to_value(d))
        elif isinstance(d, BridgeDecl):
            items.append(bridge_decl_to_value(d))
        else:
            raise TypeError(f"unknown declaration: {type(d)!r}")
    return varr(items)


# Locus -----------------------------------------------------------------------
#
# Source position for a call site.
# JSON shape (JCS-canonical key order: column, file, line).
# Mirrors Go's Locus struct (property.go lines 267-293).


@dataclass(frozen=True)
class Locus:
    file: str
    line: int
    column: int


def locus_to_value(loc: Locus) -> Value:
    """Emit Locus as a Value with JCS-canonical key order: column, file, line."""
    return vobj([
        ("column", vint(loc.column)),
        ("file", vstr(loc.file)),
        ("line", vint(loc.line)),
    ])


# CallEdgeDecl ----------------------------------------------------------------
#
# Call-edge memento per protocol/specs/2026-05-03-bridge-linkage-protocol.md §1.
# JSON shape (JCS-canonical key order: callSiteLocus, evidenceTerm, kind,
# schemaVersion, sourceContractCid, targetContractCid, targetSymbol).
# Mirrors Go's CallEdgeDeclaration.MarshalJSON (property.go lines 331-368).
#
# targetContractCid is None for cross-kit calls (encodes as JSON null).
# targetSymbol carries the kit-prefixed name, e.g. "rust-kit:foo".


@dataclass(frozen=True)
class CallEdgeDecl:
    source_contract_cid: str
    target_contract_cid: Optional[str]  # None -> JSON null
    target_symbol: str
    call_site_locus: Locus
    evidence_term: Formula


def call_edge_decl_to_value(c: CallEdgeDecl) -> Value:
    """Emit a call-edge declaration as a canonicalizer Value.

    JCS-canonical key order: callSiteLocus, evidenceTerm, kind, schemaVersion,
    sourceContractCid, targetContractCid, targetSymbol.
    """
    target_cid_value: Value = vnull() if c.target_contract_cid is None else vstr(c.target_contract_cid)
    return vobj([
        ("callSiteLocus", locus_to_value(c.call_site_locus)),
        ("evidenceTerm", formula_to_value(c.evidence_term)),
        ("kind", vstr("call-edge")),
        ("schemaVersion", vstr("1")),
        ("sourceContractCid", vstr(c.source_contract_cid)),
        ("targetContractCid", target_cid_value),
        ("targetSymbol", vstr(c.target_symbol)),
    ])


def call_edges_to_value(edges: List["CallEdgeDecl"]) -> Value:
    """Emit a list of call-edge declarations as a Value array."""
    return varr([call_edge_decl_to_value(e) for e in edges])
