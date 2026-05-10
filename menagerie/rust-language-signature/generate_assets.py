#!/usr/bin/env python3
import json
from pathlib import Path

BASE = Path(__file__).resolve().parent
SPECS = BASE / "specs"
EXAMPLE = BASE / "example"


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=False) + "\n", encoding="utf-8")


def ctor(name, args=None):
    return {"kind": "ctor", "name": name, "args": args or []}


def sort_ref(name):
    return ctor(name)


def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def var(name):
    return {"kind": "var", "name": name}


def unit_term():
    return {"kind": "unit"}


def op(name, args=None):
    return {"kind": "op", "name": name, "args": args or []}


def const_bool(value):
    return {"kind": "const", "value": value, "sort": sort_ref("Bool")}


def const_int(value):
    return {"kind": "const", "value": value, "sort": sort_ref("Int")}


def effect_set(*effects):
    return {"effects": [{"kind": "effect-signature", "name": effect} for effect in effects]}


def effect_rule(rule):
    return {"effects": [{"kind": "effect-polymorphic", "rule": rule}]}


def operation_post(name, arity, result, wp, notes=None):
    post = {
        "kind": "operation-contract",
        "operator": name,
        "arity": arity,
        "result": result,
        "wp": wp,
    }
    if notes:
        post["notes"] = notes
    return post


def algorithm(name, formals, formal_sorts, result, pre, post, effects=None):
    return {
        "kind": "algorithm",
        "fn_name": f"rust:{name}",
        "formals": formals,
        "formal_sorts": [sort_ref(s) for s in formal_sorts],
        "return_sort": sort_ref(result),
        "pre": pre,
        "post": post,
        "effects": effects or {"effects": []},
        "locus": "menagerie/rust-language-signature/README.md#operations",
    }


def equation(name, formals, formal_sorts, lhs, rhs, pre=None, notes=None):
    body = {
        "kind": "equation",
        "fn_name": f"rust:{name}",
        "formals": formals,
        "formal_sorts": [f"sort_{s}.spec.json" for s in formal_sorts],
        "pre": pre or true_formula(),
        "post": {"kind": "equation", "lhs": lhs, "rhs": rhs},
    }
    if notes:
        body["post"]["notes"] = notes
    return body


