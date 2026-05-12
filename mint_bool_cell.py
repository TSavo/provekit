#!/usr/bin/env python3
"""Mint concept:bool-cell and C realization (pointer-indirection pattern)."""

import json
import sys
from pathlib import Path


def primitive(name):
    return {"kind": "primitive", "name": name}


def ctor(name, args=None):
    return {"kind": "ctor", "name": name, "args": args or []}


def var(name):
    return {"kind": "var", "name": name}


def const(value, sort_name):
    return {"kind": "const", "value": value, "sort": primitive(sort_name)}


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


def empty_effects():
    return {"effects": []}


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


def bool_cell_c_get_contract():
    """BOOL_CELL_GET(c) -> dereferences c and returns the bool value."""
    cell = var("cell")
    val = var("val")
    post = and_formula([
        eq(var("result"), val),
        implies(
            atomic("non_null", [cell]),
            atomic("bool_value_at", [cell, val])
        ),
    ])
    effects = {
        "effects": [
            {
                "kind": "MemRead",
                "target": "cell",
                "index": None,
                "guard": atomic("non_null", [cell]),
            }
        ]
    }
    return source_contract(
        "c:bool_cell_get",
        ["cell", "val"],
        ["bool_ptr", "bool"],
        "bool",
        atomic("non_null", [cell]),
        post,
        effects,
    )


def bool_cell_c_set_contract():
    """BOOL_CELL_SET(c, v) -> sets the bool value at c to v, returns void-like (0)."""
    cell = var("cell")
    val = var("val")
    post = and_formula([
        atomic("bool_value_at", [cell, val]),
    ])
    effects = {
        "effects": [
            {
                "kind": "MemWrite",
                "target": "cell",
                "index": None,
                "value": val,
                "guard": atomic("non_null", [cell]),
            }
        ]
    }
    return source_contract(
        "c:bool_cell_set",
        ["cell", "val"],
        ["bool_ptr", "bool"],
        "int",  # returns 0 for void-like
        atomic("non_null", [cell]),
        post,
        effects,
    )


def bool_cell_c_new_contract():
    """BOOL_CELL_NEW() -> malloc(sizeof(bool)), returns bool_ptr."""
    ptr = var("ptr")
    sz = var("sizeof_bool")
    failed_term = ctor("malloc_failed", [sz])
    failed_formula = atomic("malloc_failed", [sz])
    post = and_formula([
        eq(var("result"), ctor("ite", [failed_term, const("null", "bool_ptr"), ptr])),
        implies(failed_formula, eq(ptr, const("null", "bool_ptr"))),
        implies(not_formula(failed_formula), atomic("non_null", [ptr])),
    ])
    effects = {
        "effects": [
            {
                "kind": "Alloc",
                "result": "ptr",
                "size": "sz",
                "failure_condition": failed_term,
            }
        ]
    }
    return source_contract(
        "c:bool_cell_new",
        ["sz", "ptr"],
        ["size_t", "bool_ptr"],
        "bool_ptr",
        true_formula(),
        post,
        effects,
    )


def main():
    """Generate and print the three bool_cell C contracts as JSON."""
    contracts = [
        ("bool_cell_c_get", bool_cell_c_get_contract()),
        ("bool_cell_c_set", bool_cell_c_set_contract()),
        ("bool_cell_c_new", bool_cell_c_new_contract()),
    ]

    for name, contract in contracts:
        print(json.dumps(contract, indent=2))
        print("\n---\n")


if __name__ == "__main__":
    main()
