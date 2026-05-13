#!/usr/bin/env python3
import json
from pathlib import Path


BASE = Path(__file__).resolve().parent
SPECS = BASE / "specs"
EXAMPLE = BASE / "example"
MAUDE = BASE / "maude"


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
        "wp_note": wp,
    }
    if notes:
        post["notes"] = notes
    return post


def algorithm(name, formals, formal_sorts, result, pre, post, effects=None):
    return {
        "kind": "algorithm",
        "fn_name": f"c11:{name}",
        "formals": formals,
        "formal_sorts": [sort_ref(s) for s in formal_sorts],
        "return_sort": sort_ref(result),
        "pre": pre,
        "post": post,
        "effects": effects or {"effects": []},
        "locus": "menagerie/c11-language-signature/README.md#operations",
    }


def equation(name, formals, formal_sorts, lhs, rhs, pre=None):
    return {
        "kind": "equation",
        "fn_name": f"c11:{name}",
        "formals": formals,
        "formal_sorts": [f"sort_{s.lower()}.spec.json" for s in formal_sorts],
        "pre": pre or true_formula(),
        "post": {"kind": "equation", "lhs": lhs, "rhs": rhs},
    }


def maude_term(name, args=None):
    return {"kind": "ctor", "name": name, "args": args or []}


def maude_var(name):
    return {"kind": "var", "name": name}


def c11_maude_theory(obligation):
    return {
        "kind": "atomic",
        "name": "equational_theory",
        "theory": {
            "name": "c11-core-flow",
            "sorts": ["Stmt", "Bool"],
            "operators": [
                {"name": "skip", "arity": [], "result": "Stmt"},
                {"name": "seq", "arity": ["Stmt", "Stmt"], "result": "Stmt", "attrs": ["assoc"]},
                {"name": "ifc", "arity": ["Bool", "Stmt", "Stmt"], "result": "Stmt"},
                {"name": "whilec", "arity": ["Bool", "Stmt"], "result": "Stmt"},
                {"name": "forc", "arity": ["Stmt", "Bool", "Stmt", "Stmt"], "result": "Stmt"},
                {"name": "trueb", "arity": [], "result": "Bool"},
                {"name": "falseb", "arity": [], "result": "Bool"},
                {"name": "andb", "arity": ["Bool", "Bool"], "result": "Bool"},
                {"name": "orb", "arity": ["Bool", "Bool"], "result": "Bool"},
                {"name": "notb", "arity": ["Bool"], "result": "Bool"},
            ],
            "variables": [
                {"name": "A", "sort": "Stmt"},
                {"name": "B", "sort": "Stmt"},
                {"name": "C", "sort": "Stmt"},
                {"name": "Init", "sort": "Stmt"},
                {"name": "Step", "sort": "Stmt"},
                {"name": "Body", "sort": "Stmt"},
                {"name": "P", "sort": "Bool"},
                {"name": "Q", "sort": "Bool"},
            ],
            "equations": [
                {"label": "seq-skip-left", "lhs": maude_term("seq", [maude_term("skip"), maude_var("A")]), "rhs": maude_var("A")},
                {"label": "seq-skip-right", "lhs": maude_term("seq", [maude_var("A"), maude_term("skip")]), "rhs": maude_var("A")},
                {"label": "if-true", "lhs": maude_term("ifc", [maude_term("trueb"), maude_var("A"), maude_var("B")]), "rhs": maude_var("A")},
                {"label": "if-false", "lhs": maude_term("ifc", [maude_term("falseb"), maude_var("A"), maude_var("B")]), "rhs": maude_var("B")},
                {"label": "if-idemp", "lhs": maude_term("ifc", [maude_var("P"), maude_var("A"), maude_var("A")]), "rhs": maude_var("A")},
                {"label": "while-false", "lhs": maude_term("whilec", [maude_term("falseb"), maude_var("A")]), "rhs": maude_term("skip")},
                {"label": "for-desugar", "lhs": maude_term("forc", [maude_var("Init"), maude_var("P"), maude_var("Step"), maude_var("Body")]), "rhs": maude_term("seq", [maude_var("Init"), maude_term("whilec", [maude_var("P"), maude_term("seq", [maude_var("Body"), maude_var("Step")])])])},
                {"label": "and-false", "lhs": maude_term("andb", [maude_term("falseb"), maude_var("Q")]), "rhs": maude_term("falseb")},
                {"label": "and-true", "lhs": maude_term("andb", [maude_term("trueb"), maude_var("Q")]), "rhs": maude_var("Q")},
                {"label": "or-true", "lhs": maude_term("orb", [maude_term("trueb"), maude_var("Q")]), "rhs": maude_term("trueb")},
                {"label": "or-false", "lhs": maude_term("orb", [maude_term("falseb"), maude_var("Q")]), "rhs": maude_var("Q")},
                {"label": "not-not", "lhs": maude_term("notb", [maude_term("notb", [maude_var("P")])]), "rhs": maude_var("P")},
            ],
        },
        "obligation": obligation,
    }


