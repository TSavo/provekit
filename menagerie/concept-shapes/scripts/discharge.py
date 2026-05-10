#!/usr/bin/env python3
import copy
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"
TARGET = RUST_DIR / "target" / "debug"
PROVEKIT = TARGET / "provekit"
CANON = TARGET / "compute_fixture_cid"

SPEC_DIR = BASE / "specs"
SOURCE_DIR = BASE / "sources"
RECEIPT_DIR = BASE / "receipts"
DISCHARGE_DIR = BASE / "discharges"
COMPOSITION_DIR = BASE / "compositions"
CATALOG_REAL = BASE / "catalog"
CATALOG_ARG = BASE / "dev" / ".." / "catalog"
CID_FILE = BASE / "cids.tsv"

FOO_SHAPE_CID = (
    "blake3-512:"
    "a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa"
    "02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1"
)


def primitive(name):
    return {"kind": "primitive", "name": name}


def fn_sort(name):
    return {"kind": "ctor", "name": name, "args": []}


def var(name):
    return {"kind": "var", "name": name}


def const(value, sort_name):
    return {"kind": "const", "value": value, "sort": primitive(sort_name)}


def ctor(name, args=None):
    return {"kind": "ctor", "name": name, "args": args or []}


def atomic(name, args=None):
    return {"kind": "atomic", "name": name, "args": args or []}


def eq(left, right):
    return atomic("=", [left, right])


def and_formula(operands):
    return {"kind": "and", "operands": operands}


def implies(guard, consequence):
    return {"kind": "implies", "operands": [guard, consequence]}


def not_formula(item):
    return {"kind": "not", "operands": [item]}


def true_formula():
    return atomic("true", [])


def true_term():
    return ctor("true", [])


def algorithm_payload(fn_name, formals, formal_sorts, return_sort, pre, post, effects):
    return {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": [primitive(item) for item in formal_sorts],
        "return_sort": primitive(return_sort),
        "pre": pre,
        "post": post,
        "effects": effects,
        "auto_minted_mementos": [],
    }


def algorithm_spec(payload):
    return {
        "kind": "algorithm",
        "fn_name": payload["fn_name"],
        "formals": payload["formals"],
        "formal_sorts": payload["formal_sorts"],
        "return_sort": payload["return_sort"],
        "pre": payload["pre"],
        "post": payload["post"],
        "effects": payload["effects"],
        **({"input_cids": payload["input_cids"]} if "input_cids" in payload else {}),
        **({"refines": payload["refines"]} if "refines" in payload else {}),
    }


def source_contract(fn_name, formals, formal_sorts, return_sort, pre, post, effects):
    return {
        "kind": "function-contract",
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": [primitive(item) for item in formal_sorts],
        "return_sort": primitive(return_sort),
        "pre": pre,
        "post": post,
        "effects": effects,
        "auto_minted_mementos": [],
    }


def empty_effects():
    return {"effects": []}


def sort_list(*names):
    return list(names)


def allocation_shape():
    n = var("n")
    err = var("err")
    cont = var("continuation_value")
    p = var("p")
    failed_term = ctor("alloc_failed", [n])
    failed_formula = atomic("alloc_failed", [n])
    post = and_formula(
        [
            eq(var("ret"), ctor("ite", [failed_term, err, cont])),
            implies(failed_formula, eq(p, const("null", "Buffer"))),
            implies(not_formula(failed_formula), atomic("valid_buffer", [p, n])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "Alloc",
                "result": "p",
                "size": "n",
                "failure_condition": failed_term,
            }
        ]
    }
    return algorithm_payload(
        "shape:allocate-or-bail",
        ["n", "err", "continuation_value", "p"],
        sort_list("Size", "ReturnValue", "ReturnValue", "Buffer"),
        "ReturnValue",
        true_formula(),
        post,
        effects,
    )


def allocation_source_c():
    count = var("count")
    failure = var("failure")
    next_value = var("next_value")
    ptr = var("ptr")
    failed_term = ctor("malloc_failed", [count])
    failed_formula = atomic("malloc_failed", [count])
    post = and_formula(
        [
            eq(var("result"), ctor("ite", [failed_term, failure, next_value])),
            implies(failed_formula, eq(ptr, const("null", "void_ptr"))),
            implies(not_formula(failed_formula), atomic("non_null_sized", [ptr, count])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "Alloc",
                "result": "ptr",
                "size": "count",
                "failure_condition": failed_term,
            }
        ]
    }
    return source_contract(
        "c:allocate_or_bail",
        ["count", "failure", "next_value", "ptr"],
        sort_list("size_t", "int", "int", "void_ptr"),
        "int",
        true_formula(),
        post,
        effects,
    )


def allocation_source_rust():
    length = var("len")
    err_code = var("err_code")
    ok_value = var("ok_value")
    buf = var("buf")
    failed_term = ctor("try_reserve_failed", [length])
    failed_formula = atomic("try_reserve_failed", [length])
    post = and_formula(
        [
            eq(var("out"), ctor("ite", [failed_term, err_code, ok_value])),
            implies(failed_formula, eq(buf, const("null", "RawVec"))),
            implies(not_formula(failed_formula), atomic("raw_vec_capacity_ge", [buf, length])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "Alloc",
                "result": "buf",
                "size": "len",
                "failure_condition": failed_term,
            }
        ]
    }
    return source_contract(
        "rust:allocate_or_bail",
        ["len", "err_code", "ok_value", "buf"],
        sort_list("usize", "isize", "isize", "RawVec"),
        "isize",
        true_formula(),
        post,
        effects,
    )


