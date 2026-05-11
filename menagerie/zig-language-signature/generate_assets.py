#!/usr/bin/env python3
import json
from pathlib import Path

BASE = Path(__file__).resolve().parent
SPECS = BASE / "specs"
VERSION = "0.1.0-draft"
LOCUS = "menagerie/zig-language-signature/README.md"


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def sort(name):
    return {"kind": "ctor", "name": name, "args": []}


def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def effect_rule(rule):
    return {"effects": [{"kind": "effect-polymorphic", "rule": rule}]}


def effect_sig(name):
    return {"effects": [{"kind": "effect-signature", "name": name}]}


def named_slots(*names):
    slots = []
    for name in names:
        if isinstance(name, tuple):
            slot = {"name": name[0]}
            slot.update(name[1])
            slots.append(slot)
        else:
            slots.append({"name": name})
    return {"kind": "named", "slots": slots}


def positional(n):
    return {"kind": "positional", "arity": n}


def op(name, formals, sorts, result, wp, arity_shape, effects=None):
    return {
        "kind": "algorithm",
        "version": VERSION,
        "fn_name": f"zig:{name}",
        "formals": formals,
        "formal_sorts": [sort(s) for s in sorts],
        "return_sort": sort(result),
        "pre": true_formula(),
        "post": {
            "kind": "operation-contract",
            "operator": name,
            "arity": sorts,
            "result": result,
            "wp": wp,
            "arity_shape": arity_shape,
        },
        "effects": effects or {"effects": []},
        "locus": LOCUS,
    }


