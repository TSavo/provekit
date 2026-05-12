#!/usr/bin/env python3
"""
mint_trinity.py -- mint the C / Java / Python realizations across the concept-hub abstraction layer.

Authored 2026-05-11. Part of PR feat/trinity-c-java-python-realizations.

This script:
  (A) Writes op-layer hub op spec files to specs/
  (B) Writes ConceptAbstractionMemento catalog entries to catalog/abstractions/
  (C) Writes RealizationDesugaringMemento catalog entries to catalog/realizations/

All CIDs are computed via compute_fixture_cid (BLAKE3-512 over JCS-canonical bytes).
All discharge_receipts are deferred: "deferred:pending-61-PR5"
"""
import copy
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
SPECS_DIR = BASE / "specs"
CATALOG_REAL = BASE / "catalog"
ABST_DIR = CATALOG_REAL / "abstractions"
REAL_DIR = CATALOG_REAL / "realizations"

ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"

# Use the main repo's binary if the worktree hasn't built one
BINARY_CANDIDATES = [
    RUST_DIR / "target" / "debug" / "compute_fixture_cid",
    Path("/Users/tsavo/provekit/implementations/rust/target/debug/compute_fixture_cid"),
]

BINARY = None
for candidate in BINARY_CANDIDATES:
    if candidate.exists():
        BINARY = candidate
        break

if BINARY is None:
    sys.exit("compute_fixture_cid binary not found; run cargo build -p provekit-canonicalizer first")

DEFERRED_RECEIPT = "deferred:pending-61-PR5"
UNSIGNED_SIG = {
    "alg": "ed25519",
    "key_id": "UNSIGNED_DEV_ONLY",
    "sig_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
}


def compute_cid(memento):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(memento, f, ensure_ascii=True)
        f.write("\n")
        tmp = f.name
    try:
        result = subprocess.run([str(BINARY), tmp], capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(f"compute_fixture_cid failed: {result.stderr}")
        return result.stdout.strip()
    finally:
        os.unlink(tmp)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=True)
        f.write("\n")


def catalog_entry(memento):
    cid = compute_cid(memento)
    return {"memento": memento, "cid": cid, "signature": UNSIGNED_SIG}, cid


def amp_memento(fn_name, formals, formal_sorts, return_sort, pre, post, effects):
    return {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": [{"kind": "ctor", "name": s, "args": []} for s in formal_sorts],
        "pre": pre,
        "post": post,
        "effects": effects,
        "auto_minted_mementos": [],
        "return_sort": {"kind": "ctor", "name": return_sort, "args": []},
    }


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def and_formula(operands):
    return {"kind": "connective", "op": "and", "operands": operands}


def atomic(name, args):
    return {"kind": "atomic", "name": name, "args": args}


def var(name):
    return {"kind": "var", "name": name}


def op_term(name, args):
    return {"kind": "op", "name": name, "args": args}


def const_term(value, sort_name):
    return {"kind": "const", "value": value, "sort": {"kind": "primitive", "name": sort_name}}


def realization_memento(fn_name, formals, formal_sorts, pre, lhs, rhs, target_lang, loss_record):
    m = {
        "kind": "equation",
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": [{"kind": "ctor", "name": s, "args": []} for s in formal_sorts],
        "post": {"lhs": lhs, "rhs": rhs},
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": target_lang,
        "loss_record": loss_record,
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }
    if pre is not None:
        m["pre"] = pre
    return m


def loss(structural_divergence=None, domain_narrowing=None, ub_introduction=None,
         effect_divergence=None, value_divergence=None):
    r = {}
    if structural_divergence:
        r["structural_divergence"] = structural_divergence
    if domain_narrowing:
        r["domain_narrowing"] = domain_narrowing
    if ub_introduction:
        r["ub_introduction"] = ub_introduction
    if effect_divergence:
        r["effect_divergence"] = effect_divergence
    if value_divergence:
        r["value_divergence"] = value_divergence
    return r


# ============================================================================
# PART A: Op-layer hub ops
# ============================================================================