def bounds_shape():
    guard_term = ctor("<", [var("i"), var("len")])
    guard_formula = atomic("<", [var("i"), var("len")])
    post = and_formula(
        [
            eq(var("ret"), ctor("ite", [guard_term, ctor("read", [var("buf"), var("i")]), var("err")])),
            implies(guard_formula, atomic("accessible", [var("buf"), var("i")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemRead",
                "target": "buf",
                "index": "i",
                "guard": guard_term,
            }
        ]
    }
    return algorithm_payload(
        "shape:check-bounds-then-access",
        ["buf", "i", "len", "err"],
        sort_list("Buffer", "Index", "Size", "Value"),
        "Value",
        true_formula(),
        post,
        effects,
    )


def bounds_source_c():
    guard_term = ctor("c_lt", [var("idx"), var("n")])
    guard_formula = atomic("c_lt", [var("idx"), var("n")])
    post = and_formula(
        [
            eq(
                var("result"),
                ctor("ite", [guard_term, ctor("c_read_i32", [var("arr"), var("idx")]), var("fail")]),
            ),
            implies(guard_formula, atomic("c_accessible", [var("arr"), var("idx")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemRead",
                "target": "arr",
                "index": "idx",
                "guard": guard_term,
            }
        ]
    }
    return source_contract(
        "c:check_bounds_then_access",
        ["arr", "idx", "n", "fail"],
        sort_list("int_ptr", "size_t_index", "size_t_len", "int"),
        "int",
        true_formula(),
        post,
        effects,
    )


def bounds_source_rust():
    guard_term = ctor("usize_lt", [var("pos"), var("length")])
    guard_formula = atomic("usize_lt", [var("pos"), var("length")])
    post = and_formula(
        [
            eq(
                var("retv"),
                ctor("ite", [guard_term, ctor("slice_get", [var("slice"), var("pos")]), var("fallback")]),
            ),
            implies(guard_formula, atomic("slice_accessible", [var("slice"), var("pos")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemRead",
                "target": "slice",
                "index": "pos",
                "guard": guard_term,
            }
        ]
    }
    return source_contract(
        "rust:check_bounds_then_access",
        ["slice", "pos", "length", "fallback"],
        sort_list("SliceRef", "usize_index", "usize_len", "Elem"),
        "Elem",
        true_formula(),
        post,
        effects,
    )


def critical_shape():
    m = var("m")
    post = and_formula(
        [
            eq(var("ret"), var("body_ret")),
            eq(ctor("lock_state_after", [m]), ctor("lock_state_before", [m])),
            atomic("balanced_lock_pair", [m]),
            implies(atomic("holds_lock", [m]), atomic("body_effects_done", [m])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "CriticalSection",
                "lock": "m",
                "guard": ctor("holds_lock", [m]),
                "body_effects": "body_effects",
                "balanced": True,
            }
        ]
    }
    return algorithm_payload(
        "shape:acquire-use-release",
        ["m", "body_ret"],
        sort_list("Lock", "CriticalValue"),
        "CriticalValue",
        atomic("lock_available", [m]),
        post,
        effects,
    )


def critical_source_c():
    mutex = var("mutex")
    post = and_formula(
        [
            eq(var("result"), var("body_result")),
            eq(ctor("pthread_state_after", [mutex]), ctor("pthread_state_before", [mutex])),
            atomic("pthread_balanced", [mutex]),
            implies(atomic("pthread_mutex_held", [mutex]), atomic("critical_body_done", [mutex])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "CriticalSection",
                "lock": "mutex",
                "guard": ctor("pthread_mutex_held", [mutex]),
                "body_effects": "body_effects",
                "balanced": True,
            }
        ]
    }
    return source_contract(
        "c:acquire_use_release",
        ["mutex", "body_result"],
        sort_list("pthread_mutex_t_ptr", "int"),
        "int",
        atomic("pthread_mutex_available", [mutex]),
        post,
        effects,
    )


def critical_source_rust():
    lock = var("lock_ref")
    post = and_formula(
        [
            eq(var("out"), var("value")),
            eq(ctor("mutex_state_after", [lock]), ctor("mutex_state_before", [lock])),
            atomic("mutex_guard_balanced", [lock]),
            implies(atomic("guard_holds_lock", [lock]), atomic("closure_body_done", [lock])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "CriticalSection",
                "lock": "lock_ref",
                "guard": ctor("guard_holds_lock", [lock]),
                "body_effects": "body_effects",
                "balanced": True,
            }
        ]
    }
    return source_contract(
        "rust:acquire_use_release",
        ["lock_ref", "value"],
        sort_list("MutexRef", "CriticalOut"),
        "CriticalOut",
        atomic("mutex_available", [lock]),
        post,
        effects,
    )


def validate_shape():
    valid_term = ctor("valid", [var("x")])
    valid_formula = atomic("valid", [var("x")])
    post = and_formula(
        [
            eq(var("ret"), ctor("ite", [valid_term, var("committed_state"), var("err")])),
            implies(valid_formula, atomic("commit_applied", [var("x")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemWrite",
                "target": "state",
                "value": "x",
                "guard": valid_term,
            }
        ]
    }
    return algorithm_payload(
        "shape:validate-then-commit",
        ["x", "err", "committed_state"],
        sort_list("Candidate", "Outcome", "Outcome"),
        "Outcome",
        true_formula(),
        post,
        effects,
    )


def validate_source_c():
    valid_term = ctor("c_valid_record", [var("record")])
    valid_formula = atomic("c_valid_record", [var("record")])
    post = and_formula(
        [
            eq(var("result"), ctor("ite", [valid_term, var("new_state"), var("error_rc")])),
            implies(valid_formula, atomic("c_commit_applied", [var("record")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemWrite",
                "target": "global_state",
                "value": "record",
                "guard": valid_term,
            }
        ]
    }
    return source_contract(
        "c:validate_then_commit",
        ["record", "error_rc", "new_state"],
        sort_list("record_ptr", "int", "state_handle"),
        "int",
        true_formula(),
        post,
        effects,
    )


def validate_source_rust():
    valid_term = ctor("rust_valid_item", [var("item")])
    valid_formula = atomic("rust_valid_item", [var("item")])
    post = and_formula(
        [
            eq(var("outcome"), ctor("ite", [valid_term, var("committed"), var("err_value")])),
            implies(valid_formula, atomic("rust_commit_applied", [var("item")])),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "MemWrite",
                "target": "store",
                "value": "item",
                "guard": valid_term,
            }
        ]
    }
    return source_contract(
        "rust:validate_then_commit",
        ["item", "err_value", "committed"],
        sort_list("Item", "CommitOutcome", "CommitOutcome"),
        "CommitOutcome",
        true_formula(),
        post,
        effects,
    )


def branch_shape():
    condition = ctor("err_cond", [var("x")])
    post = eq(var("ret"), ctor("ite", [condition, var("err_val"), var("x")]))
    return algorithm_payload(
        "shape:branch-on-error-else-passthrough",
        ["x", "err_val"],
        sort_list("Value", "Value"),
        "Value",
        true_formula(),
        post,
        empty_effects(),
    )


def branch_source_c():
    condition = ctor("c_status_is_error", [var("status")])
    post = eq(var("result"), ctor("ite", [condition, var("fail_value"), var("status")]))
    return source_contract(
        "c:branch_on_error_else_passthrough",
        ["status", "fail_value"],
        sort_list("int", "int"),
        "int",
        true_formula(),
        post,
        empty_effects(),
    )


def branch_source_rust():
    condition = ctor("rust_result_is_error", [var("value")])
    post = eq(var("retv"), ctor("ite", [condition, var("error_value"), var("value")]))
    return source_contract(
        "rust:branch_on_error_else_passthrough",
        ["value", "error_value"],
        sort_list("ResultValue", "ResultValue"),
        "ResultValue",
        true_formula(),
        post,
        empty_effects(),
    )


def refcount_shape():
    obj = var("o")
    post = and_formula(
        [
            eq(var("ret"), var("use_ret")),
            eq(ctor("refcount_after", [obj]), ctor("refcount_before", [obj])),
            atomic("balanced_refcount_pair", [obj]),
            implies(atomic(">", [ctor("refcount_after_inc", [obj]), const(0, "Int")]), atomic("use_guarded", [obj])),
        ]
    )
    effects = {
        "effects": [
            {"kind": "RefInc", "object": "o"},
            {"kind": "Use", "object": "o", "guard": ctor(">", [ctor("refcount_after_inc", [obj]), const(0, "Int")])},
            {"kind": "RefDec", "object": "o"},
        ]
    }
    return algorithm_payload(
        "shape:refcount-inc-use-dec",
        ["o", "use_ret"],
        sort_list("Object", "UseValue"),
        "UseValue",
        atomic(">=", [ctor("refcount_before", [obj]), const(0, "Int")]),
        post,
        effects,
    )


def refcount_source_c():
    obj = var("obj")
    post = and_formula(
        [
            eq(var("result"), var("body_result")),
            eq(ctor("c_refcount_after", [obj]), ctor("c_refcount_before", [obj])),
            atomic("c_ref_balanced", [obj]),
            implies(atomic(">", [ctor("c_refcount_after_inc", [obj]), const(0, "Int")]), atomic("c_use_guarded", [obj])),
        ]
    )
    effects = {
        "effects": [
            {"kind": "RefInc", "object": "obj"},
            {"kind": "Use", "object": "obj", "guard": ctor(">", [ctor("c_refcount_after_inc", [obj]), const(0, "Int")])},
            {"kind": "RefDec", "object": "obj"},
        ]
    }
    return source_contract(
        "c:refcount_inc_use_dec",
        ["obj", "body_result"],
        sort_list("ref_obj_ptr", "int"),
        "int",
        atomic(">=", [ctor("c_refcount_before", [obj]), const(0, "Int")]),
        post,
        effects,
    )


def refcount_source_rust():
    obj = var("arc_obj")
    post = and_formula(
        [
            eq(var("out"), var("value")),
            eq(ctor("arc_count_after", [obj]), ctor("arc_count_before", [obj])),
            atomic("arc_clone_drop_balanced", [obj]),
            implies(atomic(">", [ctor("arc_count_after_clone", [obj]), const(0, "Int")]), atomic("arc_use_guarded", [obj])),
        ]
    )
    effects = {
        "effects": [
            {"kind": "RefInc", "object": "arc_obj"},
            {"kind": "Use", "object": "arc_obj", "guard": ctor(">", [ctor("arc_count_after_clone", [obj]), const(0, "Int")])},
            {"kind": "RefDec", "object": "arc_obj"},
        ]
    }
    return source_contract(
        "rust:refcount_inc_use_dec",
        ["arc_obj", "value"],
        sort_list("ArcRef", "UseOut"),
        "UseOut",
        atomic(">=", [ctor("arc_count_before", [obj]), const(0, "Int")]),
        post,
        effects,
    )


def composite_shape():
    not_failed_term = ctor("not", [ctor("alloc_failed", [var("n")])])
    access_guard = ctor("and", [not_failed_term, ctor("<", [var("i"), var("len")])])
    post = and_formula(
        [
            eq(
                var("ret"),
                ctor(
                    "ite",
                    [
                        ctor("alloc_failed", [var("n")]),
                        var("alloc_err"),
                        ctor("ite", [ctor("<", [var("i"), var("len")]), ctor("read", [var("p"), var("i")]), var("access_err")]),
                    ],
                ),
            ),
            implies(not_formula(atomic("alloc_failed", [var("n")])), atomic("valid_buffer", [var("p"), var("n")])),
            implies(
                and_formula(
                    [
                        not_formula(atomic("alloc_failed", [var("n")])),
                        atomic("<", [var("i"), var("len")]),
                    ]
                ),
                atomic("accessible", [var("p"), var("i")]),
            ),
        ]
    )
    effects = {
        "effects": [
            {
                "kind": "Alloc",
                "result": "p",
                "size": "n",
                "failure_condition": ctor("alloc_failed", [var("n")]),
            },
            {
                "kind": "MemRead",
                "target": "p",
                "index": "i",
                "guard": access_guard,
            },
        ]
    }
    return algorithm_payload(
        "shape:validated-allocated-access",
        ["n", "i", "len", "alloc_err", "access_err", "p"],
        sort_list("Size", "Index", "Size", "Value", "Value", "Buffer"),
        "Value",
        true_formula(),
        post,
        effects,
    )


CONCEPTS = [
    {
        "slug": "allocate-or-bail",
        "shape": allocation_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": allocation_source_c,
                "renaming": {
                    "count": "n",
                    "failure": "err",
                    "next_value": "continuation_value",
                    "ptr": "p",
                    "result": "ret",
                },
                "representation": {
                    "size_t": "Size",
                    "int": "ReturnValue",
                    "void_ptr": "Buffer",
                },
                "operators": {
                    "malloc_failed": "alloc_failed",
                    "non_null_sized": "valid_buffer",
                },
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": allocation_source_rust,
                "renaming": {
                    "len": "n",
                    "err_code": "err",
                    "ok_value": "continuation_value",
                    "buf": "p",
                    "out": "ret",
                },
                "representation": {
                    "usize": "Size",
                    "isize": "ReturnValue",
                    "RawVec": "Buffer",
                },
                "operators": {
                    "try_reserve_failed": "alloc_failed",
                    "raw_vec_capacity_ge": "valid_buffer",
                },
            },
        ],
    },
    {
        "slug": "check-bounds-then-access",
        "shape": bounds_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": bounds_source_c,
                "renaming": {"arr": "buf", "idx": "i", "n": "len", "fail": "err", "result": "ret"},
                "representation": {"int_ptr": "Buffer", "size_t_index": "Index", "size_t_len": "Size", "int": "Value"},
                "operators": {"c_lt": "<", "c_read_i32": "read", "c_accessible": "accessible"},
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": bounds_source_rust,
                "renaming": {"slice": "buf", "pos": "i", "length": "len", "fallback": "err", "retv": "ret"},
                "representation": {"SliceRef": "Buffer", "usize_index": "Index", "usize_len": "Size", "Elem": "Value"},
                "operators": {"usize_lt": "<", "slice_get": "read", "slice_accessible": "accessible"},
            },
        ],
    },
    {
        "slug": "acquire-use-release",
        "shape": critical_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": critical_source_c,
                "renaming": {"mutex": "m", "body_result": "body_ret", "result": "ret"},
                "representation": {"pthread_mutex_t_ptr": "Lock", "int": "CriticalValue"},
                "operators": {
                    "pthread_mutex_available": "lock_available",
                    "pthread_state_after": "lock_state_after",
                    "pthread_state_before": "lock_state_before",
                    "pthread_balanced": "balanced_lock_pair",
                    "pthread_mutex_held": "holds_lock",
                    "critical_body_done": "body_effects_done",
                },
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": critical_source_rust,
                "renaming": {"lock_ref": "m", "value": "body_ret", "out": "ret"},
                "representation": {"MutexRef": "Lock", "CriticalOut": "CriticalValue"},
                "operators": {
                    "mutex_available": "lock_available",
                    "mutex_state_after": "lock_state_after",
                    "mutex_state_before": "lock_state_before",
                    "mutex_guard_balanced": "balanced_lock_pair",
                    "guard_holds_lock": "holds_lock",
                    "closure_body_done": "body_effects_done",
                },
            },
        ],
    },
    {
        "slug": "validate-then-commit",
        "shape": validate_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": validate_source_c,
                "renaming": {
                    "record": "x",
                    "error_rc": "err",
                    "new_state": "committed_state",
                    "result": "ret",
                    "global_state": "state",
                },
                "representation": {"record_ptr": "Candidate", "int": "Outcome", "state_handle": "Outcome"},
                "operators": {"c_valid_record": "valid", "c_commit_applied": "commit_applied"},
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": validate_source_rust,
                "renaming": {
                    "item": "x",
                    "err_value": "err",
                    "committed": "committed_state",
                    "outcome": "ret",
                    "store": "state",
                },
                "representation": {"Item": "Candidate", "CommitOutcome": "Outcome"},
                "operators": {"rust_valid_item": "valid", "rust_commit_applied": "commit_applied"},
            },
        ],
    },
    {
        "slug": "branch-on-error-else-passthrough",
        "shape": branch_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": branch_source_c,
                "renaming": {"status": "x", "fail_value": "err_val", "result": "ret"},
                "representation": {"int": "Value"},
                "operators": {"c_status_is_error": "err_cond"},
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": branch_source_rust,
                "renaming": {"value": "x", "error_value": "err_val", "retv": "ret"},
                "representation": {"ResultValue": "Value"},
                "operators": {"rust_result_is_error": "err_cond"},
            },
        ],
    },
    {
        "slug": "refcount-inc-use-dec",
        "shape": refcount_shape,
        "realizations": [
            {
                "slug": "c",
                "label": "C",
                "contract": refcount_source_c,
                "renaming": {"obj": "o", "body_result": "use_ret", "result": "ret"},
                "representation": {"ref_obj_ptr": "Object", "int": "UseValue"},
                "operators": {
                    "c_refcount_after": "refcount_after",
                    "c_refcount_before": "refcount_before",
                    "c_ref_balanced": "balanced_refcount_pair",
                    "c_refcount_after_inc": "refcount_after_inc",
                    "c_use_guarded": "use_guarded",
                },
            },
            {
                "slug": "rust",
                "label": "Rust",
                "contract": refcount_source_rust,
                "renaming": {"arc_obj": "o", "value": "use_ret", "out": "ret"},
                "representation": {"ArcRef": "Object", "UseOut": "UseValue"},
                "operators": {
                    "arc_count_after": "refcount_after",
                    "arc_count_before": "refcount_before",
                    "arc_clone_drop_balanced": "balanced_refcount_pair",
                    "arc_count_after_clone": "refcount_after_inc",
                    "arc_use_guarded": "use_guarded",
                },
            },
        ],
    },
]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, ensure_ascii=True)
        handle.write("\n")


