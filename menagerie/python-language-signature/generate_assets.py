#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
SPECS = ROOT / "specs"
VERSION = "0.1.0-draft"
LOCUS = "menagerie/python-language-signature/README.md"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    generated = generate()
    if args.check:
        missing_or_changed = []
        for rel, value in generated.items():
            path = ROOT / rel
            expected = json_text(value)
            actual = path.read_text(encoding="utf-8") if path.exists() else None
            if actual != expected:
                missing_or_changed.append(rel)
        if missing_or_changed:
            raise SystemExit(
                "python-language-signature specs are stale: "
                + ", ".join(missing_or_changed)
            )
        return

    SPECS.mkdir(parents=True, exist_ok=True)
    for rel, value in generated.items():
        path = ROOT / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json_text(value), encoding="utf-8")


def generate() -> dict[str, dict[str, Any]]:
    files: dict[str, dict[str, Any]] = {}
    sort_names = ["Value", "Bool", "String", "Unit", "Stmt", "ListOfValue"]
    for name in sort_names:
        files[f"specs/sort_{file_key(name)}.spec.json"] = sort_spec(
            name, f"Python {name} term sort."
        )

    operations = operation_specs()
    for filename, spec in operations.items():
        files[f"specs/{filename}"] = spec

    effects = effect_specs()
    for filename, spec in effects.items():
        files[f"specs/{filename}"] = spec

    effect_sigs = effect_signature_specs()
    for filename, spec in effect_sigs.items():
        files[f"specs/{filename}"] = spec

    files["specs/language_signature_python.spec.json"] = {
        "kind": "language_signature",
        "fn_name": "python",
        "version": VERSION,
        "sorts": [f"sort_{file_key(name)}.spec.json" for name in sort_names],
        "operations": list(operations.keys()),
        "equations": [],
        "effects": list(effects.keys()),
        "effect_signatures": list(effect_sigs.keys()),
        "locus": LOCUS,
    }
    return files


def sort_spec(name: str, description: str) -> dict[str, Any]:
    return {
        "kind": "sort",
        "fn_name": f"python:{name}",
        "formals": [],
        "return_sort": {"kind": "kind", "name": "*"},
        "post": {"kind": "sort-description", "name": name, "description": description},
    }


