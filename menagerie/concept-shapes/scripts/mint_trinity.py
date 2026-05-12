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


def lf(name, *args):
    """Build a loss-record IrFormula: an atomic node with snake_case name and variable args.

    Each loss-record value must be an IrFormula per the canonical LossRecord schema
    (BTreeMap<String, IrFormula>) introduced in #634. We use the `atomic` kind:
      {"kind": "atomic", "name": <snake_case_label>, "args": [<IrTerm>, ...]}
    The name encodes the semantic label; args carry the variables that the loss clause
    quantifies over (the witnesses to the divergence).
    """
    return {"args": list(args), "kind": "atomic", "name": name}


def loss(structural_divergence=None, domain_narrowing=None, ub_introduction=None,
         effect_divergence=None, value_divergence=None):
    r = {}
    if structural_divergence is not None:
        r["structural_divergence"] = structural_divergence
    if domain_narrowing is not None:
        r["domain_narrowing"] = domain_narrowing
    if ub_introduction is not None:
        r["ub_introduction"] = ub_introduction
    if effect_divergence is not None:
        r["effect_divergence"] = effect_divergence
    if value_divergence is not None:
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
        "concept:itab-of": amp_memento(
            "concept:itab-of",
            ["value"],
            ["Value"],
            "ITabPtr",
            true_formula(),
            {
                "kind": "operation-contract",
                "operator": "itab-of",
                "arity": ["Value"],
                "result": "ITabPtr",
                "wp_note": "the interface-table pointer for value's runtime type; projects a Value to the ITabPtr slot required by concept:itab-method; identity for JVM objects since the itable is embedded in the class header",
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
            structural_divergence=lf("open_coded_ptr_chain_replaces_name_lookup",
                op_term("concept:deref", [op_term("concept:member", [var("receiver"), const_term("vtbl", "FieldName")])]),
                var("method_name"),
            ),
            domain_narrowing=lf("dispatch_table_fixed_at_compile_or_link_time",
                op_term("runtime_type", [var("receiver")]),
            ),
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
            op_term("concept:itab-method", [op_term("concept:itab-of", [var("receiver")]), var("method_name"), var("receiver"), var("args")]),
            op_term("concept:cons", [var("receiver"), var("args")]),
        ]),
        "java",
        loss(
            structural_divergence=lf("invokevirtual_invokeinterface_near_identity",
                op_term("concept:itab-method", [op_term("concept:itab-of", [var("receiver")]), var("method_name")]),
            ),
            domain_narrowing=lf("method_must_be_virtual_non_final_non_static",
                var("method_name"),
                var("receiver"),
            ),
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
            structural_divergence=lf("mro_dict_scan_structurally_identical_under_mro_axiom",
                op_term("concept:dict-lookup-over-mro", [op_term("concept:type-of", [var("receiver")]), var("method_name")]),
            ),
            domain_narrowing=lf("outbound_narrows_mutable_dispatch_table",
                var("receiver"),
                var("method_name"),
            ),
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
            structural_divergence=lf("open_coded_2d_fn_ptr_table_replaces_double_dispatch",
                op_term("concept:index", [var("dispatch_table_2d"), op_term("c11:type-tag", [var("receiver")])]),
                op_term("concept:index", [var("dispatch_table_2d"), op_term("c11:type-tag", [var("secondary")])]),
                op_term("c11:cast-fn-ptr", [var("dispatch_table_2d"), const_term("void (*)(void*, void*, ...)", "TypeExpr")]),
            ),
            domain_narrowing=lf("tag_spaces_closed_at_table_construction",
                var("receiver"),
                var("secondary"),
            ),
            ub_introduction=lf("out_of_range_type_tag_index_is_ub",
                var("receiver"),
                var("secondary"),
            ),
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
            structural_divergence=lf("visitor_accept_visit_itab_pair_indirection",
                op_term("concept:itab-method", [var("receiver"), const_term("accept", "Name"), var("secondary")]),
                op_term("concept:itab-method", [var("secondary"), op_term("c11:concat-name", [const_term("visit_", "Name"), op_term("concept:type-of", [var("receiver")])])]),
            ),
            domain_narrowing=lf("visitable_and_visitor_sets_fixed_at_interface_declaration",
                var("receiver"),
                var("secondary"),
            ),
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
            structural_divergence=lf("match_type_pair_near_identity_to_contract",
                op_term("python:tuple", [op_term("concept:type-of", [var("receiver")]), op_term("concept:type-of", [var("secondary")])]),
            ),
            domain_narrowing=lf("unmatched_type_pair_raises_type_error",
                var("receiver"),
                var("secondary"),
            ),
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
            structural_divergence=lf("defunctionalization_explicit_env_struct_no_first_class_fn",
                op_term("c11:function-pointer", [var("body_as_fn"), var("env_ptr")]),
                op_term("c11:heap-alloc", [var("captured_env")]),
            ),
            effect_divergence=lf("heap_alloc_for_env_struct_when_outlives_stack",
                op_term("c11:heap-alloc", [var("captured_env")]),
            ),
            ub_introduction=lf("use_after_free_env_ptr_and_c11_aliasing_hazard",
                var("env_ptr"),
                var("captured_env"),
            ),
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
            structural_divergence=lf("invokedynamic_lambda_near_first_class",
                op_term("jvm:invoke-dynamic", [const_term("LambdaMetafactory.metafactory", "MethodRef"), var("body"), var("captured_env")]),
            ),
            domain_narrowing=lf("captured_vars_must_be_effectively_final",
                var("captured_env"),
            ),
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
            structural_divergence=lf("python_lambda_or_def_is_identity_map",
                op_term("python:lambda-or-def", [var("params"), var("body"), var("captured_env")]),
            ),
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
            structural_divergence=lf("setjmp_longjmp_replaces_structured_handler_table",
                op_term("c11:setjmp", [var("jump_buf")]),
                op_term("c11:handler-dispatch", [var("handlers"), op_term("c11:read-jump-payload", [var("jump_buf")])]),
            ),
            effect_divergence=lf("jmp_buf_longjmp_no_destructor_raii",
                var("jump_buf"),
            ),
            ub_introduction=lf("longjmp_over_nontrivial_frames_is_ub",
                var("jump_buf"),
                var("try_body"),
            ),
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
            structural_divergence=lf("jvm_try_catch_near_first_class_realization_is_abstraction",
                op_term("jvm:try-catch", [var("try_body"), var("handlers")]),
            ),
            domain_narrowing=lf("checked_exceptions_require_throws_declaration",
                var("throw_payload"),
                var("handlers"),
            ),
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
            structural_divergence=lf("python_try_except_is_identity_map",
                op_term("python:try-except", [var("try_body"), var("handlers")]),
            ),
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
            structural_divergence=lf("raw_pointer_explicit_deref_no_aliasing_tracking",
                op_term("c11:addr-of", [var("referent_var")]),
            ),
            ub_introduction=lf("use_after_free_and_strict_aliasing_violation",
                var("referent_var"),
            ),
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
            structural_divergence=lf("java_object_ref_is_identity_map_for_object_types",
                op_term("jvm:object-ref", [var("referent_var")]),
            ),
            domain_narrowing=lf("primitive_types_require_boxing",
                var("referent_var"),
            ),
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
            structural_divergence=lf("python_name_binding_is_abstraction_default_semantics",
                op_term("python:name-binding", [var("referent_var")]),
            ),
            domain_narrowing=lf("immutable_types_narrow_to_read_only_aliasing",
                var("referent_var"),
            ),
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
            structural_divergence=lf("hand_rolled_cursor_struct_no_protocol",
                op_term("c11:struct-literal", [const_term("0", "Int"), op_term("c11:array-length", [var("collection")]), op_term("c11:addr-of", [var("collection")])]),
            ),
            ub_introduction=lf("cursor_past_length_deref_is_ub",
                var("state"),
                op_term("c11:array-length", [var("collection")]),
            ),
            domain_narrowing=lf("length_must_be_known_at_iteration_start_no_lazy_collections",
                var("collection"),
            ),
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
            structural_divergence=lf("iterable_iterator_has_next_next_near_first_class",
                op_term("jvm:invoke-interface", [const_term("java.lang.Iterable.iterator", "MethodRef"), var("collection")]),
            ),
            domain_narrowing=lf("collection_must_implement_java_lang_iterable",
                var("collection"),
            ),
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
            structural_divergence=lf("iter_next_protocol_is_abstraction",
                op_term("python:iter-call", [var("collection")]),
            ),
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
            structural_divergence=lf("c_generic_macro_expansion_code_duplication_no_native_instantiation",
                op_term("c11:macro-expansion", [var("parametric_def"), var("type_args")]),
            ),
            domain_narrowing=lf("type_args_must_be_c_types_known_at_preprocessing",
                var("type_args"),
            ),
            ub_introduction=lf("textual_macro_expansion_ill_typed_expansions_ub",
                var("parametric_def"),
                var("type_args"),
            ),
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
            structural_divergence=lf("type_erasure_to_bounds_casts_at_use_sites_no_reified_types",
                op_term("jvm:type-erasure-with-casts", [var("parametric_def"), op_term("jvm:erase-to-bounds", [var("type_args")])]),
            ),
            domain_narrowing=lf("no_generic_arrays_no_instanceof_reified_generics_need_class_token",
                var("type_args"),
                var("parametric_def"),
            ),
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
            structural_divergence=lf("duck_typing_realization_is_absence_of_instantiation",
                op_term("python:duck-typed-call", [var("parametric_def"), var("type_args")]),
            ),
            domain_narrowing=lf("type_bound_violations_caught_at_use_time_not_instantiation",
                var("type_args"),
            ),
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