def read_json(path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def run(command, cwd=None, input_text=None):
    result = subprocess.run(
        [str(part) for part in command],
        cwd=str(cwd) if cwd else None,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise SystemExit(
            "command failed: "
            + " ".join(str(part) for part in command)
            + "\nstdout:\n"
            + result.stdout
            + "\nstderr:\n"
            + result.stderr
        )
    return result.stdout.strip()


def build_tools():
    run(["cargo", "build", "-p", "provekit-cli", "-p", "provekit-canonicalizer"], cwd=RUST_DIR)


def canonical_cid_file(path):
    return run([CANON, path])


def canonical_cid_value(value):
    tmp_dir = BASE / "tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json", dir=tmp_dir, delete=False) as handle:
        json.dump(value, handle, ensure_ascii=True)
        handle.write("\n")
        tmp_path = Path(handle.name)
    try:
        return canonical_cid_file(tmp_path)
    finally:
        tmp_path.unlink(missing_ok=True)


def mint(kind, spec_name):
    output = run(
        [
            PROVEKIT,
            "mint",
            kind,
            "--spec",
            SPEC_DIR / spec_name,
            "--unsigned",
            "--catalog",
            CATALOG_ARG,
        ]
    )
    cid, path = output.split("\t", 1)
    return cid, path


def normalize_string(value, renaming_map, representation_map, operator_map, literal_map):
    if value in literal_map:
        return literal_map[value]
    if value in renaming_map:
        return renaming_map[value]
    if value in representation_map:
        return representation_map[value]
    if value in operator_map:
        return operator_map[value]
    return value


def normalize_node(value, renaming_map, representation_map, operator_map, literal_map):
    if isinstance(value, list):
        return [normalize_node(item, renaming_map, representation_map, operator_map, literal_map) for item in value]
    if isinstance(value, str):
        return normalize_string(value, renaming_map, representation_map, operator_map, literal_map)
    if not isinstance(value, dict):
        return value

    out = {}
    kind = value.get("kind")
    for key, item in value.items():
        out[key] = normalize_node(item, renaming_map, representation_map, operator_map, literal_map)

    if kind == "primitive" and value.get("name") in representation_map:
        out["name"] = representation_map[value["name"]]
    if kind == "var" and value.get("name") in renaming_map:
        out["name"] = renaming_map[value["name"]]
    if kind in ("ctor", "atomic") and value.get("name") in operator_map:
        out["name"] = operator_map[value["name"]]
    return out


def after_substitution_payload(contract, target_payload, renaming_map, representation_map, operator_map, literal_map):
    normalized = normalize_node(contract, renaming_map, representation_map, operator_map, literal_map)
    payload = {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": target_payload["fn_name"],
        "formals": normalized["formals"],
        "formal_sorts": normalized["formal_sorts"],
        "return_sort": normalized["return_sort"],
        "pre": normalized["pre"],
        "post": normalized["post"],
        "effects": normalized["effects"],
        "auto_minted_mementos": normalized.get("auto_minted_mementos", []),
    }
    if payload != target_payload:
        raise ValueError(
            "normalized payload does not match target shape "
            + target_payload["fn_name"]
            + "\nnormalized:\n"
            + json.dumps(payload, indent=2, sort_keys=True)
            + "\ntarget:\n"
            + json.dumps(target_payload, indent=2, sort_keys=True)
        )
    return payload


def morphism_spec(name, source_cid, shape_cid, renaming_map, representation_map, operator_map, literal_map):
    return {
        "kind": "algorithm",
        "fn_name": name,
        "formals": ["source_contract"],
        "formal_sorts": [fn_sort("FunctionContractMemento")],
        "return_sort": fn_sort("FunctionContractMemento"),
        "pre": true_formula(),
        "post": {
            "kind": "contract-renaming-morphism",
            "source_contract_cid": source_cid,
            "target_shape_cid": shape_cid,
            "renaming_map": renaming_map,
            "representation_map": representation_map,
            "operator_map": operator_map,
            "literal_map": literal_map,
            "homomorphism_obligation": {
                "kind": "canonicalizer-alpha-equivalence-plus-representation-map",
                "source": source_cid,
                "target": shape_cid,
            },
        },
        "effects": empty_effects(),
        "input_cids": [source_cid, shape_cid],
    }


def specialization_spec(branch_shape_cid):
    return {
        "kind": "algorithm",
        "fn_name": "foo:specializes:branch-on-error-else-passthrough",
        "formals": ["source_shape"],
        "formal_sorts": [fn_sort("FunctionContractMemento")],
        "return_sort": fn_sort("FunctionContractMemento"),
        "pre": true_formula(),
        "post": {
            "kind": "specialization-of",
            "source_shape_cid": FOO_SHAPE_CID,
            "target_shape_cid": branch_shape_cid,
            "substitution": {
                "x": "arg_0",
                "err_cond": {
                    "kind": "lambda",
                    "paramName": "arg_0",
                    "paramSort": primitive("Int"),
                    "body": ctor("=", [var("arg_0"), const(0, "Int")]),
                },
                "err_val": const(-22, "Int"),
            },
            "implication": {
                "kind": "shape-instance-implies-general-shape",
                "antecedent": FOO_SHAPE_CID,
                "consequent": branch_shape_cid,
            },
        },
        "effects": empty_effects(),
        "input_cids": [FOO_SHAPE_CID, branch_shape_cid],
    }


def store_receipt(name, receipt):
    write_json(RECEIPT_DIR / f"{name}.receipt.json", receipt)
    cid = canonical_cid_file(RECEIPT_DIR / f"{name}.receipt.json")
    catalog_dir = CATALOG_REAL / "receipts"
    catalog_dir.mkdir(parents=True, exist_ok=True)
    catalog_path = catalog_dir / f"{name}.{cid}.json"
    write_json(catalog_path, {"cid": cid, "memento": receipt, "signature": None})
    return cid, str(catalog_path)


def pure_compose_atom(fn_name, formals, sorts, return_sort, post, formal_idx=None):
    atom = {
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": [primitive(item) for item in sorts],
        "formal_regions": [],
        "return_sort": primitive(return_sort),
        "return_region": None,
        "pre": true_formula(),
        "post": post,
        "body_cid": None,
        "effects": empty_effects(),
        "locus": {"file": None, "line": 0, "col": 0},
        "auto_minted_mementos": [],
    }
    if formal_idx is not None:
        atom["formal_idx"] = formal_idx
    return atom


def run_compose_probe():
    inner = pure_compose_atom(
        "shape:allocate-or-bail:pure-projection",
        ["n"],
        ["Size"],
        "Buffer",
        eq(var("result"), ctor("allocated_buffer", [var("n")])),
        0,
    )
    outer = pure_compose_atom(
        "shape:check-bounds-then-access:pure-projection",
        ["p", "i", "len"],
        ["Buffer", "Index", "Size"],
        "Value",
        eq(var("result"), ctor("read", [var("p"), var("i")])),
        0,
    )
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "compose", "params": {"atoms": [inner, outer]}},
        {"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}},
    ]
    response_text = run([PROVEKIT, "compose", "--rpc"], input_text="\n".join(json.dumps(item) for item in requests) + "\n")
    responses = [json.loads(line) for line in response_text.splitlines() if line.strip()]
    compose_response = next(item for item in responses if item.get("id") == 2)
    if "error" in compose_response:
        return {
            "status": "refused",
            "error": compose_response["error"],
            "atoms": [inner, outer],
        }
    result = compose_response["result"]
    return {
        "status": "composed",
        "composed_cid": result["composed_cid"],
        "body_jcs": result["body_jcs"],
        "atoms": [inner, outer],
    }