def operation_specs() -> dict[str, dict[str, Any]]:
    ops: list[tuple[str, list[str], list[str], str, str, dict[str, Any], list[str], str | None]] = [
        (
            "source-unit",
            ["bytes", "operational_term"],
            ["String", "Stmt"],
            "Stmt",
            "lossless Python source wrapper; project_effects descends to operational_term",
            named(
                [
                    {"name": "bytes", "evaluation": "unevaluated", "slot_sort": "literal"},
                    {"name": "operational_term"},
                ]
            ),
            [],
            "The bytes slot carries the original UTF-8 source as a string.",
        ),
        ("pass", ["unit"], ["Unit"], "Stmt", "Python pass statement", positional(1), [], None),
        ("seq", ["first", "second"], ["Stmt", "Stmt"], "Stmt", "wp(first, wp(second, post))", positional(2), [], None),
        ("assign", ["target", "value"], ["Value", "Value"], "Stmt", "store value in target", named_names("target", "value"), ["Write"], None),
        ("if", ["cond", "then_branch", "else_branch"], ["Bool", "Stmt", "Stmt"], "Stmt", "branch over cond", named_names("cond", "then_branch", "else_branch"), [], None),
        ("while", ["cond", "body"], ["Bool", "Stmt"], "Stmt", "opaque loop awaiting invariant memento", named_names("cond", "body"), ["OpaqueLoop"], None),
        ("for", ["target", "iterable", "body"], ["Value", "Value", "Stmt"], "Stmt", "opaque Python for loop over iterable", named_names("target", "iterable", "body"), ["OpaqueLoop"], None),
        ("return", ["value"], ["Value"], "Stmt", "bind function return value", named_names("value"), [], None),
        ("expr", ["value"], ["Value"], "Stmt", "evaluate expression for effects", named_names("value"), [], None),
        ("break", ["unit"], ["Unit"], "Stmt", "break nearest loop", positional(1), [], None),
        ("continue", ["unit"], ["Unit"], "Stmt", "continue nearest loop", positional(1), [], None),
        ("raise", ["exception"], ["Value"], "Stmt", "raise exception", named_names("exception"), ["Panic"], None),
        (
            "call",
            ["callee", "args"],
            ["String", "ListOfValue"],
            "Value",
            "call callee with actual arguments",
            named([{"name": "callee"}, {"name": "args", "shape": {"kind": "set"}}]),
            ["UnresolvedCall"],
            "The term encoding stores callee followed by positional actual arguments.",
        ),
        ("attribute", ["object", "field"], ["Value", "String"], "Value", "attribute projection", named([{"name": "object"}, {"name": "field", "evaluation": "unevaluated", "slot_sort": "literal"}]), ["Read"], None),
        ("subscript", ["object", "index"], ["Value", "Value"], "Value", "subscript projection", named_names("object", "index"), ["Read"], None),
        ("and", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit boolean conjunction", short_circuit_shape(), [], None),
        ("or", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit boolean disjunction", short_circuit_shape(), [], None),
        ("not", ["operand"], ["Bool"], "Bool", "boolean negation", named_names("operand"), [], None),
        ("compare", ["operator", "lhs", "rhs"], ["String", "Value", "Value"], "Bool", "single Python comparison operation", named([{"name": "operator", "evaluation": "unevaluated", "slot_sort": "literal"}, {"name": "lhs"}, {"name": "rhs"}]), [], "Chained comparisons are refused by the draft lifter."),
    ]

    for name in ["add", "sub", "mul", "div", "floordiv", "mod", "pow", "lshift", "rshift", "bitand", "bitor", "bitxor"]:
        ops.append((name, ["lhs", "rhs"], ["Value", "Value"], "Value", f"Python {name} expression", named_names("lhs", "rhs"), [], None))
    for name in ["neg", "pos", "bitnot"]:
        ops.append((name, ["operand"], ["Value"], "Value", f"Python unary {name} expression", named_names("operand"), [], None))

    return {
        f"op_{name}.spec.json": op_spec(
            name=name,
            formals=formals,
            formal_sorts=formal_sorts,
            result=result,
            wp=wp,
            shape=shape,
            effect_names=effect_names,
            notes=notes,
        )
        for name, formals, formal_sorts, result, wp, shape, effect_names, notes in ops
    }


def effect_specs() -> dict[str, dict[str, Any]]:
    specs = {
        "read": ("Read", ["target"], ["String"], "Value", "read named Python state cell"),
        "write": ("Write", ["target"], ["String"], "Unit", "write named Python state cell"),
        "io": ("IO", ["unit"], ["Unit"], "Unit", "perform Python IO"),
        "panic": ("Panic", ["exception"], ["Value"], "Unit", "raise Python exception"),
        "unresolved_call": ("UnresolvedCall", ["name"], ["String"], "Value", "call without a resolved contract"),
        "opaque_loop": ("OpaqueLoop", ["loop_cid"], ["String"], "Unit", "loop awaiting invariant memento"),
    }
    return {
        f"eff_{key}.spec.json": effect_op_spec(key, *value)
        for key, value in specs.items()
    }


def effect_signature_specs() -> dict[str, dict[str, Any]]:
    specs = {
        "read": ("Read", ["sort_string.spec.json", "sort_value.spec.json"], ["eff_read.spec.json"]),
        "write": ("Write", ["sort_string.spec.json", "sort_unit.spec.json"], ["eff_write.spec.json"]),
        "io": ("IO", ["sort_unit.spec.json"], ["eff_io.spec.json"]),
        "panic": ("Panic", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_panic.spec.json"]),
        "unresolved_call": ("UnresolvedCall", ["sort_string.spec.json", "sort_value.spec.json"], ["eff_unresolved_call.spec.json"]),
        "opaque_loop": ("OpaqueLoop", ["sort_string.spec.json", "sort_unit.spec.json"], ["eff_opaque_loop.spec.json"]),
    }
    return {
        f"effsig_{key}.spec.json": {
            "kind": "effect_signature",
            "fn_name": f"python:effect-signature:{name}",
            "sorts": sorts,
            "operations": operations,
            "equations": [],
            "effect_signatures": [],
        }
        for key, (name, sorts, operations) in specs.items()
    }


def op_spec(
    *,
    name: str,
    formals: list[str],
    formal_sorts: list[str],
    result: str,
    wp: str,
    shape: dict[str, Any],
    effect_names: list[str],
    notes: str | None,
) -> dict[str, Any]:
    post: dict[str, Any] = {
        "kind": "operation-contract",
        "operator": name,
        "arity": formal_sorts,
        "result": result,
        "wp": wp,
        "arity_shape": shape,
    }
    if notes is not None:
        post["notes"] = notes
    return {
        "kind": "algorithm",
        "fn_name": f"python:{name}",
        "formals": formals,
        "formal_sorts": [sort_ctor(sort) for sort in formal_sorts],
        "return_sort": sort_ctor(result),
        "pre": true_formula(),
        "post": post,
        "effects": {"effects": [{"kind": "effect-signature", "name": name} for name in effect_names]},
        "locus": LOCUS,
    }


def effect_op_spec(
    key: str,
    signature_name: str,
    formals: list[str],
    formal_sorts: list[str],
    result: str,
    wp: str,
) -> dict[str, Any]:
    return op_spec(
        name=f"effect:{key}",
        formals=formals,
        formal_sorts=formal_sorts,
        result=result,
        wp=wp,
        shape=named_names(*formals) if formals else positional(0),
        effect_names=[signature_name],
        notes=None,
    )


def sort_ctor(name: str) -> dict[str, Any]:
    return {"kind": "ctor", "name": name, "args": []}


def true_formula() -> dict[str, Any]:
    return {"kind": "atomic", "name": "true", "args": []}


def positional(arity: int) -> dict[str, Any]:
    return {"kind": "positional", "arity": arity}


def named(slots: list[dict[str, Any]]) -> dict[str, Any]:
    return {"kind": "named", "slots": slots}


def named_names(*names: str) -> dict[str, Any]:
    return named([{"name": name} for name in names])


def short_circuit_shape() -> dict[str, Any]:
    return named([{"name": "lhs"}, {"name": "rhs", "evaluation": "unevaluated"}])


def file_key(name: str) -> str:
    return name.lower().replace("of", "of_")


def json_text(value: dict[str, Any]) -> str:
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


if __name__ == "__main__":
    main()