def build_op_mementos():
    return {
        "concept:dict-lookup": amp_memento(
            "concept:dict-lookup",
            ["dict", "key"],
            ["Dict", "Name"],
            "Option<Value>",
            atomic("defined", [var("dict"), var("key")]),
            {
                "kind": "operation-contract",
                "operator": "dict-lookup",
                "arity": ["Dict", "Name"],
                "result": "Option<Value>",
                "wp_note": "returns Some(v) where v is the value stored at key in dict; requires the key is present",
                "arity_shape": {"kind": "named", "slots": [{"name": "dict"}, {"name": "key"}]},
            },
            {"effects": [{"kind": "effect-signature", "name": "Read"}]},
        ),
        "concept:vtable-method": amp_memento(
            "concept:vtable-method",
            ["vtable_ptr", "method_index", "receiver", "args"],
            ["VTablePtr", "Int", "Expr", "ListOfExpr"],
            "FnContract",
            and_formula([
                atomic("valid_vtable", [var("vtable_ptr")]),
                atomic("in_range", [var("method_index"), var("vtable_ptr")]),
            ]),
            {
                "kind": "operation-contract",
                "operator": "vtable-method",
                "arity": ["VTablePtr", "Int", "Expr", "ListOf<Expr>"],
                "result": "FnContract",
                "wp_note": "the method at compile-time-fixed offset method_index in the vtable; C realization target for concept:dynamic-dispatch",
                "arity_shape": {"kind": "named", "slots": [
                    {"name": "vtable_ptr"}, {"name": "method_index"},
                    {"name": "receiver"}, {"name": "args", "variadic": True},
                ]},
            },
            {"effects": [{"kind": "effect-signature", "name": "MemRead"}]},
        ),
        "concept:itab-method": amp_memento(
            "concept:itab-method",
            ["itab_ptr", "method_name", "receiver", "args"],
            ["ITabPtr", "Name", "Expr", "ListOfExpr"],
            "FnContract",
            and_formula([
                atomic("valid_itab", [var("itab_ptr")]),
                atomic("has_method", [var("itab_ptr"), var("method_name")]),
            ]),
            {
                "kind": "operation-contract",
                "operator": "itab-method",
                "arity": ["ITabPtr", "Name", "Expr", "ListOf<Expr>"],
                "result": "FnContract",
                "wp_note": "the method at method_name in the JVM itable / Go interface itab; Java realization target for concept:dynamic-dispatch and concept:double-dispatch",
                "arity_shape": {"kind": "named", "slots": [
                    {"name": "itab_ptr"}, {"name": "method_name"},
                    {"name": "receiver"}, {"name": "args", "variadic": True},
                ]},
            },
            {"effects": [{"kind": "effect-signature", "name": "MemRead"}]},
        ),
        "concept:proto-walk": amp_memento(
            "concept:proto-walk",
            ["receiver", "method_name"],
            ["Expr", "Name"],
            "Option<FnContract>",
            true_formula(),
            {
                "kind": "operation-contract",
                "operator": "proto-walk",
                "arity": ["Expr", "Name"],
                "result": "Option<FnContract>",
                "wp_note": "walk the MRO / prototype chain of receiver looking for method_name; Python realization target for concept:dynamic-dispatch",
                "arity_shape": {"kind": "named", "slots": [
                    {"name": "receiver"}, {"name": "method_name"},
                ]},
            },
            {"effects": [{"kind": "effect-signature", "name": "Read"}]},
        ),
        "concept:bound-method": amp_memento(
            "concept:bound-method",
            ["receiver", "method"],
            ["Expr", "FnContract"],
            "FnContract",
            true_formula(),
            {
                "kind": "operation-contract",
                "operator": "bound-method",
                "arity": ["Expr", "FnContract"],
                "result": "FnContract",
                "wp_note": "bind receiver as the first argument of method; calling the result equals calling method with receiver prepended",
                "arity_shape": {"kind": "named", "slots": [
                    {"name": "receiver"}, {"name": "method"},
                ]},
            },
            {"effects": []},
        ),
        "concept:type-of": amp_memento(
            "concept:type-of",
            ["value"],
            ["Expr"],
            "Type",
            true_formula(),
            {
                "kind": "operation-contract",
                "operator": "type-of",
                "arity": ["Expr"],
                "result": "Type",
                "wp_note": "the runtime type of value; reifies runtime_type(v) from the dispatch-theory as an op-layer node",
                "arity_shape": {"kind": "named", "slots": [{"name": "value"}]},
            },
            {"effects": []},
        ),
        "concept:dict-lookup-over-mro": amp_memento(
            "concept:dict-lookup-over-mro",
            ["type_tag", "method_name"],
            ["Type", "Name"],
            "Option<FnContract>",
            true_formula(),
            {
                "kind": "operation-contract",
                "operator": "dict-lookup-over-mro",
                "arity": ["Type", "Name"],
                "result": "Option<FnContract>",
                "wp_note": "scan type_tag.__mro__ in order; return the first match for method_name or None; Python MRO resolution = lookup(runtime_type(receiver), method_name) under the MRO axiom",
                "arity_shape": {"kind": "named", "slots": [
                    {"name": "type_tag"}, {"name": "method_name"},
                ]},
            },
            {"effects": [{"kind": "effect-signature", "name": "Read"}]},
        ),
    }


# ============================================================================
# PART B: ConceptAbstractionMementos
# ============================================================================