def main():
    sorts = {
        "int": ("Int", "Zig integer values in the AST-level source subset."),
        "bool": ("Bool", "Zig boolean values."),
        "string": ("String", "String and byte-slice literals used by the source algebra."),
        "unit": ("Unit", "Unit payload for zero-argument statement operations."),
        "stmt": ("Stmt", "Statement-level Zig source terms."),
        "expr": ("Expr", "Expression-level Zig source terms."),
        "lvalue": ("LValue", "Assignable Zig storage designators."),
        "ptr": ("Ptr", "Pointer-like Zig source values."),
        "fieldname": ("FieldName", "Struct, namespace, tuple, or field names."),
        "listofexpr": ("ListOfExpr", "Finite ordered expression argument lists."),
        "reason": ("Reason", "Reason payload for panic-like divergence."),
        "bottom": ("Bottom", "Divergence result sort for panic and unreachable."),
    }
    for key, (name, desc) in sorts.items():
        write(SPECS / f"sort_{key}.spec.json", {
            "kind": "sort",
            "version": VERSION,
            "fn_name": f"zig:{name}",
            "formals": [],
            "return_sort": {"kind": "kind", "name": "*"},
            "post": {"kind": "sort-description", "name": name, "description": desc},
        })

    ops = {
        "unit": op("unit", [], [], "Unit", "unit payload", positional(0)),
        "skip": op("skip", ["unit"], ["Unit"], "Stmt", "state unchanged", named_slots("unit")),
        "seq": op("seq", ["first", "second"], ["Stmt", "Stmt"], "Stmt", "wp(first, wp(second, post))", positional(2), effect_rule("union(first.effects, second.effects)")),
        "source-unit": op("source-unit", ["bytes", "operational_term"], ["String", "Stmt"], "Stmt", "lossless Zig source wrapper; bytes are recoverable and operational_term is the lifted program", named_slots(("bytes", {"evaluation": "unevaluated", "slot_sort": "literal"}), "operational_term")),
        "decl": op("decl", ["name", "value"], ["String", "Expr"], "Stmt", "bind local name to value", named_slots("name", "value")),
        "assign": op("assign", ["target", "value"], ["LValue", "Expr"], "Stmt", "store value into target", named_slots("target", "value"), effect_sig("MemWrite")),
        "return": op("return", ["value"], ["Expr"], "Stmt", "bind function return value and exit", named_slots("value")),
        "if": op("if", ["cond", "then_branch", "else_branch"], ["Bool", "Stmt", "Stmt"], "Stmt", "branch-selected weakest precondition", named_slots("cond", "then_branch", "else_branch"), effect_rule("union(then_branch.effects, else_branch.effects)")),
        "while": op("while", ["cond", "body"], ["Bool", "Stmt"], "Stmt", "opaque loop requiring an invariant memento keyed by loopCid", named_slots("cond", "body"), effect_sig("OpaqueLoop")),
        "for": op("for", ["iterable", "body"], ["Expr", "Stmt"], "Stmt", "opaque for loop requiring an invariant memento keyed by loopCid", named_slots("iterable", "body"), effect_sig("OpaqueLoop")),
        "break": op("break", ["unit"], ["Unit"], "Stmt", "exit nearest loop", named_slots("unit")),
        "continue": op("continue", ["unit"], ["Unit"], "Stmt", "continue nearest loop", named_slots("unit")),
        "call": op("call", ["callee", "args"], ["String", "ListOfExpr"], "Expr", "call callee with args", named_slots("callee", ("args", {"shape": {"kind": "set"}})), effect_rule("callee.effects or unresolved_call when unavailable")),
        "field": op("field", ["base", "field"], ["Expr", "FieldName"], "Expr", "field projection", named_slots("base", "field")),
        "index": op("index", ["base", "index"], ["Expr", "Int"], "LValue", "index projection", named_slots("base", "index")),
        "deref": op("deref", ["pointer"], ["Ptr"], "LValue", "pointer dereference p.*", named_slots("pointer"), effect_sig("MemRead")),
        "addr": op("addr", ["target"], ["LValue"], "Ptr", "address-of &target", named_slots("target")),
        "cast": op("cast", ["target_sort", "value"], ["String", "Expr"], "Expr", "explicit @as target_sort cast", named_slots("target_sort", "value")),
        "panic": op("panic", ["reason"], ["Reason"], "Bottom", "controlled @panic divergence", named_slots("reason"), effect_sig("Panic")),
        "unreachable": op("unreachable", [], [], "Bottom", "unreachable divergence", positional(0), effect_sig("Panic")),
    }
    for key, symbol in {
        "add": "+", "sub": "-", "mul": "*", "div": "/", "mod": "%",
        "eq": "==", "ne": "!=", "lt": "<", "le": "<=", "gt": ">", "ge": ">=",
        "bitand": "&", "bitor": "|", "bitxor": "^", "shl": "<<", "shr": ">>",
    }.items():
        result = "Bool" if key in {"eq", "ne", "lt", "le", "gt", "ge"} else "Int"
        ops[key] = op(key, ["lhs", "rhs"], ["Int", "Int"], result, f"Zig binary {symbol} over modeled operands", named_slots("lhs", "rhs"))
    ops["and"] = op("and", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit boolean and", named_slots("lhs", ("rhs", {"evaluation": "unevaluated"})))
    ops["or"] = op("or", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", "short-circuit boolean or", named_slots("lhs", ("rhs", {"evaluation": "unevaluated"})))
    for key, symbol in {"not": "!", "neg": "-", "bitnot": "~"}.items():
        result = "Bool" if key == "not" else "Int"
        arg_sort = "Bool" if key == "not" else "Int"
        ops[key] = op(key, ["value"], [arg_sort], result, f"Zig unary {symbol}", named_slots("value"))

    for key, value in ops.items():
        write(SPECS / f"op_{key}.spec.json", value)

    equations = {
        "seq_assoc": ("seq-assoc", ["a", "b", "c"], ["stmt", "stmt", "stmt"], "seq(seq(a,b),c)=seq(a,seq(b,c))"),
        "seq_skip_left": ("seq-skip-left", ["a"], ["stmt"], "seq(skip,a)=a"),
        "seq_skip_right": ("seq-skip-right", ["a"], ["stmt"], "seq(a,skip)=a"),
        "and_true": ("and-true", ["b"], ["bool"], "and(true,b)=b"),
        "and_false": ("and-false", ["b"], ["bool"], "and(false,b)=false"),
        "or_true": ("or-true", ["b"], ["bool"], "or(true,b)=true"),
        "or_false": ("or-false", ["b"], ["bool"], "or(false,b)=b"),
    }
    for key, (name, formals, formal_sorts, law) in equations.items():
        write(SPECS / f"eq_{key}.spec.json", {
            "kind": "equation",
            "version": VERSION,
            "fn_name": f"zig:{name}",
            "formals": formals,
            "formal_sorts": [f"sort_{s}.spec.json" for s in formal_sorts],
            "pre": true_formula(),
            "post": {"kind": "equation", "law": law},
            "locus": LOCUS,
        })

    for key, name in {
        "read": "MemRead", "write": "MemWrite", "io": "IO", "unsafe": "Unsafe", "panic": "Panic", "unresolved_call": "UnresolvedCall", "opaque_loop": "OpaqueLoop",
    }.items():
        write(SPECS / f"effsig_{key}.spec.json", {
            "kind": "effect-signature",
            "version": VERSION,
            "fn_name": f"zig:effect:{name}",
            "formals": [],
            "return_sort": sort("Unit"),
            "pre": true_formula(),
            "post": {"kind": "effect-signature-description", "name": name},
            "effects": {"effects": []},
            "locus": LOCUS,
        })

    write(SPECS / "language_signature_zig.spec.json", {
        "kind": "language_signature",
        "version": VERSION,
        "fn_name": "zig",
        "sorts": sorted(p.name for p in SPECS.glob("sort_*.spec.json")),
        "operations": sorted(p.name for p in SPECS.glob("op_*.spec.json")),
        "equations": sorted(p.name for p in SPECS.glob("eq_*.spec.json")),
        "effects": [],
        "effect_signatures": sorted(p.name for p in SPECS.glob("effsig_*.spec.json")),
        "locus": LOCUS,
    })

if __name__ == "__main__":
    main()
