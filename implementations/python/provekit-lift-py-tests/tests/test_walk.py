# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import textwrap

from provekit_lift_py_tests.ir import _Atomic, _Connective, _ConstInt, _Ctor, _Var
from provekit_lift_py_tests.lsp import _lift_source
from provekit_lift_py_tests.walk import lift_production_walk


def _lift(src: str):
    return lift_production_walk(textwrap.dedent(src), "app.py")


def _edge(out, suffix: str):
    matches = [d for d in out.decls if d.name.endswith(suffix)]
    assert len(matches) == 1, f"expected one {suffix} edge, got {[d.name for d in out.decls]}"
    return matches[0]


def _flatten_and(formula):
    if isinstance(formula, _Connective) and formula.kind == "and":
        out = []
        for operand in formula.operands:
            out.extend(_flatten_and(operand))
        return out
    return [formula]


def _assert_none_guard_formula(formula, *, comparison_name: str, guard_name: str):
    atoms = [atom for atom in _flatten_and(formula) if isinstance(atom, _Atomic)]
    assert any(
        atom.name == comparison_name
        and len(atom.args) == 2
        and isinstance(atom.args[1], _Ctor)
        and atom.args[1].name == "None"
        for atom in atoms
    )
    guards = [atom for atom in atoms if atom.name == guard_name]
    assert len(guards) == 1
    assert ":" not in guards[0].name
    assert len(guards[0].args) == 1


def test_walk_substitutes_assignment_back_to_function_entry():
    out = _lift(
        """
        def f(x):
            if x < 10:
                raise ValueError("x must be >= 10")
            return x * 2

        def caller():
            y = 42
            result = f(y)
            return result
        """
    )

    assert len(out.implications) == 3, f"warnings: {out.warnings}"
    assert all(imp.prover == "python-wp-walk" for imp in out.implications)
    assert all(imp.antecedent_slot == "pre" for imp in out.implications)
    assert all(imp.consequent_slot == "post" for imp in out.implications)

    let_edge = _edge(out, "::let:y")
    assert let_edge.name.startswith("f@app.py:")

    pre = let_edge.pre
    post = let_edge.post
    assert isinstance(pre, _Atomic)
    assert pre.name == "≥"
    assert isinstance(pre.args[0], _ConstInt)
    assert pre.args[0].value == 42
    assert isinstance(pre.args[1], _ConstInt)
    assert pre.args[1].value == 10

    assert isinstance(post, _Atomic)
    assert post.name == "≥"
    assert isinstance(post.args[0], _Var)
    assert post.args[0].name == "y"
    assert isinstance(post.args[1], _ConstInt)
    assert post.args[1].value == 10

    entry_edge = _edge(out, "::entry")
    assert entry_edge.pre == pre


def test_walk_none_precondition_emits_substrate_guard_fact():
    out = _lift(
        """
        def f(x):
            assert x is not None
            return x

        def caller(value):
            return f(value)
        """
    )

    entry_edge = _edge(out, "::entry")
    _assert_none_guard_formula(
        entry_edge.pre,
        comparison_name="≠",
        guard_name="is_some",
    )


def test_walk_non_none_identity_precondition_is_not_lifted_as_value_equality():
    out = _lift(
        """
        def f(x, y):
            assert x is y
            return x

        def caller(left, right):
            return f(left, right)
        """
    )

    assert out.decls == []
    assert out.implications == []


def test_walk_eq_none_precondition_does_not_emit_substrate_guard_fact():
    out = _lift(
        """
        def f(x):
            assert x == None
            return x

        def caller(value):
            return f(value)
        """
    )

    entry_edge = _edge(out, "::entry")
    atoms = [atom for atom in _flatten_and(entry_edge.pre) if isinstance(atom, _Atomic)]
    assert [atom.name for atom in atoms] == ["="]
    assert all(atom.name not in {"is_none", "is_some"} for atom in atoms)


def test_walk_if_guard_becomes_callsite_premise():
    out = _lift(
        """
        def f(x):
            if x < 10:
                raise ValueError("x must be >= 10")
            return x

        def guarded_caller(input):
            if input >= 10:
                return f(input)
            return 0
        """
    )

    callsite = _edge(out, "::callsite")
    assert isinstance(callsite.pre, _Connective)
    assert callsite.pre.kind == "implies"
    assert len(callsite.pre.operands) == 2
    for operand in callsite.pre.operands:
        assert isinstance(operand, _Atomic)
        assert operand.name == "≥"
        assert isinstance(operand.args[0], _Var)
        assert operand.args[0].name == "input"
        assert isinstance(operand.args[1], _ConstInt)
        assert operand.args[1].value == 10