def build_abstractions():
    return {
        "concept:dynamic-dispatch": {
            "kind": "concept-abstraction",
            "operator": "concept:dynamic-dispatch",
            "tier": "abstraction",
            "slots": [{"name": "receiver"}, {"name": "method_name"}, {"name": "args", "variadic": True}],
            "formal_sorts": ["Value", "Name", "ListOfValue"],
            "result_sort": "Value",
            "contract": {
                "kind": "wp-rule",
                "formals": ["receiver", "method_name", "args"],
                "body": {
                    "kind": "and",
                    "operands": [
                        atomic("defined", [op_term("lookup", [op_term("runtime_type", [var("receiver")]), var("method_name")])]),
                        {"kind": "apply", "fn": "wp_call", "args": [
                            op_term("lookup", [op_term("runtime_type", [var("receiver")]), var("method_name")]),
                            op_term("cons", [var("receiver"), var("args")]),
                        ]},
                    ],
                },
            },
            "contract_note": "the call result and effect equal those of lookup(runtime_type(receiver), method_name) applied to [receiver, ...args]; undefined if no method resolves",
            "realizations": [],
        },
        "concept:double-dispatch": {
            "kind": "concept-abstraction",
            "operator": "concept:double-dispatch",
            "tier": "abstraction",
            "slots": [{"name": "receiver"}, {"name": "secondary"}, {"name": "method_name"}, {"name": "args", "variadic": True}],
            "formal_sorts": ["Value", "Value", "Name", "ListOfValue"],
            "result_sort": "Value",
            "contract": {
                "kind": "wp-rule",
                "formals": ["receiver", "secondary", "method_name", "args"],
                "body": {
                    "kind": "skolem",
                    "predicate": "double_dispatch",
                    "args": [
                        op_term("runtime_type", [var("receiver")]),
                        op_term("runtime_type", [var("secondary")]),
                        var("method_name"),
                        var("args"),
                    ],
                    "note": "conjunction-of-guarded-clauses: type(receiver)=X and type(secondary)=Y implies result = f_XY(receiver, secondary, args)",
                },
            },
            "contract_note": "for each (X,Y) pair in the dispatch table: if type(receiver)=X and type(secondary)=Y then the result equals f_XY(receiver, secondary, args)",
            "realizations": [],
        },
        "concept:closure": {
            "kind": "concept-abstraction",
            "operator": "concept:closure",
            "tier": "abstraction",
            "slots": [{"name": "captured_env"}, {"name": "params"}, {"name": "body"}],
            "formal_sorts": ["Env", "ListOfName", "Stmt"],
            "result_sort": "FnContract",
            "contract": {
                "kind": "wp-rule",
                "formals": ["captured_env", "params", "body"],
                "body": {
                    "kind": "apply",
                    "fn": "wp_body",
                    "args": [op_term("extend_env", [var("captured_env"), op_term("bind_params", [var("params"), var("call_args")])])],
                },
            },
            "contract_note": "applying the closure to call_args equals evaluating body in an environment that extends captured_env with params bound to call_args",
            "realizations": [],
        },
        "concept:exception": {
            "kind": "concept-abstraction",
            "operator": "concept:exception",
            "tier": "abstraction",
            "slots": [{"name": "try_body"}, {"name": "handlers"}, {"name": "throw_payload"}],
            "formal_sorts": ["Stmt", "ListOfHandler", "Value"],
            "result_sort": "Stmt",
            "contract": {
                "kind": "wp-rule",
                "formals": ["try_body", "handlers", "throw_payload"],
                "body": {
                    "kind": "or",
                    "operands": [
                        {"kind": "apply", "fn": "wp_try_body", "args": []},
                        atomic("handler_matches", [var("throw_payload"), var("handlers")]),
                    ],
                },
            },
            "contract_note": "throw(payload) inside try_body transfers control to the first handler matching type(payload); if no handler matches, propagates to the dynamically enclosing exception frame",
            "realizations": [],
        },
        "concept:reference": {
            "kind": "concept-abstraction",
            "operator": "concept:reference",
            "tier": "abstraction",
            "slots": [{"name": "referent_var"}],
            "formal_sorts": ["LValue"],
            "result_sort": "Ref",
            "contract": {
                "kind": "wp-rule",
                "formals": ["referent_var"],
                "body": atomic("aliases_storage", [var("ref_result"), var("referent_var")]),
            },
            "contract_note": "the reference aliases the storage location of referent_var; a write through the reference is observable via referent_var and vice versa",
            "realizations": [],
        },
        "concept:iterator": {
            "kind": "concept-abstraction",
            "operator": "concept:iterator",
            "tier": "abstraction",
            "slots": [{"name": "collection"}, {"name": "state"}],
            "formal_sorts": ["Collection", "IterState"],
            "result_sort": "Iterator",
            "contract": {
                "kind": "wp-rule",
                "formals": ["collection", "state"],
                "body": and_formula([
                    atomic("yields_each_once", [var("collection"), var("state")]),
                    atomic("preserves_order", [var("collection"), var("state")]),
                ]),
            },
            "contract_note": "the iterator yields each element of collection exactly once via next() calls, in collection order; no element is observed out of order or after exhaustion",
            "realizations": [],
        },
        "concept:generic-instantiation": {
            "kind": "concept-abstraction",
            "operator": "concept:generic-instantiation",
            "tier": "abstraction",
            "slots": [{"name": "parametric_def"}, {"name": "type_args", "variadic": True}],
            "formal_sorts": ["GenericDef", "ListOfType"],
            "result_sort": "FnContract",
            "contract": {
                "kind": "wp-rule",
                "formals": ["parametric_def", "type_args"],
                "body": {
                    "kind": "substitute",
                    "target": {"kind": "apply", "fn": "wp_parametric_def", "args": [var("Q")]},
                    "var": "type_params",
                    "term": var("type_args"),
                },
            },
            "contract_note": "the instantiation has the same operational semantics as parametric_def with type_args substituted for its type parameters, on inputs satisfying the parameters' bounds",
            "realizations": [],
        },
    }