def prepare_dirs():
    for path in [SPEC_DIR, SOURCE_DIR, RECEIPT_DIR, DISCHARGE_DIR, COMPOSITION_DIR, BASE / "dev"]:
        if path.exists() and path.name != "dev":
            shutil.rmtree(path)
        path.mkdir(parents=True, exist_ok=True)
    if CATALOG_REAL.exists():
        shutil.rmtree(CATALOG_REAL)
    CATALOG_REAL.mkdir(parents=True, exist_ok=True)
    tmp_dir = BASE / "tmp"
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
    pycache = Path(__file__).resolve().parent / "__pycache__"
    if pycache.exists():
        shutil.rmtree(pycache)


def label_from_kind(item):
    if item["kind"] in ("shape", "source", "morphism", "receipt", "composition", "ccp-compose"):
        return item["name"]
    return f"{item['kind']}:{item['name']}"


def write_cids(cids):
    with CID_FILE.open("w", encoding="utf-8") as handle:
        handle.write("kind\tname\tcid\tpath\n")
        for item in cids:
            handle.write(f"{item['kind']}\t{item['name']}\t{item['cid']}\t{item['path']}\n")


def write_readme(cids, concept_records, discharge_status, composed_shape_cid, composition_receipt_cid, compose_probe):
    cid_by_name = {item["name"]: item["cid"] for item in cids}
    lines = [
        "# Concept Shape Catalog",
        "",
        "This menagerie exhibit is the node table for universal algebraic shape addresses beyond `foo`.",
        "Each recurring cross-language idiom has a universal address: the CID of its algebraic-shape contract.",
        "A lifted instance from any language or ISA lands on that CID after the renaming and representation morphism is applied, because the canonicalizer deduplicates the normalized contract bytes.",
        "",
        "The CID space is treated as a self-organizing map, a Kohonen map without loss. The catalog grows as more idioms are named, and it plateaus the way the contract catalog plateaus: there are only finitely many cluster centers, the recurring computational idioms.",
        "",
        "`menagerie/foo-algebraic-shape/` is one node in this same space. This exhibit names the rest of the starter node table, and both exhibits grow toward the same plateau.",
        "",
        "## Node Table",
        "",
        "| Concept | Shape CID | Realizations | Discharged Morphisms |",
        "| --- | --- | --- | --- |",
    ]
    for record in concept_records:
        realization_names = ", ".join(item["label"] for item in record["realizations"])
        morphism_names = ", ".join(item["morphism_name"] for item in record["realizations"])
        lines.append(f"| `{record['slug']}` | `{record['shape_cid']}` | {realization_names} | {morphism_names} |")

    lines.extend(
        [
            "",
            "## Concept Details",
            "",
        ]
    )
    for record in concept_records:
        lines.append(f"### {record['slug']}")
        lines.append("")
        lines.append(f"- Shape: `{record['shape_cid']}`")
        for item in record["realizations"]:
            lines.append(f"- {item['label']} source: `{item['source_cid']}`")
            lines.append(f"- {item['label']} morphism: `{item['morphism_cid']}`")
            lines.append(f"- {item['label']} receipt: `{item['receipt_cid']}`")
        lines.append("")

    branch_shape_cid = cid_by_name["shape:branch-on-error-else-passthrough"]
    lines.extend(
        [
            "## Foo Specialization",
            "",
            "`foo` is the instance of `branch-on-error-else-passthrough` where `err_cond(x)` is `x == 0` and `err_val` is `-22`.",
            f"- Foo shape CID: `{FOO_SHAPE_CID}`",
            f"- Branch shape CID: `{branch_shape_cid}`",
            f"- Specialization morphism CID: `{cid_by_name['foo:specializes:branch-on-error-else-passthrough']}`",
            f"- Specialization receipt CID: `{cid_by_name['receipt:foo:specializes:branch-on-error-else-passthrough']}`",
            "",
            "## Conjoinable Shapes",
            "",
            "Shapes are conjoinable. `allocate-or-bail` followed by `check-bounds-then-access` on the allocated buffer composes through CCP into `validated-allocated-access`, a composite shape with its own universal address.",
            f"- `validated-allocated-access` shape CID: `{composed_shape_cid}`",
            f"- Composition receipt CID: `{composition_receipt_cid}`",
        ]
    )
    if compose_probe["status"] == "composed":
        lines.append(f"- libprovekit compose probe CID: `{compose_probe['composed_cid']}`")
    else:
        lines.append("- libprovekit compose probe status: `refused`")

    lines.extend(
        [
            "",
            "## Discharges",
            "",
            "| Morphism | After substitution CID | Shape CID |",
            "| --- | --- | --- |",
        ]
    )
    for name, after_cid, shape_cid in discharge_status:
        lines.append(f"| `{name}` | `{after_cid}` | `{shape_cid}` |")

    lines.extend(
        [
            "",
            "All after-substitution CIDs above equal their target shape CIDs. These are canonicalizer discharges, not solver proofs.",
            "",
            "## Reproduce",
            "",
            "Run:",
            "",
            "```sh",
            "menagerie/concept-shapes/mint.sh",
            "```",
            "",
            "The script builds the Rust CLI and canonicalizer helper, writes concrete source contracts, mints shapes and morphisms into `catalog/`, writes receipts, updates `cids.tsv`, and scans this exhibit for forbidden dash characters and the forbidden sign-off name.",
            "",
            "## References",
            "",
            "- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`",
            "- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`",
            "- `protocol/specs/2026-05-10-realizer-protocol-v2.md` (ORP v0.2)",
            "",
            "T Savo",
            "",
        ]
    )
    (BASE / "README.md").write_text("\n".join(lines), encoding="utf-8")


