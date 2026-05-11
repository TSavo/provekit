#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
SPECS = ROOT / "specs"
VERSION = "0.1.0-draft"
LOCUS = "menagerie/ruby-language-signature/README.md"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    generated = generate()
    if args.check:
        stale = []
        for rel, value in generated.items():
            path = ROOT / rel
            expected = json_text(value)
            actual = path.read_text(encoding="utf-8") if path.exists() else None
            if actual != expected:
                stale.append(rel)
        if stale:
            raise SystemExit("ruby-language-signature specs are stale: " + ", ".join(stale))
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
        files[f"specs/sort_{file_key(name)}.spec.json"] = sort_spec(name)

    operations = operation_specs()
    effects = effect_specs()
    effect_sigs = effect_signature_specs()
    files.update({f"specs/{name}": spec for name, spec in operations.items()})
    files.update({f"specs/{name}": spec for name, spec in effects.items()})
    files.update({f"specs/{name}": spec for name, spec in effect_sigs.items()})
    files["specs/language_signature_ruby.spec.json"] = {
        "kind": "language_signature",
        "fn_name": "ruby",
        "version": VERSION,
        "sorts": [f"sort_{file_key(name)}.spec.json" for name in sort_names],
        "operations": list(operations.keys()),
        "equations": [],
        "effects": list(effects.keys()),
        "effect_signatures": list(effect_sigs.keys()),
        "locus": LOCUS,
    }
    return files


def sort_spec(name: str) -> dict[str, Any]:
    return {
        "kind": "sort",
        "fn_name": f"ruby:{name}",
        "formals": [],
        "return_sort": {"kind": "kind", "name": "*"},
        "post": {"kind": "sort-description", "name": name, "description": f"Ruby {name} term sort."},
    }


