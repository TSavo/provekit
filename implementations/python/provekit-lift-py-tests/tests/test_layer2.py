# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import textwrap

from provekit_lift_py_tests import lift_file_layer2
from provekit_lift_py_tests.ir import (
    _Atomic,
    _ConstInt,
    _ConstStr,
    _Connective,
    _Ctor,
    _Quantifier,
    _Var,
)


def _lift(src: str):
    return lift_file_layer2(textwrap.dedent(src), "t.py")


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


def _guard_names(formula):
    return [
        atom.name
        for atom in _flatten_and(formula)
        if isinstance(atom, _Atomic) and atom.name in {"is_none", "is_some"}
    ]


# --- Pattern 1: bounded loops --------------------------------------------


def test_pattern1_range_two_args_lifts_to_forall_implies():
    out = _lift("""
        def test_squares_are_nonneg():
            for x in range(0, 100):
                assert x >= 0
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.bounded_loop_lifted == 1
    assert "test_squares_are_nonneg" in out.claimed_tests
    assert out.decls[0].name == "test_squares_are_nonneg::loop::x"
    inv = out.decls[0].inv
    assert isinstance(inv, _Quantifier)
    assert inv.kind == "forall"
    assert inv.name == "x"


def test_pattern1_range_one_arg_normalizes_to_lo_zero():
    out = _lift("""
        def test_window():
            for i in range(16):
                assert i < 100
    """)
    assert out.bounded_loop_lifted == 1, f"warnings: {out.warnings}"


def test_pattern1_skips_nested_loop_with_warning_and_keeps_claim():
    out = _lift("""
        def test_nested():
            for x in range(10):
                for y in range(10):
                    assert x >= 0
    """)
    assert out.lifted == 0
    assert out.bounded_loop_skipped == 1
    assert any("nested" in w.reason for w in out.warnings)
    # Even on skip, claimed so Layer 0 doesn't retry.
    assert "test_nested" in out.claimed_tests


def test_pattern1_skips_range_with_step():
    out = _lift("""
        def test_step():
            for x in range(0, 10, 2):
                assert x >= 0
    """)
    assert out.lifted == 0
    assert any("range()" in w.reason or "range" in w.reason for w in out.warnings)


def test_pattern1_skips_iterating_over_call_result():
    out = _lift("""
        def test_callsite():
            for x in some_helper():
                assert x >= 0
    """)
    assert out.lifted == 0
    assert any("range" in w.reason or "list" in w.reason for w in out.warnings)


def test_pattern1_lifts_literal_list_iter_to_and_conjunction():
    out = _lift("""
        def test_three_values():
            for v in [1, 2, 3]:
                assert v >= 0
    """)
    assert out.bounded_loop_lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    assert len(inv.operands) == 3


# --- Pattern 2: helper inlining ------------------------------------------


def test_pattern2_helper_inlines_each_call():
    out = _lift("""
        def assert_is_42(x: int):
            assert x == 42

        def test_many_42s():
            assert_is_42(42)
            assert_is_42(42)
            assert_is_42(42)
    """)
    assert out.lifted == 3, f"warnings: {out.warnings}"
    assert out.helper_inlined_lifted == 3
    names = sorted(d.name for d in out.decls)
    assert names == [
        "test_many_42s::call::0",
        "test_many_42s::call::1",
        "test_many_42s::call::2",
    ]


def test_pattern2_skips_helper_without_annotation():
    out = _lift("""
        def assert_is_42(x):
            assert x == 42

        def test_many():
            assert_is_42(42)
    """)
    # Helper rejected -> not pattern 2 -> falls through. Single assert
    # is also not pattern 3 (need >=2). So nothing claimed.
    assert out.lifted == 0
    assert "test_many" not in out.claimed_tests


def test_pattern2_skips_call_with_kwarg():
    out = _lift("""
        def assert_is_42(x: int):
            assert x == 42

        def test_kw():
            assert_is_42(x=42)
    """)
    assert out.lifted == 0


# --- Pattern 3: characterization ----------------------------------------


def test_pattern3_lifts_to_and_conjunction():
    out = _lift("""
        def test_three_facts():
            assert f(1) == 1
            assert f(2) == 2
            assert f(3) != 0
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.characterization_lifted == 1
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    assert len(inv.operands) == 3


def test_pattern3_releases_claim_with_only_one_atom():
    # f(1) is liftable; "hi".upper() is not. 1 < 2 -> release claim.
    out = _lift("""
        def test_mixed():
            assert f(1) == 1
            assert "hi".upper() == "HI"
    """)
    assert out.characterization_lifted == 0
    assert "test_mixed" not in out.claimed_tests


def test_pattern3_unittest_assertEqual_recognized():
    out = _lift("""
        import unittest

        class TestSomething(unittest.TestCase):
            def test_three(self):
                self.assertEqual(f(1), 1)
                self.assertEqual(f(2), 2)
                self.assertNotEqual(f(3), 0)
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    # Class methods are qualified: "ClassName::method_name".
    assert out.decls[0].name == "TestSomething::test_three"


def test_pattern3_plain_unittest_testcase_assertions_lift_to_contract_atoms():
    out = _lift("""
        import unittest

        class ParserTest(unittest.TestCase):
            def test_native_assertions(self):
                self.assertEqual(parse_int("42"), 42)
                self.assertNotEqual(parse_int("0"), 1)
                self.assertTrue(parse_int("5") > 0)
                self.assertIsNone(maybe_none())
                self.assertIsNotNone(maybe_value())
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    # Class methods are qualified: "ClassName::method_name".
    assert out.decls[0].name == "ParserTest::test_native_assertions"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    atoms = [atom for atom in _flatten_and(inv) if isinstance(atom, _Atomic)]
    assert [atom.name for atom in atoms] == ["=", "≠", ">", "=", "is_none", "≠", "is_some"]
    none_atoms = [
        atom
        for atom in atoms
        if atom.name in {"=", "≠"} and isinstance(atom.args[1], _Ctor)
    ]
    assert [atom.args[1].name for atom in none_atoms] == ["None", "None"]
    _assert_none_guard_formula(inv, comparison_name="=", guard_name="is_none")
    _assert_none_guard_formula(inv, comparison_name="≠", guard_name="is_some")


def test_pattern3_pytest_none_comparisons_emit_substrate_guard_facts():
    out = _lift("""
        def test_none_assertions():
            assert maybe_none() is None
            assert maybe_value() is not None
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    _assert_none_guard_formula(inv, comparison_name="=", guard_name="is_none")
    _assert_none_guard_formula(inv, comparison_name="≠", guard_name="is_some")


def test_pattern3_pytest_non_none_identity_is_not_lifted_as_value_equality():
    out = _lift("""
        def test_identity_is_not_equality():
            assert left() is right()
            assert f(1) == 1
    """)
    assert out.characterization_lifted == 0
    assert "test_identity_is_not_equality" not in out.claimed_tests


def test_pattern3_pytest_eq_none_does_not_emit_substrate_guard_facts():
    out = _lift("""
        def test_eq_none_is_value_equality():
            assert maybe_none() == None
            assert maybe_value() != None
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    assert _guard_names(inv) == []


def test_pattern3_unittest_equal_none_does_not_emit_substrate_guard_facts():
    out = _lift("""
        import unittest

        class TestSomething(unittest.TestCase):
            def test_none_equality(self):
                self.assertEqual(maybe_none(), None)
                self.assertNotEqual(maybe_value(), None)
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    assert _guard_names(inv) == []


def test_pattern3_unittest_non_none_identity_is_not_lifted_as_value_equality():
    out = _lift("""
        import unittest

        class TestSomething(unittest.TestCase):
            def test_identity_is_not_equality(self):
                self.assertIs(left(), right())
                self.assertIsNot(other_left(), other_right())
    """)
    assert out.characterization_lifted == 0
    assert "test_identity_is_not_equality" not in out.claimed_tests


def test_pattern3_unittest_identity_none_assertions_emit_substrate_guard_facts():
    out = _lift("""
        import unittest

        class TestSomething(unittest.TestCase):
            def test_none_identity(self):
                self.assertIs(maybe_none(), None)
                self.assertIsNot(maybe_value(), None)
    """)
    assert out.characterization_lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    _assert_none_guard_formula(inv, comparison_name="=", guard_name="is_none")
    _assert_none_guard_formula(inv, comparison_name="≠", guard_name="is_some")


def test_unittest_unsupported_assertion_warns_without_fake_contract():
    out = _lift("""
        import unittest

        class RegexTest(unittest.TestCase):
            def test_regex(self):
                self.assertRegex("abc", "a.*")
    """)
    assert out.lifted == 0
    # Class methods are qualified in claimed_tests too.
    assert "RegexTest::test_regex" in out.claimed_tests
    assert any("assertRegex" in w.reason and "lift-gap" in w.reason for w in out.warnings)


# --- Pattern 4: parametrize ---------------------------------------------


def test_pattern4_parametrize_single_param_lifts_to_and_over_rows():
    out = _lift("""
        import pytest

        @pytest.mark.parametrize("v", [1, 2, 3, 4])
        def test_nonneg(v):
            assert v >= 0
    """)
    assert out.parametrize_lifted == 1, f"warnings: {out.warnings}"
    assert out.decls[0].name == "test_nonneg::parametrize::v"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    assert len(inv.operands) == 4
    # Each operand should be an atomic with the literal value substituted in.
    for op in inv.operands:
        assert isinstance(op, _Atomic)
        assert op.name == "≥"


def test_pattern4_parametrize_two_params_via_tuple_rows():
    out = _lift("""
        import pytest

        @pytest.mark.parametrize("a, b", [(1, 1), (2, 2), (3, 3)])
        def test_pairs(a, b):
            assert a == b
    """)
    assert out.parametrize_lifted == 1, f"warnings: {out.warnings}"
    assert out.decls[0].name == "test_pairs::parametrize::a_b"


def test_pattern4_parametrize_skips_non_literal_row():
    out = _lift("""
        import pytest

        @pytest.mark.parametrize("v", [some_helper(), 2])
        def test_dyn(v):
            assert v >= 0
    """)
    # Row arg not literal -> claim with warning, don't lift.
    assert out.parametrize_lifted == 0
    assert "test_dyn" in out.claimed_tests
    assert any("parametrize" in w.reason for w in out.warnings)


# --- Pattern 5: value-scope assertions -----------------------------------


def test_pattern5_local_assignment_scopes_pytest_assertion():
    out = _lift("""
        def test_parse_value_scope():
            actual = parse_int("42")
            assert actual == 42
    """)
    assert out.lifted == 2, f"warnings: {out.warnings}"
    assert out.value_scope_lifted == 1
    assert "test_parse_value_scope" in out.claimed_tests
    by_name = {d.name: d for d in out.decls}
    assert len(by_name) == 2
    assert all(name.startswith("parse_int@t.py:") for name in by_name)
    assert all("test_parse_value_scope" not in name for name in by_name)
    callsite_name = next(name for name in by_name if name.endswith("::facts"))
    assertion_name = next(name for name in by_name if name.endswith("::assertion"))
    assert len(out.implications) == 1
    assert out.implications[0].name.startswith("parse_int@t.py:")
    assert "test_parse_value_scope" not in out.implications[0].name
    assert out.implications[0].antecedent == callsite_name
    assert out.implications[0].consequent == assertion_name

    fact = by_name[callsite_name].inv
    consequent = by_name[assertion_name].inv

    assert isinstance(fact, _Atomic)
    assert fact.name == "="
    assert isinstance(fact.args[0], _Var)
    assert fact.args[0].name == "actual$0"
    assert isinstance(fact.args[1], _Ctor)
    assert fact.args[1].name == "parse_int"
    assert isinstance(fact.args[1].args[0], _ConstStr)
    assert fact.args[1].args[0].value == "42"

    assert isinstance(consequent, _Atomic)
    assert consequent.name == "="
    assert isinstance(consequent.args[0], _Var)
    assert consequent.args[0].name == "actual$0"
    assert isinstance(consequent.args[1], _ConstInt)
    assert consequent.args[1].value == 42


def test_pattern5_if_else_scopes_assertion_to_each_branch():
    out = _lift("""
        def test_branch_value_scope(raw):
            if raw == "42":
                actual = parse_int(raw)
            else:
                actual = parse_int("0")
            assert actual >= 0
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    names = {d.name for d in out.decls}
    assert len(names) == 4
    assert all(name.startswith("parse_int@t.py:") for name in names)
    assert all("test_branch_value_scope" not in name for name in names)
    assert len([name for name in names if name.endswith("::facts")]) == 2
    assert len([name for name in names if name.endswith("::assertion")]) == 2
    assert len(out.implications) == 2
    for d in out.decls:
        assert d.inv is not None
        if d.name.endswith("::facts"):
            assert isinstance(d.inv, _Connective)
            assert d.inv.kind == "and"
        if d.name.endswith("::assertion"):
            assert isinstance(d.inv, _Atomic)
            assert d.inv.name == "≥"


def test_pattern5_local_assignment_scopes_unittest_assertion():
    out = _lift("""
        import unittest

        class TestParser(unittest.TestCase):
            def test_parse_value_scope(self):
                actual = parse_int("42")
                self.assertEqual(actual, 42)
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    names = {d.name for d in out.decls}
    assert len(names) == 2
    assert all(name.startswith("parse_int@t.py:") for name in names)
    assert all("test_parse_value_scope" not in name for name in names)
    assert len(out.implications) == 1


def test_pattern5_direct_call_assertion_lifts_to_callsite_contracts():
    out = _lift("""
        def test_direct_parse():
            assert parse_int("42") == 42
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assert "test_direct_parse" in out.claimed_tests
    names = {d.name for d in out.decls}
    assert len(names) == 2
    assert all(name.startswith("parse_int@t.py:") for name in names)
    assert all("test_direct_parse" not in name for name in names)
    assert len([name for name in names if name.endswith("::facts")]) == 1
    assert len([name for name in names if name.endswith("::assertion")]) == 1
    assert len(out.implications) == 1


def test_pattern5_mints_every_callsite_implication_in_one_assertion():
    out = _lift("""
        def test_two_calls():
            assert parse_int("42") == parse_int("042")
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    names = {d.name for d in out.decls}
    assert len([name for name in names if name.endswith("::facts")]) == 2
    assert len([name for name in names if name.endswith("::assertion")]) == 2
    assert len(out.implications) == 2
    assert all(imp.antecedent.endswith("::facts") for imp in out.implications)
    assert all(imp.consequent.endswith("::assertion") for imp in out.implications)


def test_pattern5_non_none_identity_assertion_is_not_lifted_as_value_equality():
    out = _lift("""
        def test_parse_identity():
            actual = parse_int("42")
            expected = parse_int("042")
            assert actual is expected
    """)
    assert out.value_scope_lifted == 0
    assert not out.implications
    assert all(not name.endswith("::assertion") for name in {d.name for d in out.decls})


def test_pattern5_unittest_non_none_identity_assertion_is_not_lifted_as_value_equality():
    out = _lift("""
        import unittest

        class TestParser(unittest.TestCase):
            def test_parse_identity(self):
                actual = parse_int("42")
                expected = parse_int("042")
                self.assertIs(actual, expected)
    """)
    assert out.value_scope_lifted == 0
    assert not out.implications
    assert all(not name.endswith("::assertion") for name in {d.name for d in out.decls})


def test_pattern5_eq_none_assertion_does_not_emit_substrate_guard_fact():
    out = _lift("""
        def test_parse_eq_none():
            actual = parse_optional("42")
            assert actual == None
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assertion = next(d for d in out.decls if d.name.endswith("::assertion"))
    assert _guard_names(assertion.inv) == []


def test_pattern5_unittest_identity_none_assertion_emits_substrate_guard_fact():
    out = _lift("""
        import unittest

        class TestParser(unittest.TestCase):
            def test_parse_none_identity(self):
                actual = parse_optional("42")
                self.assertIs(actual, None)
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assertion = next(d for d in out.decls if d.name.endswith("::assertion"))
    _assert_none_guard_formula(assertion.inv, comparison_name="=", guard_name="is_none")


def test_pattern5_unittest_identity_not_none_assertion_emits_substrate_guard_fact():
    out = _lift("""
        import unittest

        class TestParser(unittest.TestCase):
            def test_parse_not_none_identity(self):
                actual = parse_optional("42")
                self.assertIsNot(actual, None)
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assertion = next(d for d in out.decls if d.name.endswith("::assertion"))
    _assert_none_guard_formula(assertion.inv, comparison_name="≠", guard_name="is_some")


# --- No pattern fires ----------------------------------------------------


def test_single_literal_assert_test_falls_through_to_layer0():
    out = _lift("""
        def test_one():
            assert 1 == 1
    """)
    assert out.lifted == 0
    assert not out.claimed_tests


# --- Attribute-access term lifting (obj.attr as opaque Var) ---------------
# Census-driven: `out.bounded_loop_lifted == 4` style asserts in test_integration.py
# were silently unhandled. Attribute access lifts to a Var named by the dotted
# path; same path -> same Var -> contradictions fire UNSAT.


def test_attribute_eq_int_lifts_to_char_pattern3():
    # POSITIVE: two consistent attribute-equality assertions -> Pattern 3.
    out = _lift("""
        def test_attr_consistent(out):
            assert out.lifted == 4
            assert out.skipped == 1
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.characterization_lifted == 1
    inv = out.decls[0].inv
    # Both atoms must be equalities involving Var names with dots.
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    assert any(
        a.name == "="
        and isinstance(a.args[0], _Var)
        and a.args[0].name == "out.lifted"
        for a in atoms
    ), f"expected '=' atomic with Var 'out.lifted', got {atoms}"
    assert any(
        a.name == "="
        and isinstance(a.args[0], _Var)
        and a.args[0].name == "out.skipped"
        for a in atoms
    ), f"expected '=' atomic with Var 'out.skipped', got {atoms}"


def test_attribute_eq_contradictory_same_path_lifts():
    # DISCRIMINATION: same dotted path, two different int literals -> UNSAT.
    # `assert out.val == 1; assert out.val == 2` -> `=(out.val, 1) ∧ =(out.val, 2)`.
    # Same Var, distinct Int constants -> z3 UNSAT -> REFUSED.
    out = _lift("""
        def test_attr_contradictory(out):
            assert out.val == 1
            assert out.val == 2
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    assert len([a for a in atoms if a.name == "="]) == 2, (
        "expected two equality atoms for the contradiction, got: "
        + str(atoms)
    )
    # Both atoms reference the SAME var name.
    eq_atoms = [a for a in atoms if a.name == "="]
    assert all(
        isinstance(a.args[0], _Var) and a.args[0].name == "out.val"
        for a in eq_atoms
    ), f"both atoms must reference 'out.val', got {eq_atoms}"
    # The two int constants must be different (1 vs 2).
    vals = sorted(
        a.args[1].value for a in eq_atoms if isinstance(a.args[1], _ConstInt)
    )
    assert vals == [1, 2], f"expected distinct int values [1, 2], got {vals}"


def test_attribute_different_paths_are_independent():
    # STRUCTURAL: different attribute paths -> different Var names -> independent.
    # `assert a.x == 1; assert b.x == 1` -> `=(a.x, 1) ∧ =(b.x, 1)` which IS
    # satisfiable (a.x and b.x are independent free variables).
    out = _lift("""
        def test_attr_independent(a, b):
            assert a.x == 1
            assert b.x == 1
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    atoms = [a for a in _flatten_and(out.decls[0].inv) if isinstance(a, _Atomic)]
    var_names = {a.args[0].name for a in atoms if isinstance(a.args[0], _Var)}
    assert var_names == {"a.x", "b.x"}, (
        f"expected two distinct Var names {{a.x, b.x}}, got {var_names}"
    )


# --- Membership lifting (in / not in as uninterpreted predicate) ----------
# ``x in coll`` -> ``member(x,coll)``; ``x not in coll`` -> ``not(member(x,coll))``.
# Same predicate symbol for both -> contradiction is propositionally UNSAT
# (no set-theory needed); z3 discharges via the Bool-returning uninterpreted fn.


def test_membership_in_lifts():
    # POSITIVE: ``x in coll`` lifts to ``member(x, coll)`` atomic.
    out = _lift("""
        def test_in_consistent(x, coll):
            assert x in coll
            assert x in coll
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    atoms = [a for a in _flatten_and(out.decls[0].inv) if isinstance(a, _Atomic)]
    assert any(a.name == "member" for a in atoms), (
        f"expected 'member' atomic, got {atoms}"
    )


def test_membership_not_in_lifts_as_negation():
    # POSITIVE: ``x not in coll`` lifts to ``not(member(x, coll))``.
    # Shape: _Connective(kind='not', operands=(_Atomic('member', ...),)).
    out = _lift("""
        def test_not_in_lifts(x, coll):
            assert x not in coll
            assert x not in coll
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    # not(member(x,coll)) appears in the conjunction.
    nots = [
        o for o in _flatten_and(inv)
        if isinstance(o, _Connective) and o.kind == "not"
    ]
    assert len(nots) >= 1, f"expected at least one 'not' connective, got inv={inv}"
    # The inner operand of each `not` is a `member` atomic.
    for n in nots:
        inner = n.operands[0]
        assert isinstance(inner, _Atomic) and inner.name == "member", (
            f"not-in must wrap a 'member' atomic, got {inner}"
        )


def test_membership_in_not_in_contradictory_lifts():
    # DISCRIMINATION: ``x in coll`` AND ``x not in coll`` -> REFUSED.
    # ``member(x,c) ∧ not(member(x,c))`` is propositionally UNSAT.
    out = _lift("""
        def test_in_contradictory(x, coll):
            assert x in coll
            assert x not in coll
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    inv = out.decls[0].inv
    # The conjunction must contain BOTH a member-atomic AND a not(member-atomic).
    atoms = [o for o in _flatten_and(inv) if isinstance(o, _Atomic)]
    nots = [o for o in _flatten_and(inv) if isinstance(o, _Connective) and o.kind == "not"]
    assert any(a.name == "member" for a in atoms), (
        f"expected a 'member' atom for the positive side, got inv={inv}"
    )
    assert len(nots) >= 1, f"expected at least one 'not' for the negative side, got inv={inv}"
    not_member = [n for n in nots if isinstance(n.operands[0], _Atomic) and n.operands[0].name == "member"]
    assert not_member, f"not-in side must wrap a 'member' atomic, got nots={nots}"


def test_membership_in_string_const_key_lifts():
    # POSITIVE: string constant as membership subject (the census form).
    # ``'foo' in names`` -> ``member(strlit, names_var)``
    out = _lift("""
        def test_str_in_names(names):
            assert 'foo' in names
            assert 'bar' in names
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    atoms = [a for a in _flatten_and(out.decls[0].inv) if isinstance(a, _Atomic)]
    assert all(a.name == "member" for a in atoms), (
        f"expected only 'member' atoms, got {atoms}"
    )
    # The RHS (coll arg) must be the Var 'names'.
    assert all(
        isinstance(a.args[1], _Var) and a.args[1].name == "names"
        for a in atoms
    ), f"collection arg must be Var 'names', got {atoms}"


# --- Class-scoped method discrimination (Task 1) -------------------------
# Class methods are qualified as ``ClassName::test_method``.  This keeps
# each class-method's decl scope independent even when two classes define
# the same method name.  Discrimination rule: contradictory → REFUSED;
# consistent → PROVEN; same method name in different classes → independent
# decls with different names.


def test_class_method_contradictory_lifts_with_qualified_name():
    # DISCRIMINATION: class-method with contradictory asserts produces a
    # qualified decl name; the conjunction is UNSAT → REFUSED.
    out = _lift("""
        class TestFoo:
            def test_contradictory_class_method(self):
                assert x == 1
                assert x == 2
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.decls[0].name == "TestFoo::test_contradictory_class_method"
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    # Two equality atoms on the same Var → UNSAT conjunction → REFUSED.
    assert len(eq_atoms) == 2, f"expected 2 equality atoms, got {atoms}"
    assert all(isinstance(a.args[0], _Var) and a.args[0].name == "x" for a in eq_atoms)


def test_class_method_consistent_lifts_with_qualified_name():
    # POSITIVE: class-method with consistent (satisfiable) asserts → PROVEN.
    out = _lift("""
        class TestFoo:
            def test_consistent_class_method(self):
                assert x == 1
                assert y == 2
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.decls[0].name == "TestFoo::test_consistent_class_method"
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    # x==1 and y==2 are independent free vars → SAT conjunction → PROVEN.
    assert len(eq_atoms) == 2
    var_names = {a.args[0].name for a in eq_atoms if isinstance(a.args[0], _Var)}
    assert var_names == {"x", "y"}


def test_class_method_same_name_two_classes_are_independent():
    # STRUCTURAL: two classes with the same method name get distinct qualified
    # names so their decls are never conjoined by the mint layer.
    out = _lift("""
        class TestFoo:
            def test_x(self):
                assert a == 1
                assert b == 2

        class TestBar:
            def test_x(self):
                assert c == 3
                assert d == 4
    """)
    assert out.lifted == 2, f"warnings: {out.warnings}"
    names = {d.name for d in out.decls}
    assert "TestFoo::test_x" in names
    assert "TestBar::test_x" in names


def test_free_function_keeps_bare_name():
    # REGRESSION: free (module-level) functions must NOT be class-qualified.
    out = _lift("""
        def test_free_function():
            assert x == 1
            assert y == 2
    """)
    assert out.lifted == 1, f"warnings: {out.warnings}"
    assert out.decls[0].name == "test_free_function", (
        f"free function must have bare name, got {out.decls[0].name!r}"
    )


# --- Attribute SSA on base variable (Task 2) ------------------------------
# When the base variable of an attribute access is SSA-renamed in scope
# (because it was bound by an assignment), the attribute Var is keyed on
# the SSA name (``out$0.val`` vs ``out$1.val``) rather than the raw
# dotted path (``out.val``).  This ensures that:
#
#   1. ``out = f(x); assert out.val == 1; out = g(y); assert out.val == 2``
#      → two INDEPENDENT Vars (``out$0.val`` and ``out$1.val``) in their
#        respective callsite-assertion contracts → SAT for each → PROVEN.
#   2. ``out = f(x); assert out.val == 1; assert out.val == 2``
#      (no reassignment) → SAME Var (``out$0.val``) in ONE conjoined
#        contract → UNSAT → REFUSED.


def test_attr_ssa_reassign_gives_independent_assertion_vars():
    # DISCRIMINATION (false-refusal CLOSED):
    # Two consecutive bindings of ``out`` (to different call-results) then
    # a single attribute assertion per binding.  The two assertions must
    # land in separate callsite-assertion contracts and reference distinct
    # SSA-keyed Var names so neither contract is unsatisfiable.
    out = _lift("""
        def test_attr_reassign_consistent():
            x = some_input()
            y = other_input()
            out = process(x)
            assert out.val == 1
            out = process(y)
            assert out.val == 2
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assertion_decls = [d for d in out.decls if d.name.endswith("::assertion")]
    assert len(assertion_decls) == 2, (
        f"expected 2 assertion contracts (one per call-site), got {[d.name for d in assertion_decls]}"
    )
    var_names = set()
    for d in assertion_decls:
        atoms = [a for a in _flatten_and(d.inv) if isinstance(a, _Atomic)]
        for a in atoms:
            if isinstance(a.args[0], _Var):
                var_names.add(a.args[0].name)
    # The two assertion contracts must reference DIFFERENT Var names.
    assert len(var_names) == 2, (
        f"expected 2 distinct attribute Var names (SSA-keyed), got {var_names}"
    )
    # Each name must contain the attribute suffix '.val'.
    assert all(".val" in n for n in var_names), (
        f"expected '.val' suffix in all Var names, got {var_names}"
    )
    # The two names must differ in their base (SSA suffix).
    names_sorted = sorted(var_names)
    assert names_sorted[0] != names_sorted[1]


def test_attr_ssa_no_reassign_conjoins_same_var():
    # DISCRIMINATION (genuine contradiction still REFUSED):
    # No reassignment between two attribute assertions on the same base.
    # Both assertions reference the SAME SSA-keyed Var → conjoin into one
    # contract → UNSAT → REFUSED.
    out = _lift("""
        def test_attr_no_reassign_contradictory():
            x = some_input()
            out = process(x)
            assert out.val == 1
            assert out.val == 2
    """)
    assert out.value_scope_lifted == 1, f"warnings: {out.warnings}"
    assertion_decls = [d for d in out.decls if d.name.endswith("::assertion")]
    assert len(assertion_decls) == 1, (
        f"expected 1 conjoined assertion contract, got {[d.name for d in assertion_decls]}"
    )
    inv = assertion_decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 equality atoms in conjoined contract, got {atoms}"
    # Both reference the SAME Var name (same SSA base, no reassignment).
    var_names = {a.args[0].name for a in eq_atoms if isinstance(a.args[0], _Var)}
    assert len(var_names) == 1, (
        f"expected both atoms to share the same Var name, got {var_names}"
    )
    the_name = next(iter(var_names))
    assert ".val" in the_name, f"expected '.val' in Var name, got {the_name!r}"


# --- Pattern 6: mixed-body (opaque bindings + asserts) --------------------
# Tests an interleaving of assignment statements (possibly with un-translatable
# RHS like ``f(**kwargs)`` or ``json.loads(...)``) and ``assert`` statements.
# The pattern SSA-keys each binding and conjoins the lifted asserts into ONE
# whole-test contract (no ::facts / ::assertion split; no implication wiring).
#
# FOUR DISCRIMINATION TESTS (permanent, per spec):
#   1. CONTRADICTORY  – same SSA-keyed subject, incompatible claims → REFUSED
#   2. CONSISTENT     – same SSA-keyed subject, compatible claims → PROVEN
#   3. INDEPENDENT    – different SSA-keyed subjects → no cross-contamination
#   4. REASSIGNMENT   – SSA rebinding keeps assertions independent → PROVEN


def test_mixed_body_contradictory_same_subject_lifts():
    # DISCRIMINATION: opaque binding (un-translatable RHS via **kwargs) + two
    # contradictory asserts about the same SSA-keyed attribute Var → UNSAT.
    # ``out = some_factory(**kw)``  → out$0 (opaque; **kwargs blocks P5)
    # ``assert out.val == 1``       → =(out$0.val, 1)
    # ``assert out.val == 2``       → =(out$0.val, 2)
    # and(=(out$0.val,1), =(out$0.val,2)) — same Var, distinct Int → UNSAT.
    out = _lift("""
        def test_mixed_body_contradictory():
            out = some_factory(**kw)
            assert out.val == 1
            assert out.val == 2
    """)
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    assert out.lifted == 1
    assert "test_mixed_body_contradictory" in out.claimed_tests
    # The decl must be named by the test (no callsite prefix).
    assert out.decls[0].name == "test_mixed_body_contradictory"
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 equality atoms, got {atoms}"
    # Both atoms must reference the SAME SSA-keyed Var (out$0.val).
    var_names = {a.args[0].name for a in eq_atoms if isinstance(a.args[0], _Var)}
    assert len(var_names) == 1, (
        f"both atoms must share one SSA Var (same subject), got {var_names}"
    )
    assert "out$0.val" in var_names, (
        f"Var must be SSA-keyed as 'out$0.val', got {var_names}"
    )
    # Distinct Int values → UNSAT conjunction.
    vals = sorted(
        a.args[1].value for a in eq_atoms if isinstance(a.args[1], _ConstInt)
    )
    assert vals == [1, 2], f"expected distinct [1,2], got {vals}"


def test_mixed_body_consistent_same_subject_lifts():
    # POSITIVE: opaque binding (un-translatable RHS) + two compatible asserts.
    # ``assert out.val == 1; assert out.val != 2`` → and(=(out$0.val,1), ≠(out$0.val,2))
    # Satisfiable (val=1 satisfies both) → SAT → PROVEN-consistent.
    out = _lift("""
        def test_mixed_body_consistent():
            out = some_factory(**kw)
            assert out.val == 1
            assert out.val != 2
    """)
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    assert out.lifted == 1
    assert "test_mixed_body_consistent" in out.claimed_tests
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    # Must contain at least one '=' and one '≠' atom.
    assert any(a.name == "=" for a in atoms), f"expected '=' atom, got {atoms}"
    assert any(a.name == "≠" for a in atoms), f"expected '≠' atom, got {atoms}"
    # Both must reference the same SSA Var.
    all_vars = {a.args[0].name for a in atoms if isinstance(a.args[0], _Var)}
    assert len(all_vars) == 1, f"expected single SSA Var, got {all_vars}"
    assert "out$0.val" in all_vars


def test_mixed_body_independent_subjects_no_cross_contamination():
    # STRUCTURAL: two opaque bindings (un-translatable RHS via **kwargs), each
    # asserted about independently.
    # → and(=(a$0.val,1), =(b$0.val,2)) — DIFFERENT free vars → SAT → PROVEN.
    # Guard: a$0.val and b$0.val must NOT be merged; merging would give a
    # spurious contradiction if 1 ≠ 2 in the same-Var interpretation.
    out = _lift("""
        def test_mixed_body_independent():
            a = factory_a(**kw)
            b = factory_b(**kw)
            assert a.val == 1
            assert b.val == 2
    """)
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    assert out.lifted == 1
    assert "test_mixed_body_independent" in out.claimed_tests
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 equality atoms, got {atoms}"
    # The two atoms must reference DIFFERENT Vars (a$0.val vs b$0.val).
    var_names = {a.args[0].name for a in eq_atoms if isinstance(a.args[0], _Var)}
    assert var_names == {"a$0.val", "b$0.val"}, (
        f"expected {{a$0.val, b$0.val}}, got {var_names}"
    )


def test_mixed_body_reassignment_gives_fresh_ssa_not_false_refused():
    # REASSIGNMENT guard (cardinal): re-binding the same name gives a fresh
    # SSA generation. The two assertions must reference DIFFERENT Vars.
    # Without SSA: both collapse to ``out.val`` → UNSAT → FALSE REFUSAL.
    # With SSA: out$0.val and out$1.val are independent → SAT → PROVEN.
    # Opaque RHS via **kwargs ensures Pattern 5 does not claim this first.
    # ``out = factory_a(**kw)``  → out$0
    # ``assert out.val == 1``    → =(out$0.val, 1)
    # ``out = factory_b(**kw)``  → out$1 (fresh SSA generation)
    # ``assert out.val == 2``    → =(out$1.val, 2)
    out = _lift("""
        def test_mixed_body_reassign():
            out = factory_a(**kw)
            assert out.val == 1
            out = factory_b(**kw)
            assert out.val == 2
    """)
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    assert out.lifted == 1
    assert "test_mixed_body_reassign" in out.claimed_tests
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    eq_atoms = [a for a in atoms if a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 equality atoms, got {atoms}"
    # The two atoms must reference DIFFERENT SSA Vars (different generations).
    var_names = {a.args[0].name for a in eq_atoms if isinstance(a.args[0], _Var)}
    assert len(var_names) == 2, (
        f"expected 2 distinct SSA-keyed Vars (out$0.val, out$1.val), got {var_names}"
    )
    assert var_names == {"out$0.val", "out$1.val"}, (
        f"expected {{out$0.val, out$1.val}}, got {var_names}"
    )


def test_mixed_body_subscript_assign_loudly_refused():
    # LOUD REFUSAL: subscript-assign targets (``kwargs['x'] = val``) are
    # mutations that cannot be soundly modeled as SSA bindings. The method
    # is claimed so Layer 0 does not retry it; a warning naming the mutation
    # is emitted; zero contracts are produced.
    out = _lift("""
        def test_mixed_body_mutation():
            kwargs = make_kwargs()
            kwargs['key'] = 42
            result = process(**kwargs)
            assert result.val == 1
    """)
    assert out.mixed_body_skipped == 1, f"warnings: {out.warnings}"
    assert out.mixed_body_lifted == 0
    assert out.lifted == 0
    assert "test_mixed_body_mutation" in out.claimed_tests
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings), (
        f"expected LOUD REFUSAL warning, got {[w.reason for w in out.warnings]}"
    )
    assert any("kwargs['key']" in w.reason or "subscript" in w.reason.lower()
               for w in out.warnings), (
        f"expected mention of subscript/mutation, got {[w.reason for w in out.warnings]}"
    )


def test_mixed_body_with_stmt_loudly_refused():
    # LOUD REFUSAL: a ``with`` block inside a mixed-body test (alongside a
    # top-level binding + assert) cannot be soundly modeled.
    # The With triggers the unsupported-statement check → LOUD REFUSAL.
    out = _lift("""
        def test_mixed_body_with():
            val = make_val(**kw)
            with some_ctx():
                do_something()
            assert val.ok == 1
    """)
    assert out.mixed_body_skipped == 1, f"warnings: {out.warnings}"
    assert out.mixed_body_lifted == 0
    assert out.lifted == 0
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_mixed_body_for_loop_loudly_refused():
    # LOUD REFUSAL: a for-loop BETWEEN an assert and a binding (top-level)
    # triggers an unsupported-statement check.
    # Note: a for-loop with only nested asserts (no top-level assert) does not
    # reach Pattern 6 at all (it fails the has_assert gate and falls through).
    # This test uses a top-level assert + a for-loop to trigger the refusal.
    out = _lift("""
        def test_mixed_body_for():
            items = make_items(**kw)
            for x in items:
                pass
            assert items != None
    """)
    assert out.mixed_body_skipped == 1, f"warnings: {out.warnings}"
    assert out.mixed_body_lifted == 0
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_mixed_body_opaque_rhs_produces_no_facts_contract():
    # SOUNDNESS: an opaque RHS (not translatable) must NOT emit a ::facts
    # contract. The only output is the test-named whole-body contract.
    out = _lift("""
        def test_opaque_rhs():
            out = some_opaque(**kw)
            assert out.val == 42
    """)
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    # Only one decl: the test-named contract.
    assert len(out.decls) == 1, f"expected 1 decl, got {[d.name for d in out.decls]}"
    assert out.decls[0].name == "test_opaque_rhs"
    # No implications (no ::facts / ::assertion split).
    assert not out.implications, f"expected no implications, got {out.implications}"
    # No ::facts in any decl name.
    assert all("::facts" not in d.name for d in out.decls)


def test_mixed_body_unliftable_assert_warned_but_rest_lifts():
    # PARTIAL LIFT: when only some asserts are liftable, warn about the
    # skipped ones but still emit a contract for the liftable ones.
    # Uses opaque RHS (**kw) to prevent Pattern 5 from claiming the test.
    # ``assert a$0.x == 1`` is liftable; a chained assert is NOT liftable.
    out = _lift("""
        def test_partial_lift():
            a = factory_a(**kw)
            b = factory_b(**kw)
            assert a.x == 1
            assert a.y == b.y
    """)
    # Both asserts ARE liftable (simple attribute-equality comparisons).
    assert out.mixed_body_lifted == 1, f"warnings: {out.warnings}"
    assert out.lifted == 1
    inv = out.decls[0].inv
    atoms = [a for a in _flatten_and(inv) if isinstance(a, _Atomic)]
    assert len(atoms) == 2, f"expected 2 atoms, got {atoms}"


def test_mixed_body_pure_opaque_all_unliftable_claims_with_warning():
    # When ALL asserts are unliftable (e.g. all are method-calls that have a
    # non-simple-name call target), zero contracts are produced but the test is
    # still CLAIMED so Layer 0 can pick it up.  A warning is emitted explaining
    # the skip.
    #
    # NOTE: ``parsed['key'] == 'value'`` is now LIFTABLE (subscript-index
    # support).  Use a method-call assert (``parsed.keys()`` is a call with an
    # attribute func, not liftable) to keep this test exercising the all-
    # unliftable path.
    out = _lift("""
        def test_all_unliftable():
            parsed = json_parse(raw)
            assert parsed.keys() == {'a', 'b'}
    """)
    # Nothing liftable → 0 contracts but claimed + warned.
    assert out.mixed_body_lifted == 0, f"unexpected lift, decls: {[d.name for d in out.decls]}"
    assert out.mixed_body_skipped == 1
    assert "test_all_unliftable" in out.claimed_tests
    assert any("releasing claim to Layer 0" in w.reason for w in out.warnings)


# ---------------------------------------------------------------------------
# SUBSCRIPT-INDEX discrimination tests (permanent regression fixtures)
# ---------------------------------------------------------------------------
#
# Three properties per the brief:
#   (1) POSITIVE: literal subscript lifts and produces a ContractDecl.
#   (2) DISCRIMINATION: same-subscript contradictory conjoins into one inv so
#       the consistency pass can detect UNSAT; different-key/base stays independent.
#   (3) STRUCTURAL: SSA reassignment of the base produces a PROVEN (not false-refused).
#
# Non-literal key LOUDLY REFUSES (ValueError routed to warn-and-skip).


def test_subscript_literal_string_key_lifts_positive():
    # POSITIVE: ``parsed['key'] == 'value'`` is a liftable subscript-index.
    # Use Pattern 3 (all asserts, >= 2 atoms) so there's no call-result and
    # the subscript assert is the direct subject.
    out = _lift("""
        def test_subscript(parsed):
            assert parsed['kind'] == 'contract'
            assert parsed['name'] == 'demo'
    """)
    # Pattern 3 (characterization): two subscript asserts conjoined.
    assert out.characterization_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    assert out.lifted == 1
    assert len(out.decls) == 1
    atoms = _flatten_and(out.decls[0].inv)
    assert len(atoms) == 2, f"expected 2 atoms: {atoms}"
    # First atom: subscript(parsed, 'kind') == 'contract'
    atom0 = atoms[0]
    assert isinstance(atom0, _Atomic) and atom0.name == "="
    lhs = atom0.args[0]
    assert isinstance(lhs, _Ctor), f"LHS must be a Ctor: {lhs!r}"
    assert lhs.name == "subscript", f"LHS Ctor name must be 'subscript': {lhs.name!r}"
    assert len(lhs.args) == 2, f"subscript Ctor must have 2 args: {lhs.args}"
    # First arg is the base var (parsed).
    assert isinstance(lhs.args[0], _Var), f"base must be a Var: {lhs.args[0]!r}"
    # Second arg is the key (a string const).
    assert isinstance(lhs.args[1], _ConstStr), f"key must be a ConstStr: {lhs.args[1]!r}"
    assert lhs.args[1].value == "kind", f"key value must be 'kind': {lhs.args[1].value!r}"


def test_subscript_literal_int_key_lifts_positive():
    # POSITIVE: integer literal key also lifts.
    # Use Pattern 3 (all asserts, 2 atoms) with a parameter base to avoid
    # Pattern 5 claiming the body.
    out = _lift("""
        def test_subscript_int_key(items):
            assert items[0] == 42
            assert items[1] == 43
    """)
    assert out.characterization_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    atoms = _flatten_and(out.decls[0].inv)
    atom0 = atoms[0]
    assert isinstance(atom0, _Atomic) and atom0.name == "="
    lhs = atom0.args[0]
    assert isinstance(lhs, _Ctor)
    assert lhs.name == "subscript"
    # Key is an integer const.
    assert isinstance(lhs.args[1], _ConstInt), f"key must be ConstInt: {lhs.args[1]!r}"
    assert lhs.args[1].value == 0


def test_subscript_same_key_same_base_conjoined():
    # DISCRIMINATION: two asserts about the same subscript subject conjoin.
    # ``parsed['k'] == 'a'; parsed['k'] == 'b'`` -> single inv with 2 atoms.
    # Same subscript Ctor in both atoms (same base var, same key) -> if 'a' ≠ 'b'
    # this is UNSAT; the consistency pass detects it.  Here we just verify the
    # two atoms land in ONE ContractDecl (Pattern 3 conjunction).
    out = _lift("""
        def test_same_subscript_two_asserts(parsed):
            assert parsed['k'] == 'a'
            assert parsed['k'] == 'b'
    """)
    # Pattern 3 (all asserts, >= 2): one conjoined decl.
    assert out.characterization_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    assert len(out.decls) == 1
    atoms = _flatten_and(out.decls[0].inv)
    assert len(atoms) == 2, f"expected 2 atoms: {atoms}"
    # Both atoms must have the SAME subscript Ctor as their LHS.
    def _lhs_ctor(atom):
        assert isinstance(atom, _Atomic) and atom.name == "="
        return atom.args[0]
    lhs0 = _lhs_ctor(atoms[0])
    lhs1 = _lhs_ctor(atoms[1])
    assert isinstance(lhs0, _Ctor) and lhs0.name == "subscript"
    assert isinstance(lhs1, _Ctor) and lhs1.name == "subscript"
    # Same base var name and same key.
    assert lhs0.args[0] == lhs1.args[0], "base var must match (same SSA)"
    assert lhs0.args[1] == lhs1.args[1], "key term must match"


def test_subscript_different_keys_independent():
    # DISCRIMINATION: ``parsed['a'] == 1; parsed['b'] == 1`` — different keys
    # -> different Ctor args -> independent atoms (no sharing).
    out = _lift("""
        def test_different_keys(parsed):
            assert parsed['a'] == 1
            assert parsed['b'] == 1
    """)
    assert out.characterization_lifted == 1
    atoms = _flatten_and(out.decls[0].inv)
    assert len(atoms) == 2
    lhs0 = atoms[0].args[0]
    lhs1 = atoms[1].args[0]
    assert isinstance(lhs0, _Ctor) and lhs0.name == "subscript"
    assert isinstance(lhs1, _Ctor) and lhs1.name == "subscript"
    # Different keys.
    assert lhs0.args[1] != lhs1.args[1], "different keys must produce different key terms"


def test_subscript_different_bases_independent():
    # DISCRIMINATION: ``a['k'] == 1; b['k'] == 1`` — different bases -> different Ctors.
    out = _lift("""
        def test_different_bases(a, b):
            assert a['k'] == 1
            assert b['k'] == 1
    """)
    assert out.characterization_lifted == 1
    atoms = _flatten_and(out.decls[0].inv)
    assert len(atoms) == 2
    lhs0 = atoms[0].args[0]
    lhs1 = atoms[1].args[0]
    assert isinstance(lhs0, _Ctor) and lhs0.name == "subscript"
    assert isinstance(lhs1, _Ctor) and lhs1.name == "subscript"
    # Different base vars.
    assert lhs0.args[0] != lhs1.args[0], "different bases must produce different base terms"


def test_subscript_ssa_reassign_produces_distinct_base():
    # STRUCTURAL (SSA): reassigning the base variable between two subscript
    # asserts must produce DISTINCT base terms so the atoms are independent
    # (not a false contradiction).
    #   parsed = f()            -> parsed$0
    #   assert parsed['k'] == 1 -> subscript(parsed$0, k) == 1
    #   parsed = g()            -> parsed$1
    #   assert parsed['k'] == 2 -> subscript(parsed$1, k) == 2
    # The two base vars (parsed$0 vs parsed$1) are distinct -> independent atoms.
    #
    # Note: ``f()`` and ``g()`` are translatable calls so Pattern 5 (value-scope)
    # claims this body and emits one ``::assertion`` contract per call-site base.
    # The two assertion contracts must have DIFFERENT base SSA vars in their
    # subscript Ctors.
    out = _lift("""
        def test_ssa_reassign():
            parsed = f()
            assert parsed['k'] == 1
            parsed = g()
            assert parsed['k'] == 2
    """)
    assert out.value_scope_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    # Extract the two ::assertion decls and check their subscript base vars.
    assertion_decls = [d for d in out.decls if d.name.endswith("::assertion")]
    assert len(assertion_decls) == 2, (
        f"expected 2 assertion decls, got {[d.name for d in assertion_decls]}"
    )
    def _subscript_base_name(decl):
        inv = decl.inv
        assert isinstance(inv, _Atomic) and inv.name == "="
        lhs = inv.args[0]
        assert isinstance(lhs, _Ctor) and lhs.name == "subscript", (
            f"expected subscript Ctor in {decl.name}: {lhs!r}"
        )
        base = lhs.args[0]
        assert isinstance(base, _Var), f"base must be Var: {base!r}"
        return base.name
    base0 = _subscript_base_name(assertion_decls[0])
    base1 = _subscript_base_name(assertion_decls[1])
    assert base0 != base1, (
        f"SSA-reassigned base must produce distinct var names: "
        f"{base0!r} vs {base1!r}"
    )
    # Both decls must use the same literal key term.
    def _subscript_key(decl):
        return decl.inv.args[0].args[1]
    assert _subscript_key(assertion_decls[0]) == _subscript_key(assertion_decls[1]), (
        "same literal key 'k' must produce equal key terms in both decls"
    )


def test_subscript_non_literal_key_loudly_refused():
    # Non-literal key (variable key ``parsed[i]``) LOUDLY REFUSES: emits a
    # warning and produces no contract for this assert.  The test body has a
    # binding (making it mixed-body); the unliftable assert causes a skip+warn.
    out = _lift("""
        def test_non_literal_key():
            parsed = json_parse(raw)
            assert parsed[i] == 1
    """)
    # The assert is unliftable (non-literal key) -> 0 lifted atoms -> mixed-body skipped.
    assert out.mixed_body_lifted == 0, f"unexpected lift: {[d.name for d in out.decls]}"
    assert out.mixed_body_skipped == 1
    # A warning must explain why (non-literal key / not soundly liftable).
    assert any(
        "non-literal key" in w.reason.lower()
        or "subscript" in w.reason.lower()
        or "not soundly liftable" in w.reason.lower()
        for w in out.warnings
    ), f"expected non-literal-key warning, got: {[w.reason for w in out.warnings]}"


def test_subscript_nested_two_levels_lifts():
    # Nested subscript (``parsed['header']['kind']``) lifts as nested Ctors:
    #   subscript(subscript(parsed, 'header'), 'kind') == 'contract'
    # Use Pattern 3 (parameter base, 2 asserts) to stay out of Pattern 5.
    out = _lift("""
        def test_nested(parsed):
            assert parsed['header']['kind'] == 'contract'
            assert parsed['header']['name'] == 'demo'
    """)
    assert out.characterization_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    atoms = _flatten_and(out.decls[0].inv)
    atom0 = atoms[0]
    assert isinstance(atom0, _Atomic) and atom0.name == "="
    lhs = atom0.args[0]
    # Outer subscript.
    assert isinstance(lhs, _Ctor) and lhs.name == "subscript"
    # Inner subscript as first arg of outer.
    inner = lhs.args[0]
    assert isinstance(inner, _Ctor) and inner.name == "subscript", (
        f"nested subscript outer-base must also be a subscript Ctor: {inner!r}"
    )
    # Outer key is 'kind', inner key is 'header'.
    assert isinstance(lhs.args[1], _ConstStr) and lhs.args[1].value == "kind"
    assert isinstance(inner.args[1], _ConstStr) and inner.args[1].value == "header"