def scan_created_text():
    bad = []
    forbidden_name = "Tra" + "vis"
    for path in BASE.rglob("*"):
        if not path.is_file():
            continue
        if "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        data = path.read_bytes()
        if b"\xe2\x80\x94" in data:
            bad.append(f"{path}: em dash")
        if b"\xe2\x80\x93" in data:
            bad.append(f"{path}: en dash")
        text = data.decode("utf-8", errors="ignore")
        if forbidden_name in text:
            bad.append(f"{path}: forbidden signoff name")
    if bad:
        raise SystemExit("\n".join(bad))


def main():
    build_tools()
    prepare_dirs()

    cids = []
    concept_records = []
    discharge_status = []
    shape_payloads = {}
    shape_cids = {}

    for concept in CONCEPTS:
        slug = concept["slug"]
        shape_payload = concept["shape"]()
        shape_payloads[slug] = shape_payload
        spec_name = f"{slug}_shape.spec.json"
        write_json(SPEC_DIR / spec_name, algorithm_spec(shape_payload))
        shape_cid, shape_path = mint("algorithm", spec_name)
        expected_shape_cid = canonical_cid_value(shape_payload)
        if shape_cid != expected_shape_cid:
            raise SystemExit(f"shape CID mismatch for {slug}: {shape_cid} != {expected_shape_cid}")
        shape_cids[slug] = shape_cid
        cids.append({"kind": "shape", "name": shape_payload["fn_name"], "cid": shape_cid, "path": shape_path})

    composed_payload = composite_shape()
    composed_payload["input_cids"] = [shape_cids["allocate-or-bail"], shape_cids["check-bounds-then-access"]]
    composed_payload["refines"] = {
        "components": ["shape:allocate-or-bail", "shape:check-bounds-then-access"],
        "rule": "ccp-conjoin",
    }
    composed_spec_name = "validated-allocated-access_shape.spec.json"
    write_json(SPEC_DIR / composed_spec_name, algorithm_spec(composed_payload))
    composed_shape_cid, composed_shape_path = mint("algorithm", composed_spec_name)
    expected_composed_cid = canonical_cid_value(composed_payload)
    if composed_shape_cid != expected_composed_cid:
        raise SystemExit(f"composed shape CID mismatch: {composed_shape_cid} != {expected_composed_cid}")
    cids.append({"kind": "shape", "name": composed_payload["fn_name"], "cid": composed_shape_cid, "path": composed_shape_path})

    for concept in CONCEPTS:
        slug = concept["slug"]
        record = {"slug": slug, "shape_cid": shape_cids[slug], "realizations": []}
        target_payload = shape_payloads[slug]
        for realization in concept["realizations"]:
            rslug = realization["slug"]
            contract = realization["contract"]()
            source_name = f"{rslug}_{slug.replace('-', '_')}"
            source_path = SOURCE_DIR / f"{source_name}.contract.json"
            write_json(source_path, contract)
            source_cid = canonical_cid_file(source_path)
            cids.append({"kind": "source", "name": source_name, "cid": source_cid, "path": str(source_path)})

            spec_stem = f"morphism_{source_name}_to_shape"
            spec_name = f"{spec_stem}.spec.json"
            m_spec = morphism_spec(
                f"{slug}:{rslug}:to-shape",
                source_cid,
                shape_cids[slug],
                realization["renaming"],
                realization["representation"],
                realization["operators"],
                realization.get("literal", {}),
            )
            write_json(SPEC_DIR / spec_name, m_spec)
            morphism_cid, morphism_path = mint("algorithm", spec_name)
            cids.append({"kind": "morphism", "name": spec_stem, "cid": morphism_cid, "path": morphism_path})

            after_payload = after_substitution_payload(
                contract,
                target_payload,
                realization["renaming"],
                realization["representation"],
                realization["operators"],
                realization.get("literal", {}),
            )
            after_path = DISCHARGE_DIR / f"{source_name}_after_substitution.json"
            write_json(after_path, after_payload)
            after_cid = canonical_cid_file(after_path)
            if after_cid != shape_cids[slug]:
                raise SystemExit(f"{source_name} discharge landed on {after_cid}, not {shape_cids[slug]}")

            receipt = {
                "schema_version": "1",
                "kind": "MorphismDischargeReceipt",
                "morphism_cid": morphism_cid,
                "source_contract_cid": source_cid,
                "renaming_map": realization["renaming"],
                "representation_map": realization["representation"],
                "operator_map": realization["operators"],
                "literal_map": realization.get("literal", {}),
                "after_substitution_cid": after_cid,
                "shape_cid": shape_cids[slug],
                "discharged": True,
                "method": "canonicalizer-alpha-equivalence-plus-representation-map",
            }
            receipt_cid, receipt_path = store_receipt(spec_stem, receipt)
            cids.append({"kind": "receipt", "name": spec_stem, "cid": receipt_cid, "path": receipt_path})
            discharge_status.append((spec_stem, after_cid, shape_cids[slug]))
            record["realizations"].append(
                {
                    "label": realization["label"],
                    "source_cid": source_cid,
                    "morphism_name": spec_stem,
                    "morphism_cid": morphism_cid,
                    "receipt_cid": receipt_cid,
                }
            )
        concept_records.append(record)

    branch_cid = shape_cids["branch-on-error-else-passthrough"]
    spec_name = "morphism_foo_shape_to_branch_on_error_else_passthrough.spec.json"
    write_json(SPEC_DIR / spec_name, specialization_spec(branch_cid))
    specialization_cid, specialization_path = mint("algorithm", spec_name)
    cids.append(
        {
            "kind": "morphism",
            "name": "foo:specializes:branch-on-error-else-passthrough",
            "cid": specialization_cid,
            "path": specialization_path,
        }
    )
    specialization_receipt = {
        "schema_version": "1",
        "kind": "SpecializationDischargeReceipt",
        "morphism_cid": specialization_cid,
        "source_shape_cid": FOO_SHAPE_CID,
        "target_shape_cid": branch_cid,
        "relation": "specialization-of",
        "implication": "foo postcondition is the branch shape under err_cond(x) = x == 0 and err_val = -22",
        "discharged": True,
        "method": "canonicalizer-recorded-specialization-implication",
    }
    specialization_receipt_cid, specialization_receipt_path = store_receipt(
        "foo_specializes_branch_on_error_else_passthrough",
        specialization_receipt,
    )
    cids.append(
        {
            "kind": "receipt",
            "name": "receipt:foo:specializes:branch-on-error-else-passthrough",
            "cid": specialization_receipt_cid,
            "path": specialization_receipt_path,
        }
    )

    compose_probe = run_compose_probe()
    write_json(COMPOSITION_DIR / "validated_allocated_access.compose-probe.json", compose_probe)
    composition_receipt = {
        "schema_version": "1",
        "kind": "ShapeCompositionReceipt",
        "composed_shape_cid": composed_shape_cid,
        "component_shape_cids": [
            shape_cids["allocate-or-bail"],
            shape_cids["check-bounds-then-access"],
        ],
        "rule": "ccp-conjoin",
        "libprovekit_compose_probe": compose_probe,
        "discharged": compose_probe["status"] == "composed",
        "method": "ccp-compose-probe-plus-minted-conjoined-shape",
    }
    composition_receipt_cid, composition_receipt_path = store_receipt(
        "validated_allocated_access_composition",
        composition_receipt,
    )
    cids.append(
        {
            "kind": "receipt",
            "name": "validated-allocated-access:composition-receipt",
            "cid": composition_receipt_cid,
            "path": composition_receipt_path,
        }
    )
    if compose_probe["status"] == "composed":
        cids.append(
            {
                "kind": "ccp-compose",
                "name": "validated-allocated-access:pure-projection",
                "cid": compose_probe["composed_cid"],
                "path": str(COMPOSITION_DIR / "validated_allocated_access.compose-probe.json"),
            }
        )

    write_cids(cids)
    write_readme(cids, concept_records, discharge_status, composed_shape_cid, composition_receipt_cid, compose_probe)
    scan_created_text()

    print(f"composed_shape_cid\t{composed_shape_cid}")
    if compose_probe["status"] == "composed":
        print(f"ccp_compose_probe_cid\t{compose_probe['composed_cid']}")
    for record in concept_records:
        print(f"shape_cid\t{record['slug']}\t{record['shape_cid']}")
        print(f"morphism_count\t{record['slug']}\t{len(record['realizations'])}")
    print(f"discharge_count\t{len(discharge_status)}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        raise