# ============================================================================
# PART C: RealizationDesugaringMementos (21 cells)
# ============================================================================

def build_realizations(op_cids, abst_cids):
    # Helpers for common lhs terms
    def dd_lhs():
        return op_term("concept:dynamic-dispatch", [var("receiver"), var("method_name"), var("args")])

    def dd2_lhs():
        return op_term("concept:double-dispatch", [var("receiver"), var("secondary"), var("method_name"), var("args")])

    def closure_lhs():
        return op_term("concept:closure", [var("captured_env"), var("params"), var("body")])

    def exc_lhs():
        return op_term("concept:exception", [var("try_body"), var("handlers"), var("throw_payload")])

    def ref_lhs():
        return op_term("concept:reference", [var("referent_var")])

    def iter_lhs():
        return op_term("concept:iterator", [var("collection"), var("state")])

    def gen_lhs():
        return op_term("concept:generic-instantiation", [var("parametric_def"), var("type_args")])

    cells = {}

    # --- dynamic-dispatch, c ---
    cells["concept:dynamic-dispatch->c11:vtable-indirection"] = realization_memento(
        "concept:dynamic-dispatch->c11:vtable-indirection",
        ["receiver", "method_name", "args"],
        ["Value", "Name", "ListOfValue"],
        atomic("dispatch_table_fixed_at_compile_time", [op_term("runtime_type", [var("receiver")])]),
        dd_lhs(),
        op_term("concept:call", [
            op_term("concept:member", [
                op_term("concept:deref", [op_term("concept:member", [var("receiver"), const_term("vtbl", "FieldName")])]),
                var("method_name"),
            ]),
            op_term("concept:cons", [var("receiver"), var("args")]),
        ]),
        "c11",
        loss(
            structural_divergence="open-coded pointer chain (deref vtbl field, index by compile-time-fixed offset, fn-ptr call); not a dispatch primitive; the abstraction lookup is by name, the realization offset is fixed at link time",
            domain_narrowing="dispatch table for runtime_type(receiver) must be fixed at compile or link time; runtime table mutation is not supportable in this realization",
        ),
    )

    # --- dynamic-dispatch, java ---
    cells["concept:dynamic-dispatch->jvm:virtual-method"] = realization_memento(
        "concept:dynamic-dispatch->jvm:virtual-method",
        ["receiver", "method_name", "args"],
        ["Value", "Name", "ListOfValue"],
        atomic("virtually_dispatched", [var("method_name"), var("receiver")]),
        dd_lhs(),
        op_term("concept:call", [
            op_term("concept:itab-method", [op_term("concept:type-of", [var("receiver")]), var("method_name"), var("receiver"), var("args")]),
            op_term("concept:cons", [var("receiver"), var("args")]),
        ]),
        "java",
        loss(
            structural_divergence="near-first-class: invokevirtual / invokeinterface; barely a desugaring; the JVM dispatch IS the lookup",
            domain_narrowing="method_name must resolve to a non-final, non-static, non-private method on the receiver class hierarchy; final/sealed/static/private methods are statically bound and require a different realization",
        ),
    )

    # --- dynamic-dispatch, python ---
    cells["concept:dynamic-dispatch->python:mro-dict-lookup"] = realization_memento(
        "concept:dynamic-dispatch->python:mro-dict-lookup",
        ["receiver", "method_name", "args"],
        ["Value", "Name", "ListOfValue"],
        None,
        dd_lhs(),
        op_term("concept:call", [
            op_term("concept:bound-method", [
                var("receiver"),
                op_term("concept:dict-lookup-over-mro", [
                    op_term("concept:type-of", [var("receiver")]),
                    var("method_name"),
                ]),
            ]),
            var("args"),
        ]),
        "python",
        loss(
            structural_divergence="name-lookup protocol over a linked list of dicts (__mro__ scan); not a primitive; structurally identical to the abstraction lookup under the MRO-axiom",
            domain_narrowing="outbound only: Python over-delivers runtime mutability of the dispatch table (setattr/delattr/__class__-reassignment); transporting OUT of Python to a vtable-family language narrows to states where the dispatched-upon type is not mutated after construction",
        ),
    )

    # --- double-dispatch, c (Sir's worked example, verbatim) ---
    cells["concept:double-dispatch->c11:2d-fn-ptr-table"] = realization_memento(
        "concept:double-dispatch->c11:2d-fn-ptr-table",
        ["receiver", "secondary", "method_name", "args"],
        ["Value", "Value", "Name", "ListOfValue"],
        atomic("dispatch_table_2d_constructed", [var("dispatch_table_2d")]),
        dd2_lhs(),
        op_term("concept:call", [
            op_term("c11:cast-fn-ptr", [
                op_term("concept:index", [
                    op_term("concept:index", [var("dispatch_table_2d"), op_term("c11:type-tag", [var("receiver")])]),
                    op_term("c11:type-tag", [var("secondary")]),
                ]),
                const_term("void (*)(void*, void*, ...)", "TypeExpr"),
            ]),
            op_term("concept:cons", [var("receiver"), op_term("concept:cons", [var("secondary"), var("args")])]),
        ]),
        "c11",
        loss(
            structural_divergence="2D array of void function pointers; table[type_tag(receiver)][type_tag(secondary)] cast to (void (*)(receiver_type, secondary_type, args*)); two indirections plus cast; no source-language equivalent to the double-index-and-cast",
            domain_narrowing="both tag spaces (receiver types and secondary types) are closed at table construction; any type added after construction is out of range",
            ub_introduction="out-of-range type_tag index into either dimension is undefined behavior in C; no bounds check in the generated dispatch",
        ),
    )

    # --- double-dispatch, java (Sir's worked example, verbatim) ---
    cells["concept:double-dispatch->jvm:visitor-itab-pair"] = realization_memento(
        "concept:double-dispatch->jvm:visitor-itab-pair",
        ["receiver", "secondary", "method_name", "args"],
        ["Value", "Value", "Name", "ListOfValue"],
        and_formula([
            atomic("implements_visitable", [var("receiver")]),
            atomic("implements_visitor", [var("secondary")]),
        ]),
        dd2_lhs(),
        op_term("concept:call", [
            op_term("concept:itab-method", [
                var("secondary"),
                op_term("c11:concat-name", [
                    const_term("visit_", "Name"),
                    op_term("concept:type-of", [var("receiver")]),
                ]),
                var("secondary"),
                var("args"),
            ]),
            op_term("concept:cons", [var("receiver"), op_term("concept:cons", [var("secondary"), var("args")])]),
        ]),
        "java",
        loss(
            structural_divergence="fresh Visitable interface + fresh Visitor interface with one visit_T method per concrete receiver type; two invokeinterface (itab) dispatches in sequence (accept then visit_T); the visitor pattern is the Java idiom for double dispatch",
            domain_narrowing="the visitable and visitor type sets are fixed at interface declaration; open extensibility of either type set requires re-opening the interface",
        ),
    )

    # --- double-dispatch, python (Sir's worked example, verbatim) ---
    cells["concept:double-dispatch->python:match-type-pair"] = realization_memento(
        "concept:double-dispatch->python:match-type-pair",
        ["receiver", "secondary", "method_name", "args"],
        ["Value", "Value", "Name", "ListOfValue"],
        None,
        dd2_lhs(),
        op_term("python:match", [
            op_term("python:tuple", [
                op_term("concept:type-of", [var("receiver")]),
                op_term("concept:type-of", [var("secondary")]),
            ]),
            const_term("guarded_clauses_from_contract", "MatchArms"),
        ]),
        "python",
        loss(
            structural_divergence="match (type(receiver), type(secondary)) with one case arm per (X,Y) pair from the contract guarded clauses; method_name string becomes case arm selection; near-identity to the contract",
            domain_narrowing="fallthrough to TypeError for unmatched (X,Y) pair; narrows the open Python dispatch domain to the explicitly enumerated pairs in the match",
        ),
    )

    # --- closure, c ---
    cells["concept:closure->c11:defunctionalized-env-struct"] = realization_memento(
        "concept:closure->c11:defunctionalized-env-struct",
        ["captured_env", "params", "body"],
        ["Env", "ListOfName", "Stmt"],
        None,
        closure_lhs(),
        op_term("c11:struct-literal", [
            op_term("c11:addr-of", [
                op_term("c11:function-pointer", [var("body_as_fn"), op_term("concept:cons", [var("env_ptr"), var("params")])])
            ]),
            op_term("c11:heap-alloc", [var("captured_env")]),
        ]),
        "c11",
        loss(
            structural_divergence="defunctionalization: body becomes a named function with an explicit env-struct pointer as extra first argument; captured_env becomes a malloc'd struct; calling the closure = calling the function with the env pointer prepended; no first-class function type in C",
            effect_divergence="heap allocation for the environment struct when the closure outlives the stack frame; absent when proven stack-lifetime (the stack-env sub-realization)",
            ub_introduction="use-after-free if the env-struct pointer outlives the heap allocation; aliasing through the env pointer introduces C11 aliasing hazards if captured vars are also accessible by name in the caller scope",
        ),
    )

    # --- closure, java ---
    cells["concept:closure->jvm:lambda-invokedynamic"] = realization_memento(
        "concept:closure->jvm:lambda-invokedynamic",
        ["captured_env", "params", "body"],
        ["Env", "ListOfName", "Stmt"],
        None,
        closure_lhs(),
        op_term("jvm:invoke-dynamic", [
            const_term("LambdaMetafactory.metafactory", "MethodRef"),
            var("body"),
            var("captured_env"),
        ]),
        "java",
        loss(
            structural_divergence="modern Java: lambda desugars to invokedynamic + LambdaMetafactory; effectively first-class; structural_divergence near zero for the modern case; the pre-Java-8 anonymous-inner-class realization has heavy structural_divergence (a one-method object standing in for a function)",
            domain_narrowing="captured variables must be effectively final; closures that mutate their captured environment require a workaround (single-element array trick or AtomicReference)",
        ),
    )

    # --- closure, python ---
    cells["concept:closure->python:native-closure"] = realization_memento(
        "concept:closure->python:native-closure",
        ["captured_env", "params", "body"],
        ["Env", "ListOfName", "Stmt"],
        None,
        closure_lhs(),
        op_term("python:lambda-or-def", [var("params"), var("body"), var("captured_env")]),
        "python",
        loss(
            structural_divergence="near-zero: a Python lambda or nested def IS the concept:closure; the realization is the identity map for captured_env (Python cell objects handle capture automatically)",
        ),
    )

    # --- exception, c ---
    cells["concept:exception->c11:setjmp-longjmp"] = realization_memento(
        "concept:exception->c11:setjmp-longjmp",
        ["try_body", "handlers", "throw_payload"],
        ["Stmt", "ListOfHandler", "Value"],
        None,
        exc_lhs(),
        op_term("c11:conditional", [
            op_term("c11:setjmp", [var("jump_buf")]),
            op_term("c11:seq", [var("try_body"), const_term("normal_exit", "Stmt")]),
            op_term("c11:seq", [
                op_term("c11:read-jump-payload", [var("jump_buf")]),
                op_term("c11:handler-dispatch", [var("handlers"), op_term("c11:read-jump-payload", [var("jump_buf")])]),
            ]),
        ]),
        "c11",
        loss(
            structural_divergence="setjmp(buf) at entry; longjmp(buf, payload) at throw site; conditional on setjmp return value; no structured handler table; handler dispatch is a hand-written switch over a type tag in the jump buffer",
            effect_divergence="a jump buffer (jmp_buf on the stack or heap); longjmp unwinds without calling destructors; no RAII/defer equivalent",
            ub_introduction="longjmp over stack frames with non-trivial objects is undefined behavior in C11; longjmp from signal handlers is restricted; cannot cross C++ frames with active destructors",
        ),
    )

    # --- exception, java ---
    cells["concept:exception->jvm:try-catch"] = realization_memento(
        "concept:exception->jvm:try-catch",
        ["try_body", "handlers", "throw_payload"],
        ["Stmt", "ListOfHandler", "Value"],
        None,
        exc_lhs(),
        op_term("jvm:try-catch", [var("try_body"), var("handlers")]),
        "java",
        loss(
            structural_divergence="near-first-class: jvm:try-catch / throw; the realization IS the abstraction",
            domain_narrowing="checked exceptions (throws declarations on methods): a source program that throws an exception not declared in the method signature requires either declaring it or wrapping in RuntimeException",
        ),
    )

    # --- exception, python ---
    cells["concept:exception->python:try-except"] = realization_memento(
        "concept:exception->python:try-except",
        ["try_body", "handlers", "throw_payload"],
        ["Stmt", "ListOfHandler", "Value"],
        None,
        exc_lhs(),
        op_term("python:try-except", [var("try_body"), var("handlers")]),
        "python",
        loss(
            structural_divergence="near-zero: python:try/except IS the concept; the realization is the identity map",
        ),
    )

    # --- reference, c ---
    cells["concept:reference->c11:raw-pointer"] = realization_memento(
        "concept:reference->c11:raw-pointer",
        ["referent_var"],
        ["LValue"],
        None,
        ref_lhs(),
        op_term("c11:addr-of", [var("referent_var")]),
        "c11",
        loss(
            structural_divergence="c11:addr-of produces a raw pointer; accessing the referent requires explicit deref; no aliasing tracking; the abstraction aliasing guarantee is present but unchecked",
            ub_introduction="use-after-free: if referent_var goes out of scope while the pointer lives, any deref is undefined behavior; aliasing-with-mutation through multiple raw pointers violates C11 strict-aliasing rules",
        ),
    )

    # --- reference, java ---
    cells["concept:reference->jvm:object-reference"] = realization_memento(
        "concept:reference->jvm:object-reference",
        ["referent_var"],
        ["LValue"],
        None,
        ref_lhs(),
        op_term("jvm:object-ref", [var("referent_var")]),
        "java",
        loss(
            structural_divergence="near-zero: Java object variables are references by default; the realization IS the identity map for object types",
            domain_narrowing="Java primitive types (int, long, double, etc.) are value-typed; concept:reference over a primitive requires boxing (Integer, Long, Double) which adds allocation and unboxing cost",
        ),
    )

    # --- reference, python ---
    cells["concept:reference->python:name-binding"] = realization_memento(
        "concept:reference->python:name-binding",
        ["referent_var"],
        ["LValue"],
        None,
        ref_lhs(),
        op_term("python:name-binding", [var("referent_var")]),
        "python",
        loss(
            structural_divergence="near-zero: Python names are references to objects; assigning one name to another makes both reference the same object; the abstraction IS Python default semantics",
            domain_narrowing="immutable objects (int, str, tuple, frozenset) cannot be mutated through the reference; concept:reference over an immutable type narrows to read-only aliasing",
        ),
    )

    # --- iterator, c ---
    cells["concept:iterator->c11:hand-rolled-cursor"] = realization_memento(
        "concept:iterator->c11:hand-rolled-cursor",
        ["collection", "state"],
        ["Collection", "IterState"],
        None,
        iter_lhs(),
        op_term("c11:struct-literal", [
            const_term("0", "Int"),
            op_term("c11:array-length", [var("collection")]),
            op_term("c11:addr-of", [var("collection")]),
        ]),
        "c11",
        loss(
            structural_divergence="hand-rolled struct { int index; int length; T* data }; next() is index++; exhausted() is index >= length; no protocol; each collection type needs its own cursor type",
            ub_introduction="advancing the cursor past the length and dereferencing is undefined behavior; no bounds check unless explicitly written",
            domain_narrowing="infinite or lazy collections cannot be represented; the length must be known at iteration start; linked lists and tree structures require different cursor implementations",
        ),
    )

    # --- iterator, java ---
    cells["concept:iterator->jvm:iterable-iterator"] = realization_memento(
        "concept:iterator->jvm:iterable-iterator",
        ["collection", "state"],
        ["Collection", "IterState"],
        atomic("implements_iterable", [var("collection")]),
        iter_lhs(),
        op_term("jvm:invoke-interface", [
            const_term("java.lang.Iterable.iterator", "MethodRef"),
            var("collection"),
        ]),
        "java",
        loss(
            structural_divergence="near-first-class: java.lang.Iterable/Iterator with hasNext()/next(); the for-each loop desugars to this; structural_divergence low",
            domain_narrowing="the collection type must implement java.lang.Iterable; primitive arrays use a special index-loop realization; the Iterator protocol is stateful and not re-entrant",
        ),
    )

    # --- iterator, python ---
    cells["concept:iterator->python:iter-next-protocol"] = realization_memento(
        "concept:iterator->python:iter-next-protocol",
        ["collection", "state"],
        ["Collection", "IterState"],
        None,
        iter_lhs(),
        op_term("python:iter-call", [var("collection")]),
        "python",
        loss(
            structural_divergence="near-zero: iter(collection) returns an iterator object implementing __next__(); StopIteration signals exhaustion; the for loop desugars to this; the abstraction IS Python iterator protocol",
        ),
    )

    # --- generic-instantiation, c ---
    cells["concept:generic-instantiation->c11:macro-expansion"] = realization_memento(
        "concept:generic-instantiation->c11:macro-expansion",
        ["parametric_def", "type_args"],
        ["GenericDef", "ListOfType"],
        None,
        gen_lhs(),
        op_term("c11:macro-expansion", [var("parametric_def"), var("type_args")]),
        "c11",
        loss(
            structural_divergence="C has no generic types; the realization is either _Generic macro expansion or X-macro code duplication; neither is a native instantiation; each instantiation is a distinct copy of the code",
            domain_narrowing="type arguments must be expressible as C types known at preprocessing time; type constraints cannot be expressed or checked",
            ub_introduction="macro expansion is textual; ill-typed expansions produce code that compiles but has undefined behavior at runtime if type assumptions are violated",
        ),
    )

    # --- generic-instantiation, java ---
    cells["concept:generic-instantiation->jvm:type-erasure"] = realization_memento(
        "concept:generic-instantiation->jvm:type-erasure",
        ["parametric_def", "type_args"],
        ["GenericDef", "ListOfType"],
        None,
        gen_lhs(),
        op_term("jvm:type-erasure-with-casts", [
            var("parametric_def"),
            op_term("jvm:erase-to-bounds", [var("type_args")]),
        ]),
        "java",
        loss(
            structural_divergence="type parameters are erased to their bounds (Object by default) at bytecode level; casts inserted at use sites by the compiler; no runtime type information for type parameters",
            domain_narrowing="cannot create arrays of a generic type; cannot use instanceof with a type parameter; reified generics require a Class<T> token workaround; two instantiations of the same generic produce identical bytecode",
        ),
    )

    # --- generic-instantiation, python ---
    cells["concept:generic-instantiation->python:duck-typing"] = realization_memento(
        "concept:generic-instantiation->python:duck-typing",
        ["parametric_def", "type_args"],
        ["GenericDef", "ListOfType"],
        None,
        gen_lhs(),
        op_term("python:duck-typed-call", [var("parametric_def"), var("type_args")]),
        "python",
        loss(
            structural_divergence="the realization IS the absence of an instantiation: the parametric_def works on any type_args via duck typing; no instantiation site is generated; the type_args annotation (if any, via type hints) is erased at runtime",
            domain_narrowing="type checking occurs at runtime, not at the instantiation site; violations of the type parameter bounds are caught as AttributeError or TypeError at use time, not at the instantiation call",
        ),
    )

    assert len(cells) == 21, f"Expected 21 realizations, got {len(cells)}"
    return cells