def main():
    SPECS.mkdir(parents=True, exist_ok=True)
    EXAMPLE.mkdir(parents=True, exist_ok=True)
    MAUDE.mkdir(parents=True, exist_ok=True)

    sorts = {
        "stmt": ("Stmt", "C11 statement terms, the carrier sort for statement-level algebra terms"),
        "expr": ("Expr", "C11 expression terms"),
        "lvalue": ("LValue", "Assignable C11 storage designators"),
        "int": ("Int", "C11 signed integer values with undefined signed overflow"),
        "ptr": ("Ptr", "C11 pointer values with null and provenance constraints"),
        "bool": ("Bool", "C11 branch truth values"),
        "unit": ("Unit", "The unit input and output sort"),
        "fncontract": ("FnContract", "Function contract values used at call sites"),
        "fieldname": ("FieldName", "Struct or union field names"),
        "listofstmt": ("ListOfStmt", "Finite ordered statement lists"),
        "listofexpr": ("ListOfExpr", "Finite ordered expression lists"),
        "addr": ("Addr", "Abstract memory addresses for effect signatures"),
        "value": ("Value", "Abstract memory or IO values for effect signatures"),
        "reason": ("Reason", "Trap reasons for undefined behavior"),
        "bottom": ("Bottom", "Divergence or unreachable result sort"),
    }
    for key, (name, description) in sorts.items():
        write_json(SPECS / f"sort_{key}.spec.json", {
            "kind": "sort",
            "fn_name": f"c11:{name}",
            "formals": [],
            "return_sort": {"kind": "kind", "name": "*"},
            "post": {"kind": "sort-description", "name": name, "description": description},
        })

    operations = {
        "skip": algorithm("skip", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("skip", ["Unit"], "Stmt", "state unchanged"), {"effects": []}),
        "seq": algorithm("seq", ["first", "second"], ["Stmt", "Stmt"], "Stmt", true_formula(), operation_post("seq", ["Stmt", "Stmt"], "Stmt", "wp(first, wp(second, post))"), effect_rule("union(first.effects, second.effects)")),
        "if": algorithm("if", ["cond", "then_branch", "else_branch"], ["Bool", "Stmt", "Stmt"], "Stmt", true_formula(), operation_post("if", ["Bool", "Stmt", "Stmt"], "Stmt", "cond ? wp(then_branch, post) : wp(else_branch, post)"), effect_rule("union(then_branch.effects, else_branch.effects)")),
        "while": algorithm("while", ["cond", "body"], ["Bool", "Stmt"], "Stmt", true_formula(), operation_post("while", ["Bool", "Stmt"], "Stmt", "loop invariant holds and cond is false at exit", "Requires a loop invariant memento for nontrivial discharge"), effect_rule("body.effects plus loop invariant obligations")),
        "for": algorithm("for", ["init", "cond", "step", "body"], ["Stmt", "Bool", "Stmt", "Stmt"], "Stmt", true_formula(), operation_post("for", ["Stmt", "Bool", "Stmt", "Stmt"], "Stmt", "seq(init, while(cond, seq(body, step)))"), effect_rule("union(init.effects, body.effects, step.effects)")),
        "switch": algorithm("switch", ["scrutinee", "arms"], ["Int", "ListOfStmt"], "Stmt", true_formula(), operation_post("switch", ["Int", "ListOf<Stmt>"], "Stmt", "case-dispatched WP join over arms"), effect_rule("union(arm.effects for arm in arms)")),
        "call": algorithm("call", ["callee", "args"], ["FnContract", "ListOfExpr"], "Stmt", true_formula(), operation_post("call", ["FnContract", "ListOf<Expr>"], "Stmt", "callee pre under bound args implies callee post under caller state"), effect_rule("callee.effects")),
        "return": algorithm("return", ["value"], ["Expr"], "Stmt", true_formula(), operation_post("return", ["Expr"], "Stmt", "bind function out value and exit current body"), effect_rule("early-return")),
        "break": algorithm("break", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("break", ["Unit"], "Stmt", "exit nearest enclosing switch or loop"), effect_rule("control-transfer")),
        "continue": algorithm("continue", ["unit"], ["Unit"], "Stmt", true_formula(), operation_post("continue", ["Unit"], "Stmt", "jump to next loop iteration"), effect_rule("control-transfer")),
        "deref": algorithm("deref", ["ptr"], ["Ptr"], "LValue", {"kind": "connective", "op": "and", "operands": [{"kind": "atomic", "name": "!=", "args": [var("ptr"), {"kind": "const", "value": "NULL", "sort": sort_ref("Ptr")}]}, {"kind": "atomic", "name": "valid", "args": [var("ptr")]}]}, operation_post("deref", ["Ptr"], "LValue", "lvalue at ptr"), effect_set("MemRead")),
        "member": algorithm("member", ["base", "field"], ["LValue", "FieldName"], "LValue", true_formula(), operation_post("member", ["LValue", "FieldName"], "LValue", "field lvalue projection"), {"effects": []}),
        "add": algorithm("add", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow", "args": [op("add", [var("lhs"), var("rhs")])]}, operation_post("add", ["Int", "Int"], "Int", "mathematical integer addition when no overflow holds"), effect_set("Trap")),
        "sub": algorithm("sub", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow", "args": [op("sub", [var("lhs"), var("rhs")])]}, operation_post("sub", ["Int", "Int"], "Int", "mathematical integer subtraction when no overflow holds"), effect_set("Trap")),
        "mul": algorithm("mul", ["lhs", "rhs"], ["Int", "Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow", "args": [op("mul", [var("lhs"), var("rhs")])]}, operation_post("mul", ["Int", "Int"], "Int", "mathematical integer multiplication when no overflow holds"), effect_set("Trap")),
        "eq": algorithm("eq", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("eq", ["Int", "Int"], "Bool", "integer equality comparison"), {"effects": []}),
        "lt": algorithm("lt", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("lt", ["Int", "Int"], "Bool", "integer less-than comparison"), {"effects": []}),
        "le": algorithm("le", ["lhs", "rhs"], ["Int", "Int"], "Bool", true_formula(), operation_post("le", ["Int", "Int"], "Bool", "integer less-than-or-equal comparison"), {"effects": []}),
        "and": algorithm("and", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", true_formula(), operation_post("and", ["Bool", "Bool"], "Bool", "short-circuit conjunction"), effect_rule("rhs evaluated only when lhs is true")),
        "or": algorithm("or", ["lhs", "rhs"], ["Bool", "Bool"], "Bool", true_formula(), operation_post("or", ["Bool", "Bool"], "Bool", "short-circuit disjunction"), effect_rule("rhs evaluated only when lhs is false")),
        "not": algorithm("not", ["value"], ["Bool"], "Bool", true_formula(), operation_post("not", ["Bool"], "Bool", "boolean negation"), {"effects": []}),
        "assign": algorithm("assign", ["target", "value"], ["LValue", "Expr"], "Stmt", true_formula(), operation_post("assign", ["LValue", "Expr"], "Stmt", "store value into target and update state"), effect_set("MemWrite")),
        "neg": algorithm("neg", ["value"], ["Int"], "Int", {"kind": "atomic", "name": "no_signed_overflow", "args": [op("neg", [var("value")])]}, operation_post("neg", ["Int"], "Int", "integer arithmetic negation when no overflow holds"), effect_set("Trap")),
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
        "for_desugar": equation("for-desugar", ["init", "cond", "step", "body"], ["stmt", "bool", "stmt", "stmt"], op("for", [var("init"), var("cond"), var("step"), var("body")]), op("seq", [var("init"), op("while", [var("cond"), op("seq", [var("body"), var("step")])])])),
        "and_false": equation("and-false", ["b"], ["bool"], op("and", [const_bool(False), var("b")]), const_bool(False)),
        "and_true": equation("and-true", ["b"], ["bool"], op("and", [const_bool(True), var("b")]), var("b")),
        "or_true": equation("or-true", ["b"], ["bool"], op("or", [const_bool(True), var("b")]), const_bool(True)),
        "or_false": equation("or-false", ["b"], ["bool"], op("or", [const_bool(False), var("b")]), var("b")),
        "not_not": equation("not-not", ["p"], ["bool"], op("not", [op("not", [var("p")])]), var("p")),
    }
    for key, spec in equations.items():
        write_json(SPECS / f"eq_{key}.spec.json", spec)

    effect_ops = {
        "read": algorithm("effect:read", ["addr"], ["Addr"], "Value", true_formula(), operation_post("read", ["Addr"], "Value", "read memory value at addr"), effect_set("MemRead")),
        "write": algorithm("effect:write", ["addr", "value"], ["Addr", "Value"], "Unit", true_formula(), operation_post("write", ["Addr", "Value"], "Unit", "write value to memory addr"), effect_set("MemWrite")),
        "input": algorithm("effect:input", ["unit"], ["Unit"], "Value", true_formula(), operation_post("input", ["Unit"], "Value", "read one external input value"), effect_set("IO")),
        "output": algorithm("effect:output", ["value"], ["Value"], "Unit", true_formula(), operation_post("output", ["Value"], "Unit", "emit one external output value"), effect_set("IO")),
        "trap": algorithm("effect:trap", ["reason"], ["Reason"], "Bottom", true_formula(), operation_post("trap", ["Reason"], "Bottom", "diverge due to undefined behavior or trap"), effect_set("Trap")),
    }
    for key, spec in effect_ops.items():
        write_json(SPECS / f"eff_op_{key}.spec.json", spec)

    read_after_write = {
        "kind": "equation",
        "fn_name": "c11:effect:read-after-write",
        "formals": ["addr", "v"],
        "formal_sorts": ["sort_addr.spec.json", "sort_value.spec.json"],
        "pre": true_formula(),
        "post": {
            "kind": "equation",
            "lhs": op("after", [op("read", [var("addr")]), op("write", [var("addr"), var("v")])]),
            "rhs": var("v"),
        },
    }
    write_json(SPECS / "eff_eq_read_after_write.spec.json", read_after_write)

    effect_sigs = {
        "memread": {
            "kind": "effect_signature",
            "fn_name": "c11:effect-signature:MemRead",
            "sorts": ["sort_addr.spec.json", "sort_value.spec.json"],
            "operations": ["eff_op_read.spec.json"],
            "equations": [],
            "effect_signatures": [],
        },
        "memwrite": {
            "kind": "effect_signature",
            "fn_name": "c11:effect-signature:MemWrite",
            "sorts": ["sort_addr.spec.json", "sort_value.spec.json", "sort_unit.spec.json"],
            "operations": ["eff_op_write.spec.json"],
            "equations": ["eff_eq_read_after_write.spec.json"],
            "effect_signatures": [],
        },
        "io": {
            "kind": "effect_signature",
            "fn_name": "c11:effect-signature:IO",
            "sorts": ["sort_value.spec.json", "sort_unit.spec.json"],
            "operations": ["eff_op_input.spec.json", "eff_op_output.spec.json"],
            "equations": [],
            "effect_signatures": [],
        },
        "trap": {
            "kind": "effect_signature",
            "fn_name": "c11:effect-signature:Trap",
            "sorts": ["sort_reason.spec.json", "sort_bottom.spec.json"],
            "operations": ["eff_op_trap.spec.json"],
            "equations": [],
            "effect_signatures": [],
        },
    }
    for key, spec in effect_sigs.items():
        write_json(SPECS / f"effsig_{key}.spec.json", spec)

    write_json(SPECS / "language_signature_c11.spec.json", {
        "kind": "language_signature",
        "fn_name": "c:c11",
        "sorts": [
            "sort_stmt.spec.json",
            "sort_expr.spec.json",
            "sort_lvalue.spec.json",
            "sort_int.spec.json",
            "sort_ptr.spec.json",
            "sort_bool.spec.json",
            "sort_unit.spec.json",
            "sort_fncontract.spec.json",
            "sort_fieldname.spec.json",
            "sort_listofstmt.spec.json",
            "sort_listofexpr.spec.json",
        ],
        "operations": [f"op_{name}.spec.json" for name in operations],
        "equations": [f"eq_{name}.spec.json" for name in equations],
        "effect_signatures": [
            "effsig_memread.spec.json",
            "effsig_memwrite.spec.json",
            "effsig_io.spec.json",
            "effsig_trap.spec.json",
        ],
    })

    seq_obligation = {
        "lhs": maude_term("seq", [maude_term("seq", [maude_var("A"), maude_var("B")]), maude_var("C")]),
        "rhs": maude_term("seq", [maude_var("A"), maude_term("seq", [maude_var("B"), maude_var("C")])]),
    }
    if_obligation = {
        "lhs": maude_term("ifc", [maude_var("P"), maude_var("A"), maude_var("A")]),
        "rhs": maude_var("A"),
    }
    write_json(MAUDE / "seq_assoc.ir.json", c11_maude_theory(seq_obligation))
    write_json(MAUDE / "if_idemp.ir.json", c11_maude_theory(if_obligation))

    (EXAMPLE / "foo.c").write_text(
        "static int foo(int x) {\n"
        "    if (x == 0)\n"
        "        return -22;\n"
        "    return x;\n"
        "}\n",
        encoding="utf-8",
    )

    (BASE / "mint.sh").write_text(
        "#!/bin/sh\n"
        "set -eu\n"
        "BASE=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\"\n"
        "ROOT=\"$(CDPATH= cd -- \"$BASE/../..\" && pwd)\"\n"
        "RUST_DIR=\"$ROOT/implementations/rust\"\n"
        "PROVEKIT=\"$RUST_DIR/target/debug/provekit\"\n"
        "SPEC_DIR=\"$BASE/specs\"\n"
        "CATALOG_REAL=\"$BASE/catalog\"\n"
        "CATALOG_ARG=\"$BASE/dev/../catalog\"\n"
        "CID_FILE=\"$BASE/cids.tsv\"\n"
        "mkdir -p \"$BASE/dev\"\n"
        "rm -rf \"$CATALOG_REAL\"\n"
        "cargo build --manifest-path \"$RUST_DIR/Cargo.toml\" -p provekit-cli -p provekit-ir-compiler-maude\n"
        ": > \"$CID_FILE\"\n"
        "mint_one() {\n"
        "  kind=\"$1\"\n"
        "  spec=\"$2\"\n"
        "  out=$(\"$PROVEKIT\" mint \"$kind\" --spec \"$SPEC_DIR/$spec\" --unsigned --catalog \"$CATALOG_ARG\")\n"
        "  printf '%s\\t%s\\t%s\\n' \"$kind\" \"$spec\" \"$out\" | tee -a \"$CID_FILE\"\n"
        "}\n"
        "for spec in sort_stmt.spec.json sort_expr.spec.json sort_lvalue.spec.json sort_int.spec.json sort_ptr.spec.json sort_bool.spec.json sort_unit.spec.json sort_fncontract.spec.json sort_fieldname.spec.json sort_listofstmt.spec.json sort_listofexpr.spec.json sort_addr.spec.json sort_value.spec.json sort_reason.spec.json sort_bottom.spec.json; do\n"
        "  mint_one sort \"$spec\"\n"
        "done\n"
        "for spec in op_skip.spec.json op_seq.spec.json op_if.spec.json op_while.spec.json op_for.spec.json op_switch.spec.json op_call.spec.json op_return.spec.json op_break.spec.json op_continue.spec.json op_deref.spec.json op_member.spec.json op_add.spec.json op_sub.spec.json op_mul.spec.json op_eq.spec.json op_lt.spec.json op_le.spec.json op_and.spec.json op_or.spec.json op_not.spec.json op_assign.spec.json op_neg.spec.json eff_op_read.spec.json eff_op_write.spec.json eff_op_input.spec.json eff_op_output.spec.json eff_op_trap.spec.json; do\n"
        "  mint_one algorithm \"$spec\"\n"
        "done\n"
        "for spec in eq_seq_assoc.spec.json eq_seq_skip_left.spec.json eq_seq_skip_right.spec.json eq_if_true.spec.json eq_if_false.spec.json eq_if_idemp.spec.json eq_while_false.spec.json eq_for_desugar.spec.json eq_and_false.spec.json eq_and_true.spec.json eq_or_true.spec.json eq_or_false.spec.json eq_not_not.spec.json eff_eq_read_after_write.spec.json; do\n"
        "  mint_one equation \"$spec\"\n"
        "done\n"
        "for spec in effsig_memread.spec.json effsig_memwrite.spec.json effsig_io.spec.json effsig_trap.spec.json; do\n"
        "  mint_one effect-signature \"$spec\"\n"
        "done\n"
        "mint_one language-signature language_signature_c11.spec.json\n",
        encoding="utf-8",
    )
    (BASE / "mint.sh").chmod(0o755)


if __name__ == "__main__":
    main()