def main():
    SPECS.mkdir(parents=True, exist_ok=True)
    EXAMPLE.mkdir(parents=True, exist_ok=True)

    sorts = {
        "stmt": ("Stmt", "Rust statement terms, the carrier sort for statement-level algebra terms"),
        "expr": ("Expr", "Rust expression terms"),
        "place": ("Place", "Assignable Rust places"),
        "int": ("Int", "Rust integer values with debug overflow modeled as Panic"),
        "bool": ("Bool", "Rust branch truth values"),
        "unit": ("Unit", "The unit input and output sort"),
        "fncontract": ("FnContract", "Function contract values used at call sites"),
        "fieldname": ("FieldName", "Struct, tuple, or enum field names"),
        "listofstmt": ("ListOfStmt", "Finite ordered statement lists"),
        "listofexpr": ("ListOfExpr", "Finite ordered expression lists"),
        "addr": ("Addr", "Abstract memory addresses for effect signatures"),
        "value": ("Value", "Abstract memory or IO values for effect signatures"),
        "reason": ("Reason", "Reason values for Panic or Unsafe divergence"),
        "bottom": ("Bottom", "Controlled divergence or unreachable result sort"),
        "ref": ("Ref", "Safe borrowed reference values"),
        "rawptr": ("RawPtr", "Unsafe raw pointer values"),
        "lifetime": ("Lifetime", "Rust lifetime and region values"),
        "result": ("Result", "Result carrier for question propagation"),
        "option": ("Option", "Option carrier for question propagation"),
        "closure": ("Closure", "Closure values with captured environment"),
        "slice": ("Slice", "Slice and array view values"),
        "box": ("Box", "Owned heap allocation values"),
        "matcharm": ("MatchArm", "One match arm with pattern, guard, and body"),
        "listofarm": ("ListOfArm", "Finite ordered match arm lists"),
        "pattern": ("Pattern", "Rust match and let patterns"),
        "sort": ("Sort", "Rust signature sort values used by cast"),
        "float": ("Float", "Rust floating point values as IEEE bit patterns"),
        "string": ("String", "Rust string values"),
    }
    for key, (name, description) in sorts.items():
        write_json(SPECS / f"sort_{key}.spec.json", {
            "kind": "sort",
            "fn_name": f"rust:{name}",
            "formals": [],
            "return_sort": {"kind": "kind", "name": "*"},
            "post": {"kind": "sort-description", "name": name, "description": description},
        })

    nonzero = {"kind": "atomic", "name": "nonzero", "args": [var("rhs")]}
    in_bounds = {"kind": "atomic", "name": "in_bounds", "args": [var("slice"), var("idx")]}
    valid_raw = {"kind": "atomic", "name": "valid_raw_ptr", "args": [var("ptr")]}
    live_place = {"kind": "atomic", "name": "place_live", "args": [var("place")]}

    operations = {
        "skip": algorithm("skip", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("skip", ["Unit"], "Stmt", "state unchanged")),
        "seq": algorithm("seq", ["first", "second"], ["Stmt", "Stmt"], "Stmt", true_formula(), operation_post("seq", ["Stmt", "Stmt"], "Stmt", "wp(first, wp(second, post))"), effect_rule("union(first.effects, second.effects)")),
        "if": algorithm("if", ["cond", "then_branch", "else_branch"], ["Bool", "Stmt", "Stmt"], "Stmt", true_formula(), operation_post("if", ["Bool", "Stmt", "Stmt"], "Stmt", "cond ? wp(then_branch, post) : wp(else_branch, post)"), effect_rule("union(then_branch.effects, else_branch.effects)")),
        "while": algorithm("while", ["cond", "body"], ["Bool", "Stmt"], "Stmt", true_formula(), operation_post("while", ["Bool", "Stmt"], "Stmt", "loop invariant holds and cond is false at exit", "Requires a loop invariant memento for nontrivial discharge"), effect_rule("body.effects plus loop invariant obligations")),
        "for": algorithm("for", ["init", "cond", "step", "body"], ["Stmt", "Bool", "Stmt", "Stmt"], "Stmt", true_formula(), operation_post("for", ["Stmt", "Bool", "Stmt", "Stmt"], "Stmt", "core C11 shaped for operation federation. Rust iterator for lowers by rust:for-desugar"), effect_rule("union(init.effects, body.effects, step.effects)")),
        "switch": algorithm("switch", ["scrutinee", "arms"], ["Int", "ListOfStmt"], "Stmt", true_formula(), operation_post("switch", ["Int", "ListOf<Stmt>"], "Stmt", "integer SwitchInt special case of match"), effect_rule("union(arm.effects for arm in arms)")),
        "call": algorithm("call", ["callee", "args"], ["FnContract", "ListOfExpr"], "Stmt", true_formula(), operation_post("call", ["FnContract", "ListOf<Expr>"], "Stmt", "callee pre under bound args implies callee post under caller state"), effect_rule("callee.effects")),
        "return": algorithm("return", ["value"], ["Expr"], "Stmt", true_formula(), operation_post("return", ["Expr"], "Stmt", "bind function out value and exit current body"), effect_rule("early-return")),
        "break": algorithm("break", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("break", ["Unit"], "Stmt", "exit nearest enclosing match or loop"), effect_rule("control-transfer")),
        "continue": algorithm("continue", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("continue", ["Unit"], "Stmt", "jump to next loop iteration"), effect_rule("control-transfer")),
        "deref": algorithm("deref", ["ref"], ["Ref"], "Place", true_formula(), operation_post("deref", ["Ref"], "Place", "safe reference dereference"), effect_set("MemRead")),
        "member": algorithm("member", ["base", "field"], ["Place", "FieldName"], "Place", true_formula(), operation_post("member", ["Place", "FieldName"], "Place", "field place projection")),
        "add": algorithm("add", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow_or_panic", "args": [op("add", [var("lhs"), var("rhs")])]}, operation_post("add", ["Int", "Int"], "Int", "mathematical integer addition when no overflow holds, otherwise Panic in debug"), effect_set("Panic")),
        "sub": algorithm("sub", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow_or_panic", "args": [op("sub", [var("lhs"), var("rhs")])]}, operation_post("sub", ["Int", "Int"], "Int", "mathematical integer subtraction when no overflow holds, otherwise Panic in debug"), effect_set("Panic")),
        "mul": algorithm("mul", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow_or_panic", "args": [op("mul", [var("lhs"), var("rhs")])]}, operation_post("mul", ["Int", "Int"], "Int", "mathematical integer multiplication when no overflow holds, otherwise Panic in debug"), effect_set("Panic")),
        "eq": algorithm("eq", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("eq", ["Int", "Int"], "Bool", "integer equality comparison")),
        "lt": algorithm("lt", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("lt", ["Int", "Int"], "Bool", "integer less than comparison")),
        "le": algorithm("le", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("le", ["Int", "Int"], "Bool", "integer less than or equal comparison")),
        "and": algorithm("and", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", true_formula(), operation_post("and", ["Bool", "Bool"], "Bool", "short circuit conjunction"), effect_rule("rhs evaluated only when lhs is true")),
        "or": algorithm("or", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", true_formula(), operation_post("or", ["Bool", "Bool"], "Bool", "short circuit disjunction"), effect_rule("rhs evaluated only when lhs is false")),
        "not": algorithm("not", ["value"], ["Bool"], "Bool", true_formula(), operation_post("not", ["Bool"], "Bool", "boolean negation")),
        "assign": algorithm("assign", ["target", "value"], ["Place", "Expr"], "Stmt", true_formula(), operation_post("assign", ["Place", "Expr"], "Stmt", "store value into target and update state"), effect_set("MemWrite")),
        "neg": algorithm("neg", ["value"], ["Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow_or_panic", "args": [op("neg", [var("value")])]}, operation_post("neg", ["Int"], "Int", "integer arithmetic negation when no overflow holds, otherwise Panic in debug"), effect_set("Panic")),
        "panic": algorithm("panic", ["reason"], ["Reason"], "Bottom", true_formula(), operation_post("panic", ["Reason"], "Bottom", "controlled divergence through Rust panic"), effect_set("Panic")),
        "try": algorithm("try", ["carrier"], ["Result"], "Expr", true_formula(), operation_post("try", ["Result"], "Expr", "Ok payload or early return of converted Err"), effect_set("Panic")),
        "try_option": algorithm("try_option", ["carrier"], ["Option"], "Expr", true_formula(), operation_post("try_option", ["Option"], "Expr", "Some payload or early return of None"), effect_rule("early-return None")),
        "match": algorithm("match", ["scrutinee", "arms"], ["Expr", "ListOfArm"], "Stmt", true_formula(), operation_post("match", ["Expr", "ListOfArm"], "Stmt", "guarded WP join over match arms"), effect_rule("union(arm.effects for arm in arms)")),
        "match_expr": algorithm("match_expr", ["scrutinee", "arms"], ["Expr", "ListOfArm"], "Expr", true_formula(), operation_post("match_expr", ["Expr", "ListOfArm"], "Expr", "expression valued match used by try desugaring"), effect_rule("union(arm.effects for arm in arms)")),
        "borrow": algorithm("borrow", ["place"], ["Place"], "Ref", live_place, operation_post("borrow", ["Place"], "Ref", "shared borrow introducing a lifetime")),
        "borrow_mut": algorithm("borrow_mut", ["place"], ["Place"], "Ref", live_place, operation_post("borrow_mut", ["Place"], "Ref", "mutable borrow introducing a lifetime"), effect_set("MemWrite")),
        "deref_raw": algorithm("deref_raw", ["ptr"], ["RawPtr"], "Place", valid_raw, operation_post("deref_raw", ["RawPtr"], "Place", "unsafe raw pointer dereference"), effect_set("Unsafe", "MemRead")),
        "drop": algorithm("drop", ["place"], ["Place"], "Stmt", true_formula(), operation_post("drop", ["Place"], "Stmt", "run drop glue for a place"), effect_set("Drop")),
        "await": algorithm("await", ["future"], ["Expr"], "Expr", true_formula(), operation_post("await", ["Expr"], "Expr", "wp_of_resumed_continuation"), effect_set("Async")),
        "index": algorithm("index", ["slice", "idx"], ["Slice", "Int"], "Place", in_bounds, operation_post("index", ["Slice", "Int"], "Place", "slice or array index place"), effect_set("Panic", "MemRead")),
        "box_new": algorithm("box_new", ["value"], ["Expr"], "Box", true_formula(), operation_post("box_new", ["Expr"], "Box", "allocate owned heap value"), effect_set("Alloc")),
        "closure": algorithm("closure", ["captures", "body"], ["ListOfExpr", "Stmt"], "Closure", true_formula(), operation_post("closure", ["ListOfExpr", "Stmt"], "Closure", "closure value with captures and body"), effect_set("ClosureCapture")),
        "closure_call": algorithm("closure_call", ["closure", "args"], ["Closure", "ListOfExpr"], "Stmt", true_formula(), operation_post("closure_call", ["Closure", "ListOfExpr"], "Stmt", "call a closure contract"), effect_rule("closure body effects")),
        "cast": algorithm("cast", ["value", "target_sort"], ["Expr", "Sort"], "Expr", true_formula(), operation_post("cast", ["Expr", "Sort"], "Expr", "Rust as cast, including truncating conversions")),
        "move": algorithm("move", ["place"], ["Place"], "Expr", live_place, operation_post("move", ["Place"], "Expr", "read value and invalidate source place"), effect_set("MemRead")),
        "div": algorithm("div", ["lhs", "rhs"], ["Int", "Int"], "Int", nonzero, operation_post("div", ["Int", "Int"], "Int", "integer division, divisor zero panics"), effect_set("Panic")),
        "rem": algorithm("rem", ["lhs", "rhs"], ["Int", "Int"], "Int", nonzero, operation_post("rem", ["Int", "Int"], "Int", "integer remainder, divisor zero panics"), effect_set("Panic")),
        "ne": algorithm("ne", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("ne", ["Int", "Int"], "Bool", "integer disequality comparison")),
        "gt": algorithm("gt", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("gt", ["Int", "Int"], "Bool", "integer greater than comparison")),
        "ge": algorithm("ge", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("ge", ["Int", "Int"], "Bool", "integer greater than or equal comparison")),
        "bit_and": algorithm("bit_and", ["lhs", "rhs"], ["Int", "Int"], "Int", true_formula(), operation_post("bit_and", ["Int", "Int"], "Int", "integer bitwise and")),
        "bit_or": algorithm("bit_or", ["lhs", "rhs"], ["Int", "Int"], "Int", true_formula(), operation_post("bit_or", ["Int", "Int"], "Int", "integer bitwise or")),
        "bit_xor": algorithm("bit_xor", ["lhs", "rhs"], ["Int", "Int"], "Int", true_formula(), operation_post("bit_xor", ["Int", "Int"], "Int", "integer bitwise xor")),
        "shl": algorithm("shl", ["lhs", "rhs"], ["Int", "Int"], "Int", true_formula(), operation_post("shl", ["Int", "Int"], "Int", "integer shift left"), effect_set("Panic")),
        "shr": algorithm("shr", ["lhs", "rhs"], ["Int", "Int"], "Int", true_formula(), operation_post("shr", ["Int", "Int"], "Int", "integer shift right"), effect_set("Panic")),
        "bit_not": algorithm("bit_not", ["value"], ["Int"], "Int", true_formula(), operation_post("bit_not", ["Int"], "Int", "integer bitwise not")),
        "field": algorithm("field", ["base", "field"], ["Expr", "FieldName"], "Expr", true_formula(), operation_post("field", ["Expr", "FieldName"], "Expr", "field expression projection emitted by the walker")),
        "range": algorithm("range", ["start", "end"], ["Expr", "Expr"], "Expr", true_formula(), operation_post("range", ["Expr", "Expr"], "Expr", "half open range expression")),
        "range_incl": algorithm("range_incl", ["start", "end"], ["Expr", "Expr"], "Expr", true_formula(), operation_post("range_incl", ["Expr", "Expr"], "Expr", "closed range expression")),
        "tuple": algorithm("tuple", ["items"], ["ListOfExpr"], "Expr", true_formula(), operation_post("tuple", ["ListOfExpr"], "Expr", "tuple expression, walker variadic form is resolved to this op")),
        "array": algorithm("array", ["items"], ["ListOfExpr"], "Expr", true_formula(), operation_post("array", ["ListOfExpr"], "Expr", "array expression, walker variadic form is resolved to this op")),
        "array_repeat": algorithm("array_repeat", ["value", "count"], ["Expr", "Int"], "Expr", true_formula(), operation_post("array_repeat", ["Expr", "Int"], "Expr", "array repeat expression")),
        "len": algorithm("len", ["slice"], ["Slice"], "Int", true_formula(), operation_post("len", ["Slice"], "Int", "slice length metadata")),
        "ite": algorithm("ite", ["cond", "then_value", "else_value"], ["Bool", "Expr", "Expr"], "Expr", true_formula(), operation_post("ite", ["Bool", "Expr", "Expr"], "Expr", "expression conditional used by WP contracts"), effect_rule("union branch value effects")),
        "method_call": algorithm("method_call", ["receiver", "args"], ["Expr", "ListOfExpr"], "Expr", true_formula(), operation_post("method_call", ["Expr", "ListOfExpr"], "Expr", "method call expression, walker method:name ctors resolve here"), effect_set("UnresolvedCall")),
        "call_result": algorithm("call_result", ["callee", "args"], ["FnContract", "ListOfExpr"], "Expr", true_formula(), operation_post("call_result", ["FnContract", "ListOfExpr"], "Expr", "call result expression, walker call:name ctors resolve here"), effect_set("UnresolvedCall")),
        "loop": algorithm("loop", ["body"], ["Stmt"], "Stmt", true_formula(), operation_post("loop", ["Stmt"], "Stmt", "unconditional Rust loop requiring an invariant"), effect_set("OpaqueLoop")),
        "let": algorithm("let", ["pattern", "value", "body"], ["Pattern", "Expr", "Stmt"], "Stmt", true_formula(), operation_post("let", ["Pattern", "Expr", "Stmt"], "Stmt", "bind pattern then continue body")),
        "into_iter": algorithm("into_iter", ["value"], ["Expr"], "Expr", true_formula(), operation_post("into_iter", ["Expr"], "Expr", "desugar for Rust for loops")),
        "next": algorithm("next", ["iterator"], ["Expr"], "Option", true_formula(), operation_post("next", ["Expr"], "Option", "iterator next result")),
        "arm": algorithm("arm", ["pattern", "body"], ["Pattern", "Stmt"], "MatchArm", true_formula(), operation_post("arm", ["Pattern", "Stmt"], "MatchArm", "unguarded match arm")),
        "guarded_arm": algorithm("guarded_arm", ["pattern", "guard", "body"], ["Pattern", "Bool", "Stmt"], "MatchArm", true_formula(), operation_post("guarded_arm", ["Pattern", "Bool", "Stmt"], "MatchArm", "guarded match arm")),
        "arms": algorithm("arms", ["head", "tail"], ["MatchArm", "ListOfArm"], "ListOfArm", true_formula(), operation_post("arms", ["MatchArm", "ListOfArm"], "ListOfArm", "match arm list constructor")),
        "if_let": algorithm("if_let", ["pattern", "expr", "then_branch", "else_branch"], ["Pattern", "Expr", "Stmt", "Stmt"], "Stmt", true_formula(), operation_post("if_let", ["Pattern", "Expr", "Stmt", "Stmt"], "Stmt", "if let surface form, desugars to match")),
        "while_let": algorithm("while_let", ["pattern", "expr", "body"], ["Pattern", "Expr", "Stmt"], "Stmt", true_formula(), operation_post("while_let", ["Pattern", "Expr", "Stmt"], "Stmt", "while let surface form, desugars to loop and match")),
        "pattern_ok": algorithm("pattern_ok", ["inner"], ["Pattern"], "Pattern", true_formula(), operation_post("pattern_ok", ["Pattern"], "Pattern", "Ok pattern")),
        "pattern_err": algorithm("pattern_err", ["inner"], ["Pattern"], "Pattern", true_formula(), operation_post("pattern_err", ["Pattern"], "Pattern", "Err pattern")),
        "pattern_some": algorithm("pattern_some", ["inner"], ["Pattern"], "Pattern", true_formula(), operation_post("pattern_some", ["Pattern"], "Pattern", "Some pattern")),
        "pattern_none": algorithm("pattern_none", ["unit"], ["Unit"], "Pattern", true_formula(), operation_post("pattern_none", ["Unit"], "Pattern", "None pattern")),
        "pattern_wild": algorithm("pattern_wild", ["unit"], ["Unit"], "Pattern", true_formula(), operation_post("pattern_wild", ["Unit"], "Pattern", "wildcard pattern")),
        "pattern_bind": algorithm("pattern_bind", ["name"], ["Value"], "Pattern", true_formula(), operation_post("pattern_bind", ["Value"], "Pattern", "binding pattern")),
    }
    for key, spec in operations.items():
        write_json(SPECS / f"op_{key}.spec.json", spec)

    skip = op("skip", [unit_term()])
    equations = {
        "seq_assoc": equation("seq-assoc", ["a", "b", "c"], ["stmt", "stmt", "stmt"], op("seq", [op("seq", [var("a"), var("b")]), var("c")]), op("seq", [var("a"), op("seq", [var("b"), var("c")])])),
        "seq_skip_left": equation("seq-skip-left", ["a"], ["stmt"], op("seq", [skip, var("a")]), var("a")),
        "seq_skip_right": equation("seq-skip-right", ["a"], ["stmt"], op("seq", [var("a"), skip]), var("a")),
        "if_true": equation("if-true", ["a", "b"], ["stmt", "stmt"], op("if", [const_bool(True), var("a"), var("b")]), var("a")),
        "if_false": equation("if-false", ["a", "b"], ["stmt", "stmt"], op("if", [const_bool(False), var("a"), var("b")]), var("b")),
        "if_idemp": equation("if-idemp", ["p", "a"], ["bool", "stmt"], op("if", [var("p"), var("a"), var("a")]), var("a")),
        "while_false": equation("while-false", ["a"], ["stmt"], op("while", [const_bool(False), var("a")]), skip),
        "and_false": equation("and-false", ["b"], ["bool"], op("and", [const_bool(False), var("b")]), const_bool(False)),
        "and_true": equation("and-true", ["b"], ["bool"], op("and", [const_bool(True), var("b")]), var("b")),
        "or_true": equation("or-true", ["b"], ["bool"], op("or", [const_bool(True), var("b")]), const_bool(True)),
        "or_false": equation("or-false", ["b"], ["bool"], op("or", [const_bool(False), var("b")]), var("b")),
        "not_not": equation("not-not", ["p"], ["bool"], op("not", [op("not", [var("p")])]), var("p")),
        "question_desugar": equation("question-desugar", ["e", "v", "err"], ["result", "pattern", "pattern"], op("try", [var("e")]), op("match_expr", [var("e"), op("arms", [op("arm", [op("pattern_ok", [var("v")]), var("v")]), op("arm", [op("pattern_err", [var("err")]), op("return", [op("call_result", [var("into_contract"), var("err")])])])])]), notes="Uses match_expr because try returns a payload expression. The statement match operation remains primitive."),
        "for_desugar": equation("for-desugar", ["pat", "iter", "body"], ["pattern", "expr", "stmt"], op("for", [op("let", [var("pat"), var("iter"), skip]), const_bool(True), skip, var("body")]), op("seq", [op("let", [var("it"), op("into_iter", [var("iter")]), skip]), op("loop", [op("match", [op("next", [var("it")]), op("arms", [op("arm", [op("pattern_some", [var("pat")]), op("seq", [var("body"), op("continue", [unit_term()])])]), op("arm", [op("pattern_none", [unit_term()]), op("break", [unit_term()])])])])])])),
        "if_let_desugar": equation("if-let-desugar", ["p", "e", "a", "b"], ["pattern", "expr", "stmt", "stmt"], op("if_let", [var("p"), var("e"), var("a"), var("b")]), op("match", [var("e"), op("arms", [op("arm", [var("p"), var("a")]), op("arm", [op("pattern_wild", [unit_term()]), var("b")])])])),
        "while_let_desugar": equation("while-let-desugar", ["p", "e", "body"], ["pattern", "expr", "stmt"], op("while_let", [var("p"), var("e"), var("body")]), op("loop", [op("match", [var("e"), op("arms", [op("arm", [var("p"), op("seq", [var("body"), op("continue", [unit_term()])])]), op("arm", [op("pattern_wild", [unit_term()]), op("break", [unit_term()])])])])])),
        "drop_skip": equation("drop-skip", ["p"], ["place"], op("drop", [var("p")]), skip, pre={"kind": "atomic", "name": "has_no_drop_glue", "args": [var("p")]}),
        "deref_borrow": equation("deref-borrow", ["p"], ["place"], op("deref", [op("borrow", [var("p")])]), var("p"), notes="Modulo aliasing side conditions."),
        "deref_borrow_mut": equation("deref-borrow-mut", ["p"], ["place"], op("deref", [op("borrow_mut", [var("p")])]), var("p"), notes="Modulo aliasing side conditions."),
    }
    for key, spec in equations.items():
        write_json(SPECS / f"eq_{key}.spec.json", spec)

    effect_ops = {
        "read": algorithm("effect:read", ["addr"], ["Addr"], "Value", true_formula(), operation_post("read", ["Addr"], "Value", "read memory value at addr"), effect_set("MemRead")),
        "write": algorithm("effect:write", ["addr", "value"], ["Addr", "Value"], "Unit", true_formula(), operation_post("write", ["Addr", "Value"], "Unit", "write value to memory addr"), effect_set("MemWrite")),
        "input": algorithm("effect:input", ["unit"], ["Unit"], "Value", true_formula(), operation_post("input", ["Unit"], "Value", "read one external input value"), effect_set("IO")),
        "output": algorithm("effect:output", ["value"], ["Value"], "Unit", true_formula(), operation_post("output", ["Value"], "Unit", "emit one external output value"), effect_set("IO")),
        "panic": algorithm("effect:panic", ["reason"], ["Reason"], "Bottom", true_formula(), operation_post("panic", ["Reason"], "Bottom", "controlled divergence through panic"), effect_set("Panic")),
        "alloc": algorithm("effect:alloc", ["unit"], ["Unit"], "Addr", true_formula(), operation_post("alloc", ["Unit"], "Addr", "allocate an address"), effect_set("Alloc")),
        "dealloc": algorithm("effect:dealloc", ["addr"], ["Addr"], "Unit", true_formula(), operation_post("dealloc", ["Addr"], "Unit", "deallocate an address"), effect_set("Alloc")),
        "unsafe_marker": algorithm("effect:unsafe_marker", ["unit"], ["Unit"], "Unit", true_formula(), operation_post("unsafe_marker", ["Unit"], "Unit", "unsafe code marker"), effect_set("Unsafe")),
        "suspend": algorithm("effect:suspend", ["unit"], ["Unit"], "Unit", true_formula(), operation_post("suspend", ["Unit"], "Unit", "async suspension marker"), effect_set("Async")),
        "drop_glue": algorithm("effect:drop_glue", ["addr"], ["Addr"], "Unit", true_formula(), operation_post("drop_glue", ["Addr"], "Unit", "run drop glue for address"), effect_set("Drop")),
        "unresolved_call": algorithm("effect:unresolved_call", ["name"], ["Value"], "Unit", true_formula(), operation_post("unresolved_call", ["Value"], "Unit", "call with unknown contract"), effect_set("UnresolvedCall")),
        "opaque_loop": algorithm("effect:opaque_loop", ["loop_cid"], ["Value"], "Unit", true_formula(), operation_post("opaque_loop", ["Value"], "Unit", "loop awaiting invariant memento"), effect_set("OpaqueLoop")),
        "early_return": algorithm("effect:early_return", ["try_cid"], ["Value"], "Unit", true_formula(), operation_post("early_return", ["Value"], "Unit", "try branch awaiting residual memento"), effect_set("EarlyReturn")),
        "closure_capture": algorithm("effect:closure_capture", ["body_fn_cid"], ["Value"], "Unit", true_formula(), operation_post("closure_capture", ["Value"], "Unit", "closure capture awaiting body contract"), effect_set("ClosureCapture")),
        "pin_invariant": algorithm("effect:pin_invariant", ["target"], ["Value"], "Unit", true_formula(), operation_post("pin_invariant", ["Value"], "Unit", "pinned reference invariant obligation"), effect_set("PinnedReference")),
        "raw_ptr_provenance": algorithm("effect:raw_ptr_provenance", ["target"], ["Value"], "Unit", true_formula(), operation_post("raw_ptr_provenance", ["Value"], "Unit", "raw pointer provenance obligation"), effect_set("RawPointerProvenance")),
        "atomic_load": algorithm("effect:atomic_load", ["addr"], ["Addr"], "Value", true_formula(), operation_post("atomic_load", ["Addr"], "Value", "atomic load"), effect_set("AtomicAccess")),
        "atomic_store": algorithm("effect:atomic_store", ["addr", "value"], ["Addr", "Value"], "Unit", true_formula(), operation_post("atomic_store", ["Addr", "Value"], "Unit", "atomic store"), effect_set("AtomicAccess")),
        "atomic_rmw": algorithm("effect:atomic_rmw", ["addr", "value"], ["Addr", "Value"], "Value", true_formula(), operation_post("atomic_rmw", ["Addr", "Value"], "Value", "atomic read modify write"), effect_set("AtomicAccess")),
        "atomic_cas": algorithm("effect:atomic_cas", ["addr", "old", "new"], ["Addr", "Value", "Value"], "Value", true_formula(), operation_post("atomic_cas", ["Addr", "Value", "Value"], "Value", "atomic compare and swap"), effect_set("AtomicAccess")),
        "possible_aliasing": algorithm("effect:possible_aliasing", ["formals"], ["ListOfExpr"], "Unit", true_formula(), operation_post("possible_aliasing", ["ListOfExpr"], "Unit", "shared references may alias through interior mutability"), effect_set("PossibleAliasing")),
    }
    for key, spec in effect_ops.items():
        write_json(SPECS / f"eff_op_{key}.spec.json", spec)

    read_after_write = {
        "kind": "equation",
        "fn_name": "rust:effect:read-after-write",
        "formals": ["addr", "v"],
        "formal_sorts": ["sort_addr.spec.json", "sort_value.spec.json"],
        "pre": true_formula(),
        "post": {"kind": "equation", "lhs": op("after", [op("read", [var("addr")]), op("write", [var("addr"), var("v")])]), "rhs": var("v")},
    }
    write_json(SPECS / "eff_eq_read_after_write.spec.json", read_after_write)

    effect_sigs = {
        "memread": ("MemRead", ["sort_addr.spec.json", "sort_value.spec.json"], ["eff_op_read.spec.json"], []),
        "memwrite": ("MemWrite", ["sort_addr.spec.json", "sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_write.spec.json"], ["eff_eq_read_after_write.spec.json"]),
        "io": ("IO", ["sort_unit.spec.json", "sort_value.spec.json"], ["eff_op_input.spec.json", "eff_op_output.spec.json"], []),
        "panic": ("Panic", ["sort_reason.spec.json", "sort_bottom.spec.json"], ["eff_op_panic.spec.json"], []),
        "alloc": ("Alloc", ["sort_unit.spec.json", "sort_addr.spec.json"], ["eff_op_alloc.spec.json", "eff_op_dealloc.spec.json"], []),
        "unsafe": ("Unsafe", ["sort_unit.spec.json"], ["eff_op_unsafe_marker.spec.json"], []),
        "async": ("Async", ["sort_unit.spec.json"], ["eff_op_suspend.spec.json"], []),
        "drop": ("Drop", ["sort_addr.spec.json", "sort_unit.spec.json"], ["eff_op_drop_glue.spec.json"], []),
        "unresolvedcall": ("UnresolvedCall", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_unresolved_call.spec.json"], []),
        "opaqueloop": ("OpaqueLoop", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_opaque_loop.spec.json"], []),
        "earlyreturn": ("EarlyReturn", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_early_return.spec.json"], []),
        "closurecapture": ("ClosureCapture", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_closure_capture.spec.json"], []),
        "pinnedreference": ("PinnedReference", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_pin_invariant.spec.json"], []),
        "rawpointerprovenance": ("RawPointerProvenance", ["sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_raw_ptr_provenance.spec.json"], []),
        "atomicaccess": ("AtomicAccess", ["sort_addr.spec.json", "sort_value.spec.json", "sort_unit.spec.json"], ["eff_op_atomic_load.spec.json", "eff_op_atomic_store.spec.json", "eff_op_atomic_rmw.spec.json", "eff_op_atomic_cas.spec.json"], []),
        "possiblealiasing": ("PossibleAliasing", ["sort_listofexpr.spec.json", "sort_unit.spec.json"], ["eff_op_possible_aliasing.spec.json"], []),
    }
    for key, (name, eff_sorts, ops, eqs) in effect_sigs.items():
        write_json(SPECS / f"effsig_{key}.spec.json", {
            "kind": "effect_signature",
            "fn_name": f"rust:effect-signature:{name}",
            "sorts": eff_sorts,
            "operations": ops,
            "equations": eqs,
            "effect_signatures": [],
        })

    sort_files = [f"sort_{key}.spec.json" for key in sorts.keys()]
    op_files = [f"op_{key}.spec.json" for key in operations.keys()]
    eq_files = [f"eq_{key}.spec.json" for key in equations.keys()]
    eff_op_files = [f"eff_op_{key}.spec.json" for key in effect_ops.keys()]
    effsig_files = [f"effsig_{key}.spec.json" for key in effect_sigs.keys()]

    write_json(SPECS / "language_signature_rust.spec.json", {
        "kind": "language_signature",
        "fn_name": "rust:rust",
        "sorts": sort_files,
        "operations": op_files,
        "equations": eq_files,
        "effect_signatures": effsig_files,
    })

    (EXAMPLE / "foo.rs").write_text("fn foo(x: i32) -> i32 { if x == 0 { -22 } else { x } }\n", encoding="utf-8")


if __name__ == "__main__":
    main()