# ============================================================================
# Main
# ============================================================================

def main():
    print("=== mint_trinity.py ===")

    # PART A: Op specs
    print("\n[A] Minting op-layer hub ops...")
    op_mementos = build_op_mementos()
    op_cids = {}
    for name, memento in op_mementos.items():
        entry, cid = catalog_entry(memento)
        op_cids[name] = cid

        # Write spec JSON
        slug = name.replace("concept:", "")
        spec_path = SPECS_DIR / f"op_{slug.replace('-', '_')}.spec.json"
        spec = {
            "kind": "algorithm",
            "fn_name": name,
            "formals": memento["formals"],
            "formal_sorts": memento["formal_sorts"],
            "return_sort": memento["return_sort"],
            "pre": memento["pre"],
            "post": memento["post"],
            "effects": memento["effects"],
            "locus": "protocol/specs/2026-05-15-concept-hub-abstraction-layer.md#s1.4",
        }
        write_json(spec_path, spec)

        # Write catalog entry
        fn_name_safe = name.replace(":", "_colon_")
        catalog_path = CATALOG_REAL / "algorithms" / f"{name}.{cid}.json"
        write_json(catalog_path, entry)
        print(f"  {name}: {cid[:40]}...")

    # Verify stability
    print("\n  Verifying stability...")
    for name, memento in op_mementos.items():
        cid2 = compute_cid(memento)
        assert cid2 == op_cids[name], f"UNSTABLE: {name}"
    print("  All stable.")

    # PART B: ConceptAbstractionMementos (empty realizations first)
    print("\n[B] Minting ConceptAbstractionMementos...")
    abstractions = build_abstractions()

    # PART C: RealizationDesugaringMementos
    print("\n[C] Minting RealizationDesugaringMementos (21 cells)...")
    realizations = build_realizations(op_cids, {})
    real_cids = {}
    for fn_name, memento in realizations.items():
        entry, cid = catalog_entry(memento)
        real_cids[fn_name] = cid
        catalog_path = REAL_DIR / f"{fn_name}.{cid}.json"
        write_json(catalog_path, entry)
        print(f"  {fn_name}: {cid[:40]}...")

    # Verify stability of realizations
    print("\n  Verifying realization stability...")
    for fn_name, memento in realizations.items():
        cid2 = compute_cid(memento)
        assert cid2 == real_cids[fn_name], f"UNSTABLE: {fn_name}"
    print("  All stable.")

    # Now populate realizations field in abstractions and write
    print("\n[B cont.] Writing abstractions with populated realization_cids...")
    abst_real_map = {
        "concept:dynamic-dispatch": [
            "concept:dynamic-dispatch->c11:vtable-indirection",
            "concept:dynamic-dispatch->jvm:virtual-method",
            "concept:dynamic-dispatch->python:mro-dict-lookup",
        ],
        "concept:double-dispatch": [
            "concept:double-dispatch->c11:2d-fn-ptr-table",
            "concept:double-dispatch->jvm:visitor-itab-pair",
            "concept:double-dispatch->python:match-type-pair",
        ],
        "concept:closure": [
            "concept:closure->c11:defunctionalized-env-struct",
            "concept:closure->jvm:lambda-invokedynamic",
            "concept:closure->python:native-closure",
        ],
        "concept:exception": [
            "concept:exception->c11:setjmp-longjmp",
            "concept:exception->jvm:try-catch",
            "concept:exception->python:try-except",
        ],
        "concept:reference": [
            "concept:reference->c11:raw-pointer",
            "concept:reference->jvm:object-reference",
            "concept:reference->python:name-binding",
        ],
        "concept:iterator": [
            "concept:iterator->c11:hand-rolled-cursor",
            "concept:iterator->jvm:iterable-iterator",
            "concept:iterator->python:iter-next-protocol",
        ],
        "concept:generic-instantiation": [
            "concept:generic-instantiation->c11:macro-expansion",
            "concept:generic-instantiation->jvm:type-erasure",
            "concept:generic-instantiation->python:duck-typing",
        ],
    }

    abst_cids = {}
    for abst_name, real_keys in abst_real_map.items():
        memento = copy.deepcopy(abstractions[abst_name])
        memento["realizations"] = [real_cids[k] for k in real_keys]
        entry, cid = catalog_entry(memento)
        abst_cids[abst_name] = cid
        catalog_path = ABST_DIR / f"{abst_name}.{cid}.json"
        write_json(catalog_path, entry)
        print(f"  {abst_name}: {cid[:40]}...")

    # Verify stability of abstractions
    print("\n  Verifying abstraction stability...")
    for abst_name, real_keys in abst_real_map.items():
        memento = copy.deepcopy(abstractions[abst_name])
        memento["realizations"] = [real_cids[k] for k in real_keys]
        cid2 = compute_cid(memento)
        assert cid2 == abst_cids[abst_name], f"UNSTABLE: {abst_name}"
    print("  All stable.")

    print(f"\n=== Done ===")
    print(f"  Op-layer ops:     {len(op_cids)}")
    print(f"  Abstractions:     {len(abst_cids)}")
    print(f"  Realizations:     {len(real_cids)}")
    print(f"  Total new files:  {len(op_cids) * 2 + len(abst_cids) + len(real_cids)} (7 specs + 7 algo catalog + 7 abst catalog + 21 real catalog)")


if __name__ == "__main__":
    main()