def operation_specs() -> dict[str, dict[str, Any]]:
    ops: list[tuple[str, list[str], list[str], str, str, dict[str, Any], list[str], str | None]] = [
        (
            "source-unit",
            ["bytes", "operational_term"],
            ["String", "Stmt"],
            "Stmt",
            "lossless Ruby source wrapper; project_effects descends to operational_term",
            named([
                {"name": "bytes", "evaluation": "unevaluated", "slot_sort": "literal"},
                {"name": "operational_term"},
            ]),
            [],
            "The bytes slot carries the original UTF-8 Ruby source as a string.",
        ),
        ("expr", ["value"], ["Value"], "Stmt", "evaluate expression for effects/result", named_names("value"), [], None),
        ("seq", ["first", "second"], ["Stmt", "Stmt"], "Stmt", "wp(first, wp(second, post))", positional(2), [], None),
        ("assign", ["target", "value"], ["Value", "Value"], "Stmt", "store value in target", named_names("target", "value"), ["Writes"], None),
        ("if", ["cond", "then_branch", "else_branch"], ["Bool", "Stmt", "Stmt"], "Stmt", "Ruby if statement", named_names("cond", "then_branch", "else_branch"), [], None),
        ("while", ["cond", "body"], ["Bool", "Stmt"], "Stmt", "opaque Ruby while loop awaiting invariant memento", named_names("cond", "body"), ["OpaqueLoop"], None),
        ("until", ["cond", "body"], ["Bool", "Stmt"], "Stmt", "opaque Ruby until loop awaiting invariant memento", named_names("cond", "body"), ["OpaqueLoop"], None),
        ("for", ["target", "iterable", "body"], ["Value", "Value", "Stmt"], "Stmt", "opaque Ruby for loop over iterable", named_names("target", "iterable", "body"), ["OpaqueLoop"], None),
        ("return", ["value"], ["Value"], "Stmt", "bind function return value", named_names("value"), [], None),
        ("raise", ["exception"], ["Value"], "Stmt", "raise Ruby exception", named_names("exception"), ["Panics"], None),
        (
            "call",
            ["callee", "args"],
            ["String", "ListOfValue"],
            "Value",
            "call callee with positional arguments",
            named([{"name": "callee", "evaluation": "unevaluated", "slot_sort": "literal"}, {"name": "args", "shape": {"kind": "set"}}]),
            ["UnresolvedCall"],
            "The term encoding stores callee followed by positional actual arguments.",
        ),
        (
            "send",
            ["receiver", "method", "args"],
            ["Value", "String", "ListOfValue"],
            "Value",
            "send method to receiver with positional arguments",
            named([
                {"name": "receiver"},
                {"name": "method", "evaluation": "unevaluated", "slot_sort": "literal"},
                {"name": "args", "shape": {"kind": "set"}},
            ]),
            ["UnresolvedCall"],
            "The term encoding stores receiver, method, then positional actual arguments.",
        ),
        ("index", ["base", "index"], ["Value", "Value"], "Value", "Ruby index expression base[index]", named_names("base", "index"), ["Reads"], None),
        ("ivar", ["name"], ["String"], "Value", "instance variable access", literal_name_shape(), ["Reads"], None),
        ("gvar", ["name"], ["String"], "Value", "global variable access", literal_name_shape(), ["Reads"], None),
        ("cvar", ["name"], ["String"], "Value", "class variable access", literal_name_shape(), ["Reads"], None),
        ("const", ["name"], ["String"], "Value", "constant access", literal_name_shape(), ["Reads"], None),
        ("and", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit Ruby conjunction", short_circuit_shape(), [], None),
        ("or", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit Ruby disjunction", short_circuit_shape(), [], None),
        ("not", ["operand"], ["Bool"], "Bool", "boolean negation", named_names("operand"), [], None),
        (
            "ternary",
            ["cond", "then_expr", "else_expr"],
            ["Bool", "Value", "Value"],
            "Value",
            "Ruby cond ? then_expr : else_expr",
            named([
                {"name": "cond"},
                {"name": "then_expr", "evaluation": "unevaluated"},
                {"name": "else_expr", "evaluation": "unevaluated"},
            ]),
            [],
            None,
        ),
    ]

    for name in ["add", "sub", "mul", "div", "mod", "pow", "eq", "ne", "lt", "le", "gt", "ge", "bitand", "bitor", "bitxor", "shl", "shr"]:
        ops.append((name, ["lhs", "rhs"], ["Value", "Value"], "Value", f"Ruby {name} expression", named_names("lhs", "rhs"), [], None))
    for name in ["neg", "pos", "bitnot"]:
        ops.append((name, ["operand"], ["Value"], "Value", f"Ruby unary {name} expression", named_names("operand"), [], None))

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
        "reads": ("Reads", ["target"], ["String"], "Value", "read named Ruby state cell"),
        "writes": ("Writes", ["target"], ["String"], "Unit", "write named Ruby state cell"),
        "io": ("IO", ["unit"], ["Unit"], "Unit", "perform Ruby IO"),
        "panics": ("Panics", ["exception"], ["Value"], "Unit", "raise Ruby exception"),
        "unresolved_call": ("UnresolvedCall", ["name"], ["String"], "Value", "call without a resolved contract"),
        "opaque_loop": ("OpaqueLoop", ["loop_cid"], ["String"], "Unit", "loop awaiting invariant memento"),
    }
    return {f"eff_{key}.spec.json": effect_op_spec(key, *value) for key, value in specs.items()}


def effect_signature_specs() -> dict[str, dict[str, Any]]:
    specs = {
        "reads": ("Reads", ["sort_string.spec.json", "sort_value.spec.json"], ["eff_reads.spec.json"]),
        "writes": ("Writes", ["sort_string.spec.json", "sort_unit.spec.json"], ["eff_writes.spec.json"]),
        "io": ("IO", ["sort_unit.spec.json"], ["eff_io.spec.json"]),
        "panics": ("Panics", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_panics.spec.json"]),
        "unresolved_call": ("UnresolvedCall", ["sort_string.spec.json", "sort_value.spec.json"], ["eff_unresolved_call.spec.json"]),
        "opaque_loop": ("OpaqueLoop", ["sort_string.spec.json", "sort_unit.spec.json"], ["eff_opaque_loop.spec.json"]),
    }
    return {
        f"effsig_{key}.spec.json": {
            "kind": "effect_signature",
            "fn_name": f"ruby:effect-signature:{name}",
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
        "fn_name": f"ruby:{name}",
        "formals": formals,
        "formal_sorts": [sort_ctor(sort) for sort in formal_sorts],
        "return_sort": sort_ctor(result),
        "pre": true_formula(),
        "post": post,
        "effects": {"effects": [{"kind": "effect-signature", "name": effect_name} for effect_name in effect_names]},
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


def literal_name_shape() -> dict[str, Any]:
    return named([{"name": "name", "evaluation": "unevaluated", "slot_sort": "literal"}])


def file_key(name: str) -> str:
    return name.lower().replace("of", "of_")


def json_text(value: dict[str, Any]) -> str:
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


if __name__ == "__main__":
    main()