def test_walk_emits_one_callsite_chain_per_production_call():
    out = _lift(
        """
        def f(x):
            assert x >= 10
            return x

        def g(x):
            if x == 0:
                raise ValueError("zero")
            return x

        def caller():
            a = 12
            b = 3
            left = f(a)
            right = g(b)
            return left + right
        """
    )

    names = [d.name for d in out.decls]
    assert len([name for name in names if name.startswith("f@app.py:")]) == 4
    assert len([name for name in names if name.startswith("g@app.py:")]) == 5
    assert len(out.implications) == 9


def test_lsp_lift_source_includes_production_walk_edges_and_implications():
    lifted = _lift_source(
        "app.py",
        textwrap.dedent(
            """
            def f(x):
                if x < 10:
                    raise ValueError("x must be >= 10")
                return x

            def caller():
                y = 42
                return f(y)
            """
        ),
    )

    names = [decl["name"] for decl in lifted["declarations"]]
    implications = [
        imp for imp in lifted["implications"] if imp["prover"] == "python-wp-walk"
    ]
    assert any(name.endswith("::callsite") for name in names)
    assert any(name.endswith("::let:y") for name in names)
    assert any(name.endswith("::entry") for name in names)
    assert len(implications) == 3
    assert all(imp["antecedentSlot"] == "pre" for imp in implications)
    assert all(imp["consequentSlot"] == "post" for imp in implications)


def test_lsp_lift_source_shows_production_composes_while_tests_conflict():
    lifted = _lift_source(
        "app.py",
        textwrap.dedent(
            """
            import unittest

            def checked(x):
                if x < 10:
                    raise ValueError("x must be >= 10")
                return x

            def composed_ok():
                y = 42
                return checked(y)

            class CheckedContracts(unittest.TestCase):
                def test_checked_returns_42(self):
                    actual = checked(42)
                    self.assertEqual(actual, 42)

                def test_checked_does_not_return_42(self):
                    actual = checked(42)
                    self.assertNotEqual(actual, 42)
            """
        ),
    )

    contracts = lifted["declarations"]
    names = [decl["name"] for decl in contracts]

    production = [
        decl
        for decl in contracts
        if decl["name"].startswith("checked@app.py:")
        and any(decl["name"].endswith(suffix) for suffix in ["::callsite", "::let:y", "::entry"])
    ]
    assert len(production) == 3
    let_edge = next(decl for decl in production if decl["name"].endswith("::let:y"))
    assert let_edge["pre"]["name"] == "≥"
    assert let_edge["pre"]["args"][0]["value"] == 42
    assert let_edge["pre"]["args"][1]["value"] == 10

    # BINDING-FORM EUF SUBSTITUTION: both test methods bind
    # ``actual = checked(42)`` (a CONCRETE 1-arg call) then assert contradictory
    # values (``== 42`` and ``!= 42``).  After the fix the bound assertion
    # subject is the EUF ctor ``callresult_checked_a1(42)`` (not the per-method
    # SSA var ``actual$0``), so the two cross-method assertions coalesce by name
    # into ONE ``checked#euf#...::assertion`` contract whose inv conjoins both
    # equalities — a contradiction that fires UNSAT (REFUSED) at prove time.
    # That is the "tests conflict" this test asserts: the conflict is now a
    # single coalesced contradictory contract rather than two independent
    # location-keyed assertions that would each spuriously PROVE.
    test_assertions = [
        decl
        for decl in contracts
        if decl["name"].startswith("checked#euf#")
        and decl["name"].endswith("::assertion")
    ]
    assert len(test_assertions) == 1, (
        f"concrete-arg binding cross-method must coalesce into ONE EUF assertion, "
        f"got {[d['name'] for d in test_assertions]}"
    )
    # The coalesced inv is an ``and`` of the two contradictory equalities.
    coalesced = test_assertions[0]["inv"]
    assert coalesced["kind"] == "and", coalesced
    coalesced_ops = sorted(op["name"] for op in coalesced["operands"])
    assert coalesced_ops == ["=", "≠"], coalesced_ops
    # No location-keyed ::assertion survives for the concrete-arg binding.
    assert not [
        d for d in contracts
        if d["name"].startswith("checked@app.py:") and d["name"].endswith("::assertion")
    ]
    for test_name in ["test_checked_returns_42", "test_checked_does_not_return_42"]:
        assert all(test_name not in name for name in names)

    wp_implications = [
        imp for imp in lifted["implications"] if imp["prover"] == "python-wp-walk"
    ]
    test_implications = [
        imp for imp in lifted["implications"] if imp["prover"] == "python-test-value-scope"
    ]
    assert len(wp_implications) == 3
    # Both value-scope facts-implies-assertion edges now point at the SAME
    # coalesced EUF assertion name (one per call site, two call sites).
    assert len(test_implications) == 2
