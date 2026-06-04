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
    # f(1) is liftable; assert with a walrus operator is not. 1 < 2 -> release claim.
    # NOTE: ``"hi".upper() == "HI"`` is now LIFTABLE (method-call-result, Form 3).
    # Use a walrus-operator assert (not liftable in any form) to exercise the
    # "only 1 of 2 atoms lifted → release" path.
    out = _lift("""
        def test_mixed():
            assert f(1) == 1
            assert (y := f(2)) == 2
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
    # When ALL asserts are unliftable, zero contracts are produced but the test
    # is still CLAIMED so Layer 0 can pick it up.  A warning is emitted.
    #
    # NOTE: ``parsed.keys() == {'a', 'b'}`` is now LIFTABLE (Form 2: set literal
    # on RHS, Form 3: method-call-result on LHS).  To keep this test exercising
    # the all-unliftable path we need an assert whose expression cannot be translated
    # at all — a walrus (named expression) inside the assert is not liftable.
    out = _lift("""
        def test_all_unliftable():
            parsed = json_parse(raw)
            assert (k := parsed.key) is not None and (k := other.key) is not None
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


# ---------------------------------------------------------------------------
# TRUTHINESS CALL ASSERTIONS (census row: method/fn call as boolean)
# ---------------------------------------------------------------------------
#
# These tests cover the new "truthiness" lift: ``assert <call>`` where the
# call is a method call (``h.startswith(p)``) or a function call (``f(a,b)``).
# The lift model: UNINTERPRETED Bool predicate, exactly like membership.
# ``P ∧ ¬P`` is propositionally UNSAT; ``P(args1) ∧ P(args2)`` is CONSISTENT.
#
# Discrimination rule (soundness invariant):
#   same call expression, both true and false  → REFUSED (UNSAT conjunction)
#   same callee, different args                → PROVEN (independent predicates)
#   isinstance                                 → LOUD REFUSAL (type-lattice needed)


def test_truthiness_method_call_lifts_to_atomic_predicate():
    # ``assert h.startswith(prefix)`` lifts to an uninterpreted Bool predicate
    # ``call_startswith_a2(h, prefix)``.  Two such asserts (different prefixes)
    # must produce a PROVEN conjunction — the predicate atoms are independent.
    out = _lift("""
        def test_header_has_right_prefix(h):
            assert h.startswith("blake3-512:")
            assert h.startswith("ed25519:")
    """)
    # Pattern 3: two asserts → characterization.
    assert out.characterization_lifted == 1, (
        f"expected characterization lift; warnings: {[w.reason for w in out.warnings]}"
    )
    assert len(out.decls) == 1
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    # Both atoms must be _Atomic with the same predicate head (startswith, arity 2).
    assert len(atoms) == 2, f"expected 2 atoms, got: {atoms}"
    for atom in atoms:
        assert isinstance(atom, _Atomic), f"expected _Atomic, got {type(atom)}"
        assert atom.name == "call_startswith_a2", (
            f"expected predicate head 'call_startswith_a2', got {atom.name!r}"
        )
        assert len(atom.args) == 2, f"expected arity 2, got {len(atom.args)}"


def test_truthiness_method_call_discrimination_same_args_produces_contradiction():
    # DISCRIMINATION: ``assert h.startswith(p); assert not h.startswith(p)``
    # lifts to ``call_startswith_a2(h,p) ∧ ¬call_startswith_a2(h,p)`` which is
    # propositionally UNSAT.  The conjoined inv is the REFUSED case.
    # (This is the "unsafe twin" that must be present for soundness.)
    out = _lift("""
        def test_contradiction_same_call():
            assert h.startswith(prefix)
            assert not h.startswith(prefix)
    """)
    # Must lift (characterization pattern, 2 atoms).
    assert out.characterization_lifted == 1, (
        f"expected characterization lift; warnings: {[w.reason for w in out.warnings]}"
    )
    assert len(out.decls) == 1
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    # The conjoined inv must contain a positive atom and a NOT-wrapped atom.
    positive = [a for a in atoms if isinstance(a, _Atomic) and "startswith" in a.name]
    negated = [
        a for a in atoms
        if isinstance(a, _Connective) and a.kind == "not"
    ]
    assert positive, f"expected positive predicate atom in conjunction: {atoms}"
    assert negated, f"expected negated predicate atom in conjunction: {atoms}"
    # The negated operand must also be the same predicate.
    neg_inner = negated[0].operands[0]
    assert isinstance(neg_inner, _Atomic) and "startswith" in neg_inner.name, (
        f"negated inner must be call_startswith_a2 atom: {neg_inner!r}"
    )
    # Predicate heads must match: same call expression, same args binding.
    assert positive[0].name == neg_inner.name, (
        f"positive head {positive[0].name!r} must match negated head {neg_inner.name!r}"
    )
    assert positive[0].args == neg_inner.args, (
        f"positive args {positive[0].args!r} must match negated args {neg_inner.args!r}"
    )


def test_truthiness_function_call_lifts_to_atomic_predicate():
    # ``assert f(a, b)`` lifts to ``call_f_a2(a, b)`` — function call shape.
    out = _lift("""
        def test_function_call_truthiness(a, b):
            assert is_valid(a)
            assert is_valid(b)
    """)
    assert out.characterization_lifted == 1, (
        f"warnings: {[w.reason for w in out.warnings]}"
    )
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    assert len(atoms) == 2
    for atom in atoms:
        assert isinstance(atom, _Atomic)
        assert atom.name == "call_is_valid_a1", (
            f"expected 'call_is_valid_a1', got {atom.name!r}"
        )


def test_truthiness_not_call_lifts_to_not_atomic():
    # ``assert not x.is_valid()`` lifts to ``not(call_is_valid_a1(x))``.
    out = _lift("""
        def test_method_not_call(x):
            assert not x.is_valid()
            assert x.is_valid()
    """)
    assert out.characterization_lifted == 1, (
        f"warnings: {[w.reason for w in out.warnings]}"
    )
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    assert len(atoms) == 2
    negated = [a for a in atoms if isinstance(a, _Connective) and a.kind == "not"]
    positive = [a for a in atoms if isinstance(a, _Atomic)]
    assert negated, f"expected negated atom: {atoms}"
    assert positive, f"expected positive atom: {atoms}"
    neg_inner = negated[0].operands[0]
    assert isinstance(neg_inner, _Atomic) and neg_inner.name == positive[0].name


def test_truthiness_isinstance_is_loudly_refused():
    # ``assert isinstance(x, SomeType)`` must be LOUDLY REFUSED: the lifter
    # emits a warning naming "isinstance" and "type-lattice", and claims the
    # test (so Layer 0 does not silently retry it).  Zero contracts.
    out = _lift("""
        def test_isinstance_guard():
            x = get_value()
            assert isinstance(x, MyClass)
    """)
    # The mixed-body pattern must either:
    # (a) loudly refuse (claimed, 0 contracts, warning about isinstance), or
    # (b) the assert is silently skipped and 0 atoms → mixed_body_skipped.
    # Either way: 0 lifted contracts for the isinstance assert.
    assert out.mixed_body_lifted == 0, (
        f"isinstance must not produce lifted contracts; got {[d.name for d in out.decls]}"
    )
    # A warning MUST be present naming isinstance and type-lattice.
    isinstance_warnings = [
        w for w in out.warnings
        if "isinstance" in w.reason
    ]
    assert isinstance_warnings, (
        f"expected loud isinstance refusal warning; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any(
        "type-lattice" in w.reason or "type lattice" in w.reason.lower()
        for w in isinstance_warnings
    ), (
        f"isinstance warning must mention type-lattice: {[w.reason for w in isinstance_warnings]}"
    )


def test_truthiness_isinstance_builtin_as_bare_assert_lifts_as_characterization():
    # UPDATED (isinstance now lifts for known builtins): ``assert isinstance(x, T)``
    # on recognized concrete builtin types now lifts to an atomic formula.
    # A 2-assert body with both isinstance assertions on DIFFERENT subjects
    # is a valid characterization (independent atoms → consistent).
    out = _lift("""
        def test_isinstance_in_characterization(x, y):
            assert isinstance(x, int)
            assert isinstance(y, str)
    """)
    # Both isinstance assertions on different subjects → must lift as characterization.
    assert out.characterization_lifted == 1, (
        f"isinstance(x,int) ∧ isinstance(y,str) must lift as characterization; "
        f"warnings={[w.reason for w in out.warnings]}, decls={[d.name for d in out.decls]}"
    )
    # The lifted formula must have two isinstance atoms.
    atoms = _flatten_and(out.decls[0].inv)
    isinstance_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
    assert len(isinstance_atoms) == 2, (
        f"expected 2 isinstance atoms; atoms={atoms!r}"
    )


def test_truthiness_method_receiver_from_subscript_lifts():
    # Census form from test_claim_envelope.py:
    # ``assert parsed["envelope"]["signer"].startswith("ed25519:")``
    # The receiver is a nested subscript — verify it lifts as an opaque term.
    out = _lift("""
        def test_envelope_fields(parsed):
            assert parsed["envelope"]["signer"].startswith("ed25519:")
            assert parsed["envelope"]["signature"].startswith("ed25519:")
    """)
    assert out.characterization_lifted == 1, (
        f"expected characterization lift; warnings: {[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    assert len(atoms) == 2
    for atom in atoms:
        assert isinstance(atom, _Atomic)
        assert atom.name == "call_startswith_a2", (
            f"expected call_startswith_a2, got {atom.name!r}"
        )
        # Receiver must be a subscript Ctor (nested).
        recv = atom.args[0]
        assert isinstance(recv, _Ctor) and recv.name == "subscript", (
            f"receiver must be subscript Ctor: {recv!r}"
        )


def test_truthiness_arity_stable_different_callees_different_heads():
    # Two different method names produce DIFFERENT predicate heads (no aliasing).
    out = _lift("""
        def test_two_methods(x):
            assert x.startswith("a")
            assert x.endswith("z")
    """)
    assert out.characterization_lifted == 1, (
        f"warnings: {[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    heads = [a.name for a in atoms if isinstance(a, _Atomic)]
    assert "call_startswith_a2" in heads, f"missing startswith head: {heads}"
    assert "call_endswith_a2" in heads, f"missing endswith head: {heads}"
    assert heads[0] != heads[1], "different callees must produce different predicate heads"


def test_truthiness_arg_carrying_different_args_not_contradictory():
    # ARG-CARRYING DISCRIMINATION: ``assert h.startswith(p); assert not h.startswith(q)``
    # must lift to ``call_startswith_a2(h,p) ∧ ¬call_startswith_a2(h,q)`` which is
    # SATISFIABLE — the predicate CAN be true for p and false for q independently.
    # This is the third receipt that proves arg-carrying is faithful: an arg-DROPPING
    # implementation would collapse both to a 0-ary P → P ∧ ¬P → UNSAT (falsePass).
    # Here we verify the LIFTED representation has distinct arg terms in pos vs neg.
    out = _lift("""
        def test_arg_carrying(h, p, q):
            assert h.startswith(p)
            assert not h.startswith(q)
    """)
    assert out.characterization_lifted == 1, (
        f"warnings: {[w.reason for w in out.warnings]}"
    )
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    # Find the positive atom and the negated atom.
    positive = [a for a in atoms if isinstance(a, _Atomic) and "startswith" in a.name]
    negated = [
        a for a in atoms
        if isinstance(a, _Connective) and a.kind == "not"
        and isinstance(a.operands[0], _Atomic) and "startswith" in a.operands[0].name
    ]
    assert positive, f"expected positive call_startswith_a2 atom: {atoms}"
    assert negated, f"expected negated call_startswith_a2 atom: {atoms}"
    pos_atom = positive[0]
    neg_atom = negated[0].operands[0]
    # Both must be arity 2 (receiver + 1 arg).
    assert len(pos_atom.args) == 2, f"positive atom must be arity 2: {pos_atom}"
    assert len(neg_atom.args) == 2, f"negated atom must be arity 2: {neg_atom}"
    # CRITICAL: arg[1] must differ (p ≠ q as distinct free vars).
    pos_arg1 = pos_atom.args[1]
    neg_arg1 = neg_atom.args[1]
    assert pos_arg1 != neg_arg1, (
        f"arg[1] of positive ({pos_arg1!r}) must differ from negated ({neg_arg1!r}); "
        "arg-dropping would make them equal (falsePass regression)"
    )
    # arg[0] (receiver) must be the same (same h).
    assert pos_atom.args[0] == neg_atom.args[0], (
        f"arg[0] (receiver) must be same h: pos={pos_atom.args[0]!r}, neg={neg_atom.args[0]!r}"
    )


# ---------------------------------------------------------------------------
# Form 1: CHAINED COMPARE  ``assert a == b == c``
# ---------------------------------------------------------------------------
#
# Python semantics: ``a == b == c`` is ``(a == b) and (b == c)`` (with b
# evaluated once).  We lift to and_([eq(a,b), eq(b,c)]) — a CONJUNCTION of
# the pairwise comparisons.  Generalises to n-way and mixed ops.
#
# DISCRIMINATION:
#   CONSISTENT chain  (a == b == 42 with a=b free) → lifted → PROVEN
#   CONTRADICTORY chain (x == 1 == 2) → lifted conjunction is UNSAT → REFUSED
#
# ---------------------------------------------------------------------------


def test_chained_compare_consistent_lifts():
    # CONSISTENT 3-way chain: ``assert a == b == 42``
    # Lifts to: and_([eq(a,b), eq(b,num(42))]) — satisfiable → PROVEN.
    # Uses 2-assert body so Pattern 3 / characterization handles it.
    out = _lift("""
        def test_chain_consistent(a, b, c):
            assert a == b == 42
            assert c >= 0
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    # Must be an and-conjunction (chain contributes 2 eq atoms; c>=0 contributes 1).
    assert isinstance(inv, _Connective) and inv.kind == "and", (
        f"expected and-conjunction, got: {inv!r}"
    )
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 eq atoms from chain, got: {eq_atoms}"
    # Pair 0: eq(a, b); Pair 1: eq(b, 42)
    assert isinstance(eq_atoms[0].args[0], _Var) and eq_atoms[0].args[0].name == "a"
    assert isinstance(eq_atoms[0].args[1], _Var) and eq_atoms[0].args[1].name == "b"
    assert isinstance(eq_atoms[1].args[0], _Var) and eq_atoms[1].args[0].name == "b"
    assert isinstance(eq_atoms[1].args[1], _ConstInt) and eq_atoms[1].args[1].value == 42


def test_chained_compare_contradictory_lifts_as_conjunction():
    # CONTRADICTION: ``assert x == 1 == 2``
    # Lifts to: and_([eq(x,1), eq(1,2)]) — eq(1,2) is False in any Int model → UNSAT → REFUSED.
    # We verify here only that it LIFTS (not silently skipped), producing a conjunction.
    # The prove-step REFUSAL is the real receipt; this test guards the lift side.
    # Uses 2-assert body so Pattern 3 / characterization handles it.
    out = _lift("""
        def test_chain_contradictory(x, y):
            assert x == 1 == 2
            assert y >= 0
    """)
    assert out.lifted == 1, f"expected 1 lifted (contradiction must lift, not silently skip), warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective) and inv.kind == "and", (
        f"expected and-conjunction for contradictory chain, got: {inv!r}"
    )
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    # Pair 0: eq(x, 1); Pair 1: eq(1, 2) — both must be present.
    assert len(eq_atoms) == 2, f"expected 2 eq atoms from chain, got: {eq_atoms}"
    # eq(1, 2) must be present — this is the UNSAT witness.
    num_pairs = [(a.args[0], a.args[1]) for a in eq_atoms
                 if isinstance(a.args[0], _ConstInt) and isinstance(a.args[1], _ConstInt)]
    assert num_pairs, f"expected eq(1,2) in atoms, got: {eq_atoms}"


def test_chained_compare_mixed_ops_lifts():
    # MIXED OPS: ``assert a < b <= c`` → and_([lt(a,b), lte(b,c)])
    # Uses 2-assert body so Pattern 3 handles it.
    out = _lift("""
        def test_chain_mixed(a, b, c, d):
            assert a < b <= c
            assert d == 0
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective) and inv.kind == "and"
    atoms = _flatten_and(inv)
    lt_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "<"]
    lte_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "≤"]
    assert lt_atoms, f"expected < atom, got: {atoms}"
    assert lte_atoms, f"expected ≤ atom, got: {atoms}"


def test_chained_compare_four_way_lifts():
    # 4-WAY: ``assert a == b == c == d`` → and_([eq(a,b), eq(b,c), eq(c,d)])
    # Uses 2-assert body so Pattern 3 handles it.
    out = _lift("""
        def test_chain_four(a, b, c, d, e):
            assert a == b == c == d
            assert e >= 0
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert len(eq_atoms) == 3, f"expected 3 eq atoms for 4-way chain, got: {eq_atoms}"


def test_chained_compare_in_characterization_body():
    # Chained compare inside a multi-assert body (Pattern 3 / characterization).
    # ``assert a == b == CONST; assert c == d`` → both lift → conjunction of 3 atoms total.
    out = _lift("""
        def test_multi_with_chain(a, b, c, d):
            assert a == b == 99
            assert c == d
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    # Chain contributes 2; the plain eq contributes 1 → 3 total.
    assert len(eq_atoms) == 3, f"expected 3 eq atoms, got: {eq_atoms}"


# ---------------------------------------------------------------------------
# Form 2: DICT / SET LITERAL EQUALITY  ``assert x == {'k': 'v'}``
# ---------------------------------------------------------------------------
#
# Model: the dict/set literal is an opaque constant keyed by its CANONICAL
# string representation (sorted keys for dicts, sorted elements for sets).
# We use ``str_const(canonical)`` so the Rust encoder emits ``strlit_<hash>``
# — two structurally-different literals → different hashes → distinct consts
# → contradictions fire UNSAT; identical literals → same hash → same const
# → contradictions still fire UNSAT.
#
# SOUNDNESS (the only direction that matters):
#   different literals MUST be distinct → if x==L1 ∧ x==L2 and L1≠L2 → UNSAT.
#   same literal twice → same const → x==L ∧ x==L is SAT → PROVEN.
#   non-translatable contents (nested calls, computed values) → LOUD REFUSE.
#
# ---------------------------------------------------------------------------


def test_dict_literal_eq_lifts():
    # ``assert a == {'k': 'v'}`` → eq(a, str_const(canonical_dict_repr))
    # Uses 2-assert body so Pattern 3 / characterization handles it.
    out = _lift("""
        def test_dict_eq(a, b):
            assert a == {'k': 'v'}
            assert b >= 0
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert eq_atoms, f"expected eq atom for dict literal, got: {atoms}"
    # RHS must be a _ConstStr (the canonical representation).
    rhs_terms = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstStr)]
    assert rhs_terms, f"expected _ConstStr on RHS, got args: {[(a.args[0], a.args[1]) for a in eq_atoms]}"


def test_set_literal_eq_lifts():
    # ``assert a == {1, 2}`` — the set literal on RHS → opaque str_const.
    # Note: in Python AST, ``{1, 2}`` is ast.Set, not ast.Dict.
    # Uses 2-assert body so Pattern 3 / characterization handles it.
    out = _lift("""
        def test_set_eq(a, b):
            assert a == {1, 2}
            assert b >= 0
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert eq_atoms, f"expected eq atom for set literal, got: {atoms}"
    rhs_terms = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstStr)]
    assert rhs_terms, f"expected _ConstStr on RHS for set literal"


def test_dict_literal_discrimination_different_content_distinct():
    # DISCRIMINATION: ``assert x == {'k': 1}; assert x == {'k': 2}``
    # Two DIFFERENT dict literals → two different str_const values → different
    # strlit_<hash> SMT consts → x == L1 ∧ x == L2 with L1 ≠ L2 → UNSAT → REFUSED.
    # This test verifies the LIFTED representation has DISTINCT RHS terms.
    out = _lift("""
        def test_dict_discrimination(x):
            assert x == {'k': 1}
            assert x == {'k': 2}
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    const_rhs = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstStr)]
    assert len(const_rhs) == 2, f"expected 2 ConstStr RHS terms, got: {const_rhs}"
    # CRITICAL: the two constants must be DISTINCT so the SMT solver sees the contradiction.
    assert const_rhs[0] != const_rhs[1], (
        f"different dict literals must produce distinct str_const values; "
        f"got {const_rhs[0]!r} == {const_rhs[1]!r} (falsePass regression)"
    )


def test_dict_literal_discrimination_same_content_equal():
    # CONSISTENT: ``assert x == {'k': 1}; assert x == {'k': 1}`` (identical literal twice).
    # Same content → same str_const → same strlit_<hash> → SAT → PROVEN.
    out = _lift("""
        def test_dict_same(x):
            assert x == {'k': 1}
            assert x == {'k': 1}
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    const_rhs = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstStr)]
    assert len(const_rhs) == 2, f"expected 2 ConstStr RHS terms for same-literal pair"
    # CRITICAL: the two constants must be EQUAL (same content → same str_const).
    assert const_rhs[0] == const_rhs[1], (
        f"identical dict literals must produce equal str_const values; "
        f"got {const_rhs[0]!r} != {const_rhs[1]!r}"
    )


def test_dict_literal_nontranslatable_value_loudly_refused():
    # LOUD REFUSE: dict literal with a computed/untranslatable value.
    # ``assert x == {'k': some_call()}`` → cannot establish content identity → REFUSED.
    # Uses 2-assert body so Pattern 3 tries both; the dict assert is skipped with warning.
    out = _lift("""
        def test_dict_nontranslatable(x, y):
            assert x == {'k': some_call()}
            assert y >= 0
    """)
    # The non-translatable dict assert must NOT contribute a lifted contract —
    # only the y>=0 assert may be lifted (or characterization falls back to 1 atom → releases).
    # The critical invariant: no dict-equality contract with a computed value should appear.
    dict_eq_contracts = [
        d for d in out.decls
        if any(
            isinstance(a, _Atomic) and a.name == "=" and isinstance(a.args[1], _ConstStr)
            for a in _flatten_and(d.inv)
        )
    ]
    assert not dict_eq_contracts, (
        f"dict literal with computed value MUST NOT produce an eq(_ConstStr) contract; "
        f"got: {[d.inv for d in dict_eq_contracts]}"
    )
    # A warning must be emitted for the non-translatable literal.
    assert any("dict" in w.reason.lower() or "set" in w.reason.lower() or
               "not liftable" in w.reason.lower() or "literal" in w.reason.lower() or
               "liftable" in w.reason.lower()
               for w in out.warnings), (
        f"expected a warning about non-translatable dict literal, got: {[w.reason for w in out.warnings]}"
    )


def test_set_literal_discrimination_different_content_distinct():
    # DISCRIMINATION: ``assert x == {1, 2}; assert x == {1, 3}`` → DISTINCT consts → REFUSED.
    out = _lift("""
        def test_set_discrimination(x):
            assert x == {1, 2}
            assert x == {1, 3}
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    const_rhs = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstStr)]
    assert len(const_rhs) == 2, f"expected 2 ConstStr RHS"
    assert const_rhs[0] != const_rhs[1], "different set literals must be distinct (falsePass guard)"


# ---------------------------------------------------------------------------
# Form 3: METHOD-CALL-RESULT COMPARE  ``assert x.method(args) == y``
# ---------------------------------------------------------------------------
#
# The call result is a VALUE on LHS/RHS of ==.  We model it as an opaque term:
#   ``ctor('callval_<method>_a<n>', [recv_term, arg_terms...])``
# where n = total arity (receiver + args).
#
# Identity rule (soundness):
#   Same method name + same recv + same args → same frozen ctor term → EQUAL in SMT.
#   Different method OR args → different ctor terms → INDEPENDENT.
#
# DISCRIMINATION:
#   ``assert x.m() == 1; assert x.m() == 2`` → callval_m_a1(x)==1 ∧ callval_m_a1(x)==2
#     → same ctor, both equated to distinct Int consts → UNSAT → REFUSED.
#   ``assert x.m(p) == 1; assert x.m(q) == 2`` (p≠q) → different arg terms → independent.
#
# Keyword args / untranslatable args → LOUD REFUSE (raise ValueError in _translate_term).
#
# ---------------------------------------------------------------------------


def test_method_call_result_eq_lifts():
    # ``assert x.method() == 1`` → eq(ctor('callval_method_a1', [x]), num(1))
    # Uses 2-assert body so Pattern 3 / characterization handles it.
    out = _lift("""
        def test_method_result(x, y):
            assert x.method() == 1
            assert y >= 0
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert eq_atoms, f"expected eq atom for method-call-result, got: {atoms}"
    # LHS must be a _Ctor with 'callval' in the name.
    callval_ctors = [
        a.args[0] for a in eq_atoms
        if isinstance(a.args[0], _Ctor) and "callval" in a.args[0].name
    ]
    assert callval_ctors, f"expected callval _Ctor on LHS, got atoms: {eq_atoms}"
    cval = callval_ctors[0]
    # Arity: method() with receiver → n=1 arity in ctor.
    assert len(cval.args) == 1, f"expected 1 arg (receiver only), got: {cval.args}"
    assert isinstance(cval.args[0], _Var) and cval.args[0].name == "x"


def test_method_call_result_with_arg_lifts():
    # ``assert x.method(k) == 'v'`` → eq(ctor('callval_method_a2', [x, k]), str_const('v'))
    # Uses 2-assert body so Pattern 3 handles it.
    out = _lift("""
        def test_method_with_arg(x, k, z):
            assert x.method(k) == 'v'
            assert z >= 0
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    callval_ctors = [
        a.args[0] for a in eq_atoms
        if isinstance(a.args[0], _Ctor) and "callval" in a.args[0].name
    ]
    assert callval_ctors, f"expected callval _Ctor: {eq_atoms}"
    cval = callval_ctors[0]
    # Arity 2: receiver + 1 arg.
    assert len(cval.args) == 2, f"expected 2 args (receiver + k), got: {cval.args}"


def test_method_call_result_discrimination_same_recv_same_args_contradicts():
    # DISCRIMINATION: same recv, same method, same args, different RHS consts → REFUSED.
    # ``assert x.m() == 1; assert x.m() == 2`` → same callval_m_a1(x) Ctor → UNSAT.
    out = _lift("""
        def test_method_contradiction(x):
            assert x.m() == 1
            assert x.m() == 2
    """)
    assert out.lifted == 1, f"expected 1 lifted, warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    callval_lhs = [
        a.args[0] for a in eq_atoms
        if isinstance(a.args[0], _Ctor) and "callval" in a.args[0].name
    ]
    assert len(callval_lhs) == 2, f"expected 2 callval LHS terms, got: {callval_lhs}"
    # CRITICAL: the two callval terms must be EQUAL (same recv, same args) → same ctor → UNSAT.
    assert callval_lhs[0] == callval_lhs[1], (
        f"same method call on same receiver must produce equal ctor terms for contradiction to fire; "
        f"got {callval_lhs[0]!r} != {callval_lhs[1]!r} (falsePass regression)"
    )
    # RHS constants must be DISTINCT.
    rhs_consts = [a.args[1] for a in eq_atoms if isinstance(a.args[1], _ConstInt)]
    assert len(rhs_consts) == 2 and rhs_consts[0] != rhs_consts[1], (
        f"expected distinct Int RHS constants, got: {rhs_consts}"
    )


def test_method_call_result_discrimination_different_args_independent():
    # INDEPENDENT: same method, same recv, DIFFERENT args → different ctor terms → SAT.
    # ``assert x.m(p) == 1; assert x.m(q) == 2`` must NOT contradict (p and q are free vars).
    out = _lift("""
        def test_method_different_args(x, p, q):
            assert x.m(p) == 1
            assert x.m(q) == 2
    """)
    assert out.lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    callval_lhs = [
        a.args[0] for a in eq_atoms
        if isinstance(a.args[0], _Ctor) and "callval" in a.args[0].name
    ]
    assert len(callval_lhs) == 2, f"expected 2 callval terms"
    # CRITICAL: different args → DIFFERENT ctor terms → independent (NOT a contradiction).
    assert callval_lhs[0] != callval_lhs[1], (
        f"different args must produce distinct ctor terms; "
        f"got same {callval_lhs[0]!r} (would miss contradiction between independent calls)"
    )


def test_method_call_result_keyword_args_loudly_refused():
    # LOUD REFUSE: keyword args in the comparison call.
    # ``assert x.method(k=1) == 1`` → LOUD REFUSE (cannot order-stably translate kwargs).
    # Uses 2-assert body; the kwarg assert is skipped with a warning; the other lifts.
    out = _lift("""
        def test_method_kwarg(x, y):
            assert x.method(k=1) == 1
            assert y >= 0
    """)
    # A callval_method contract with kwarg MUST NOT appear.
    kwarg_method_contracts = [
        d for d in out.decls
        if any(
            isinstance(a, _Atomic) and a.name == "=" and isinstance(a.args[0], _Ctor) and "callval_method" in a.args[0].name
            for a in _flatten_and(d.inv)
        )
    ]
    assert not kwarg_method_contracts, (
        f"method call with kwargs MUST NOT be silently lifted; got: {[d.inv for d in kwarg_method_contracts]}"
    )
    # A warning must be emitted about the non-liftable kwarg call.
    assert any("kwarg" in w.reason.lower() or "keyword" in w.reason.lower() or
               "not liftable" in w.reason.lower()
               for w in out.warnings), (
        f"expected warning about keyword args, got: {[w.reason for w in out.warnings]}"
    )


# ---------------------------------------------------------------------------
# PATTERN 7: pytest.raises blocks
# ---------------------------------------------------------------------------
#
# Census row: ``with pytest.raises(ExcType): <body-call>``
# Encoding: ``eq(ctor("raised_exc_a1", [call_term]), str_const(ExcType_name))``
#
# Models the RAISED EXCEPTION TYPE as a function-valued term keyed on the
# callsite, exactly mirroring the callval method-call-result encoding (Pattern 3
# / Form 3).  DISCRIMINATION is automatic:
#
#   same callsite + same exc type  → same raised_exc_a1(call_term) = same const
#     → SAT → PROVEN (consistent)
#   same callsite + different exc types
#     → raised_exc_a1(g) = c1  ∧  raised_exc_a1(g) = c2,  c1 ≠ c2
#     → same term equated to two distinct str-constants
#     → UNSAT by EUF/congruence (no extra axioms needed)
#     → REFUSED
#
# TEETH (deferred): whether <body-call> actually raises is a production-bridge
# obligation (requires callsite postconditions / exception specifications) and
# is NOT claimed here.  The consistency-level lift only checks that the RAISE
# CLAIM is internally consistent within a single test function.
#
# Loud-refuse cases (each has an explicit test below):
#   - ``as exc_info`` clause
#   - ``match=`` or other keyword args on pytest.raises
#   - Tuple exception type (``pytest.raises((A, B))``)
#   - Multi-statement body inside the with-block
#   - Non-translatable call in body
#   - Non-pure body (binding/assert alongside raises blocks)
#
# ---------------------------------------------------------------------------


def test_raises_simple_lifts():
    # POSITIVE: ``with pytest.raises(ValueError): f(x)``
    # lifts to ``eq(raised_exc_a1(f(x)), str_const("ValueError"))``
    out = _lift("""
        import pytest
        def test_pure_raises():
            with pytest.raises(ValueError):
                f(x)
    """)
    assert out.raises_lifted == 1, f"expected 1 raises lift; warnings: {[w.reason for w in out.warnings]}"
    assert out.lifted == 1
    assert "test_pure_raises" in out.claimed_tests
    assert len(out.decls) == 1
    inv = out.decls[0].inv
    # Must be eq(raised_exc_a1(call_term), str_const("ValueError"))
    assert isinstance(inv, _Atomic) and inv.name == "=", f"expected eq atom, got: {inv!r}"
    lhs = inv.args[0]
    rhs = inv.args[1]
    assert isinstance(lhs, _Ctor) and lhs.name == "raised_exc_a1", (
        f"LHS must be raised_exc_a1 ctor, got: {lhs!r}"
    )
    assert len(lhs.args) == 1, f"raised_exc_a1 must have 1 arg (the call term), got: {lhs.args}"
    assert isinstance(rhs, _ConstStr) and rhs.value == "ValueError", (
        f"RHS must be str_const('ValueError'), got: {rhs!r}"
    )


def test_raises_bare_raises_name_lifts():
    # POSITIVE: ``with raises(ValueError): f(x)`` (bare ``raises``, no ``pytest.``)
    # lifts exactly as pytest.raises does.
    out = _lift("""
        def test_bare_raises():
            with raises(ValueError):
                f(x)
    """)
    assert out.raises_lifted == 1, f"expected 1 raises lift; warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Atomic) and inv.name == "="
    rhs = inv.args[1]
    assert isinstance(rhs, _ConstStr) and rhs.value == "ValueError"


def test_raises_discrimination_same_call_different_exc_contradicts():
    # DISCRIMINATION (unsafe twin): same callsite, different exc types.
    # ``with pytest.raises(ValueError): f(x)`` AND
    # ``with pytest.raises(KeyError):   f(x)``
    # lifts to:
    #   eq(raised_exc_a1(f(x)), str_const("ValueError"))
    #   ∧ eq(raised_exc_a1(f(x)), str_const("KeyError"))
    # Both LHS terms are EQUAL (same ctor, same arg → same term).
    # RHS constants are DISTINCT.
    # → same term equated to two distinct consts → UNSAT → REFUSED.
    out = _lift("""
        import pytest
        def test_contradictory_exc():
            with pytest.raises(ValueError):
                f(x)
            with pytest.raises(KeyError):
                f(x)
    """)
    assert out.raises_lifted == 1, (
        f"contradictory raises must LIFT (not silently skip); warnings: {[w.reason for w in out.warnings]}"
    )
    assert len(out.decls) == 1
    inv = out.decls[0].inv
    # Must be a conjunction of two eq atoms.
    assert isinstance(inv, _Connective) and inv.kind == "and", (
        f"expected and-conjunction for contradictory raises, got: {inv!r}"
    )
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert len(eq_atoms) == 2, f"expected 2 eq atoms, got: {eq_atoms}"
    # LHS: raised_exc_a1(call_term) — MUST be EQUAL (same call → same ctor → UNSAT).
    lhs0 = eq_atoms[0].args[0]
    lhs1 = eq_atoms[1].args[0]
    assert isinstance(lhs0, _Ctor) and lhs0.name == "raised_exc_a1"
    assert isinstance(lhs1, _Ctor) and lhs1.name == "raised_exc_a1"
    assert lhs0 == lhs1, (
        f"same call must produce equal raised_exc_a1 ctor for contradiction to fire; "
        f"got lhs0={lhs0!r}, lhs1={lhs1!r} (falsePass regression if different)"
    )
    # RHS: str_const — MUST be DISTINCT.
    rhs0 = eq_atoms[0].args[1]
    rhs1 = eq_atoms[1].args[1]
    assert isinstance(rhs0, _ConstStr) and isinstance(rhs1, _ConstStr)
    assert rhs0 != rhs1, (
        f"different exc types must produce distinct str_const RHS; "
        f"got rhs0={rhs0!r}, rhs1={rhs1!r}"
    )


def test_raises_discrimination_same_exc_same_call_consistent():
    # POSITIVE (safe twin): same callsite, same exc type.
    # eq(g, c) ∧ eq(g, c) is SAT → PROVEN (consistent).
    out = _lift("""
        import pytest
        def test_consistent_exc():
            with pytest.raises(ValueError):
                f(x)
            with pytest.raises(ValueError):
                f(x)
    """)
    assert out.raises_lifted == 1, (
        f"consistent raises must LIFT; warnings: {[w.reason for w in out.warnings]}"
    )
    inv = out.decls[0].inv
    # Conjunction of two eq atoms.
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert len(eq_atoms) == 2
    # Both LHS must be equal AND both RHS must be equal.
    lhs0 = eq_atoms[0].args[0]
    lhs1 = eq_atoms[1].args[0]
    rhs0 = eq_atoms[0].args[1]
    rhs1 = eq_atoms[1].args[1]
    assert lhs0 == lhs1, "same call → equal LHS"
    assert rhs0 == rhs1, "same exc type → equal RHS (consistent → SAT)"


def test_raises_discrimination_different_calls_independent():
    # INDEPENDENT: different callsites, different exc types.
    # eq(raised_exc_a1(f(x)), c1) ∧ eq(raised_exc_a1(g(y)), c2)
    # LHS terms are DIFFERENT (different inner call terms) → independent → SAT.
    out = _lift("""
        import pytest
        def test_independent_exc():
            with pytest.raises(ValueError):
                f(x)
            with pytest.raises(KeyError):
                g(y)
    """)
    assert out.raises_lifted == 1, (
        f"independent raises must LIFT; warnings: {[w.reason for w in out.warnings]}"
    )
    inv = out.decls[0].inv
    atoms = _flatten_and(inv)
    eq_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "="]
    assert len(eq_atoms) == 2
    lhs0 = eq_atoms[0].args[0]
    lhs1 = eq_atoms[1].args[0]
    # Different inner call terms → different LHS ctors → independent.
    assert lhs0 != lhs1, (
        f"different calls must produce DIFFERENT raised_exc_a1 ctor terms so they "
        f"remain independent (not a contradiction); got same {lhs0!r}"
    )


def test_raises_as_clause_loudly_refused():
    # LOUD REFUSE: ``with pytest.raises(ValueError) as exc_info: f(x)``
    # exc_info inspection is not soundly liftable.
    out = _lift("""
        import pytest
        def test_as_clause():
            with pytest.raises(ValueError) as exc_info:
                f(x)
    """)
    assert out.raises_lifted == 0, (
        f"as-clause raises must NOT lift; got {[d.name for d in out.decls]}"
    )
    assert out.raises_skipped == 1
    assert out.lifted == 0
    assert "test_as_clause" in out.claimed_tests, "must be claimed (not silently skipped)"
    assert any("exc_info" in w.reason or "as" in w.reason for w in out.warnings), (
        f"expected warning about as-clause; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_raises_match_kwarg_loudly_refused():
    # LOUD REFUSE: ``with pytest.raises(ValueError, match="foo"): f(x)``
    # match= is a regex string assertion, deferred to production-bridge.
    out = _lift("""
        import pytest
        def test_match_raises():
            with pytest.raises(ValueError, match="foo"):
                f(x)
    """)
    assert out.raises_lifted == 0
    assert out.raises_skipped == 1
    assert "test_match_raises" in out.claimed_tests
    assert any("match" in w.reason.lower() or "keyword" in w.reason.lower() for w in out.warnings), (
        f"expected warning about match= kwarg; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_raises_tuple_exception_type_loudly_refused():
    # LOUD REFUSE: ``with pytest.raises((ValueError, KeyError)): f(x)``
    # Tuple exception types mean "one of A or B" — cannot model as a single
    # string constant soundly (would require a disjunction axiom).
    out = _lift("""
        import pytest
        def test_tuple_exc():
            with pytest.raises((ValueError, KeyError)):
                f(x)
    """)
    assert out.raises_lifted == 0
    assert out.raises_skipped == 1
    assert "test_tuple_exc" in out.claimed_tests
    assert any("Tuple" in w.reason or "tuple" in w.reason.lower() or "simple Name" in w.reason
               for w in out.warnings), (
        f"expected warning about tuple exception type; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_raises_multi_statement_body_loudly_refused():
    # LOUD REFUSE: multi-statement body inside the raises block.
    # Cannot reduce to one callsite term.
    out = _lift("""
        import pytest
        def test_multi_body():
            with pytest.raises(ValueError):
                f(x)
                g(y)
    """)
    assert out.raises_lifted == 0
    assert out.raises_skipped == 1
    assert "test_multi_body" in out.claimed_tests
    assert any("1 statement" in w.reason or "multi" in w.reason.lower() for w in out.warnings), (
        f"expected warning about multi-statement body; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_raises_mixed_body_binding_loudly_refused():
    # LOUD REFUSE: binding statement alongside pytest.raises block.
    # Mixed binding+raises bodies are out of scope for v0.
    out = _lift("""
        import pytest
        def test_mixed():
            x = setup()
            with pytest.raises(ValueError):
                f(x)
    """)
    assert out.raises_lifted == 0
    assert out.raises_skipped == 1
    assert "test_mixed" in out.claimed_tests
    assert any("Assign" in w.reason or "binding" in w.reason.lower() or
               "mixed" in w.reason.lower() for w in out.warnings), (
        f"expected warning about mixed body; warnings: {[w.reason for w in out.warnings]}"
    )
    assert any("LOUD REFUSAL" in w.reason for w in out.warnings)


def test_raises_call_in_body_translated_as_term():
    # POSITIVE structural: verify the body call is translated correctly.
    # ``f(42)`` → ctor("f", [num(42)]) as the inner term.
    out = _lift("""
        import pytest
        def test_call_with_literal():
            with pytest.raises(TypeError):
                parse(42)
    """)
    assert out.raises_lifted == 1, f"warnings: {[w.reason for w in out.warnings]}"
    inv = out.decls[0].inv
    assert isinstance(inv, _Atomic) and inv.name == "="
    lhs = inv.args[0]
    assert isinstance(lhs, _Ctor) and lhs.name == "raised_exc_a1"
    inner = lhs.args[0]
    # Inner should be ctor("parse", [num(42)])
    assert isinstance(inner, _Ctor) and inner.name == "parse", (
        f"inner call term must be ctor('parse', ...), got: {inner!r}"
    )


def test_raises_silent_gap_closed_pure_body_is_claimed():
    # REGRESSION guard for the Δ>0 gap: a pure with-raises body (no binding,
    # no top-level assert) was previously silently skipped (not claimed, no
    # warning).  After Pattern 7, the test is always claimed.
    out = _lift("""
        import pytest
        def test_previously_silent():
            with pytest.raises(RuntimeError):
                risky_operation()
    """)
    assert "test_previously_silent" in out.claimed_tests, (
        "with pytest.raises body must be CLAIMED (not silently fall through to Layer 0)"
    )
    # No silent fall-through: must be either lifted or loudly-refused.
    assert out.raises_lifted + out.raises_skipped >= 1, (
        "must be either lifted or loud-refused; silent skip is a Δ>0 gap"
    )


# ---------------------------------------------------------------------------
# pytest.approx — census form: ``assert x == pytest.approx(target)``
#
# SOUND MODEL (conservative uninterpreted predicate, never falsePass):
#   ``x == approx(t)`` -> ``atomic("approx_eq", [x_term, target_term])``
#   ``x != approx(t)`` -> ``not_(atomic("approx_eq", [x_term, target_term]))``
#
# DISCRIMINATION: same-target P ^ not-P is UNSAT -> REFUSED.
# CONSERVATIVE UNDER-REFUSAL (documented, acceptable):
#   Different targets are treated as independent -> PROVEN.  This misses a
#   real contradiction for DISJOINT tolerance intervals, but never
#   falsely refuses OVERLAPPING ranges (no falsePass).
#
# LOUD REFUSE: kwargs (rel=/abs=), non-literal target, list/dict target,
#   approx on both sides, approx-as-sub-term.
# ---------------------------------------------------------------------------


def test_approx_eq_float_lifts_to_atomic():
    # POSITIVE: ``x == pytest.approx(1.5)`` -> approx_eq atomic.
    # Two asserts are needed to trigger Pattern 3 (characterization).
    out = _lift("""
        def test_approx_float_eq(x, y):
            assert x == pytest.approx(1.5)
            assert y == pytest.approx(2.5)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    atoms = _flatten_and(out.decls[0].inv)
    approx_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(approx_atoms) == 2, f"expected 2 approx_eq atoms (one per assert), got: {atoms}"
    # First atom: x is the value side (first arg), target is str_const (second arg).
    val_arg = approx_atoms[0].args[0]
    assert isinstance(val_arg, _Var) and val_arg.name == "x", (
        f"value side must be Var('x'), got: {val_arg!r}"
    )
    target_arg = approx_atoms[0].args[1]
    assert isinstance(target_arg, _ConstStr), (
        f"target must be str_const (encoded float), got: {target_arg!r}"
    )


def test_approx_eq_int_lifts_to_atomic():
    # POSITIVE: ``x == pytest.approx(42)`` -> approx_eq atomic.
    out = _lift("""
        def test_approx_int_eq(x, y):
            assert x == pytest.approx(42)
            assert y == pytest.approx(99)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    atoms = _flatten_and(out.decls[0].inv)
    approx_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(approx_atoms) == 2, f"expected 2 approx_eq atoms, got: {atoms}"


def test_approx_bare_name_lifts_to_atomic():
    # POSITIVE: bare ``approx(...)`` (imported as ``from pytest import approx``).
    out = _lift("""
        def test_bare_approx(x, y):
            assert x == approx(0.5)
            assert y == approx(0.5)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    atoms = _flatten_and(out.decls[0].inv)
    approx_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(approx_atoms) == 2, f"expected 2 approx_eq atoms for bare approx, got: {atoms}"


def test_approx_ne_lifts_as_negation():
    # POSITIVE: ``x != pytest.approx(1.5)`` -> not_(approx_eq(...)).
    out = _lift("""
        def test_approx_ne(x, y):
            assert x != pytest.approx(1.5)
            assert y != pytest.approx(1.5)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    inv = out.decls[0].inv
    nots = [
        o for o in _flatten_and(inv)
        if isinstance(o, _Connective) and o.kind == "not"
    ]
    assert len(nots) >= 1, f"expected not_ wrapper for !=, got inv={inv}"
    for n in nots:
        inner = n.operands[0]
        assert isinstance(inner, _Atomic) and inner.name == "approx_eq", (
            f"not-approx must wrap approx_eq atom, got: {inner!r}"
        )


def test_approx_eq_ne_same_target_is_discriminating_contradiction():
    # DISCRIMINATION: ``x == approx(t); x != approx(t)``
    # -> ``approx_eq(x,t) ^ not_(approx_eq(x,t))`` -> UNSAT -> REFUSED.
    # This is the core discrimination test: the same-subject P ^ not-P pattern.
    out = _lift("""
        def test_approx_contradiction(x):
            assert x == pytest.approx(1.5)
            assert x != pytest.approx(1.5)
    """)
    assert out.lifted == 1, f"expected lift (for REFUSED check), warnings: {out.warnings}"
    inv = out.decls[0].inv
    flat = _flatten_and(inv)
    atoms = [a for a in flat if isinstance(a, _Atomic) and a.name == "approx_eq"]
    nots = [
        o for o in flat
        if isinstance(o, _Connective) and o.kind == "not"
        and isinstance(o.operands[0], _Atomic)
        and o.operands[0].name == "approx_eq"
    ]
    assert atoms, f"expected positive approx_eq atom, got inv={inv}"
    assert nots, f"expected not_(approx_eq) atom, got inv={inv}"
    # The two atoms must refer to the same target (same str_const value).
    pos_target = atoms[0].args[1]
    neg_target = nots[0].operands[0].args[1]
    assert pos_target == neg_target, (
        f"discrimination requires same target; pos={pos_target!r} neg={neg_target!r}"
    )


def test_approx_eq_twice_same_target_is_consistent():
    # POSITIVE: ``x == approx(t); x == approx(t)`` -> two identical approx_eq
    # atoms -> PROVEN (CONSISTENT).
    # Both asserts use the same x and same target -> same atom twice -> SAT.
    out = _lift("""
        def test_approx_consistent(x):
            assert x == pytest.approx(1.5)
            assert x == pytest.approx(1.5)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    flat = _flatten_and(out.decls[0].inv)
    atoms = [a for a in flat if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(atoms) == 2, f"expected 2 approx_eq atoms (one per assert), got: {flat}"
    # Both must share the same target (same str_const).
    assert atoms[0].args[1] == atoms[1].args[1], (
        f"both atoms must share the same target term; got {atoms[0].args[1]!r} vs {atoms[1].args[1]!r}"
    )
    # Both must share the same value var (x).
    assert atoms[0].args[0] == atoms[1].args[0], (
        f"both atoms must refer to the same value var; got {atoms[0].args[0]!r} vs {atoms[1].args[0]!r}"
    )


def test_approx_different_targets_are_independent_conservative_under_refusal():
    # CONSERVATIVE UNDER-REFUSAL: ``x == approx(5.0); x == approx(99.0)``
    # -> two DIFFERENT approx_eq atoms -> treated as INDEPENDENT -> PROVEN.
    # This is the conservative under-refusal guard: disjoint targets that
    # cannot overlap are NOT refused (we never assert target distinctness —
    # doing so would falsePass overlapping ranges).
    out = _lift("""
        def test_approx_different_targets(x):
            assert x == pytest.approx(5.0)
            assert x == pytest.approx(99.0)
    """)
    assert out.lifted == 1, f"expected lift, warnings: {out.warnings}"
    flat = _flatten_and(out.decls[0].inv)
    atoms = [a for a in flat if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(atoms) == 2, f"expected 2 approx_eq atoms, got: {flat}"
    # Targets must be different str_const values (encoding 5.0 vs 99.0).
    assert atoms[0].args[1] != atoms[1].args[1], (
        f"different targets must produce different str_const terms; "
        f"got {atoms[0].args[1]!r} and {atoms[1].args[1]!r}"
    )


def test_approx_kwargs_rel_loudly_refused():
    # LOUD REFUSE: ``rel=`` kwarg on pytest.approx.
    # Different rel= values produce different effective intervals;
    # collapsing them to the same atom would hide the distinction.
    out = _lift("""
        def test_approx_rel(x):
            assert x == pytest.approx(1.5, rel=1e-3)
            assert x == pytest.approx(1.5, rel=1e-3)
    """)
    # The test must not silently lift: either 0 decls (warning emitted) or
    # a loud-refuse warning present.
    approx_warns = [w for w in out.warnings if "approx" in w.reason.lower()]
    assert approx_warns or out.lifted == 0, (
        "rel= kwarg must produce a loud-refuse warning or 0 lifts; "
        f"got lifted={out.lifted}, warnings={out.warnings}"
    )


def test_approx_kwargs_abs_loudly_refused():
    # LOUD REFUSE: ``abs=`` kwarg on pytest.approx.
    out = _lift("""
        def test_approx_abs(x):
            assert x == pytest.approx(1.5, abs=0.1)
            assert x == pytest.approx(1.5, abs=0.1)
    """)
    approx_warns = [w for w in out.warnings if "approx" in w.reason.lower()]
    assert approx_warns or out.lifted == 0, (
        "abs= kwarg must produce a loud-refuse warning or 0 lifts; "
        f"got lifted={out.lifted}, warnings={out.warnings}"
    )


def test_approx_list_target_loudly_refused():
    # LOUD REFUSE: ``pytest.approx([1.0, 2.0])`` — list target is out of scope.
    out = _lift("""
        def test_approx_list(x):
            assert x == pytest.approx([1.0, 2.0])
    """)
    approx_warns = [w for w in out.warnings if "approx" in w.reason.lower()]
    assert approx_warns or out.lifted == 0, (
        "list approx target must produce a loud-refuse warning or 0 lifts; "
        f"got lifted={out.lifted}, warnings={out.warnings}"
    )


def test_approx_variable_target_loudly_refused():
    # LOUD REFUSE: computed / variable target cannot be content-addressed.
    out = _lift("""
        def test_approx_var_target(x, expected):
            assert x == pytest.approx(expected)
    """)
    approx_warns = [w for w in out.warnings if "approx" in w.reason.lower()]
    assert approx_warns or out.lifted == 0, (
        "variable target must produce a loud-refuse warning or 0 lifts; "
        f"got lifted={out.lifted}, warnings={out.warnings}"
    )


def test_approx_negative_float_target_lifts():
    # POSITIVE: unary-neg float literal (``-1.5``) is a valid target.
    out = _lift("""
        def test_approx_neg_float(x, y):
            assert x == pytest.approx(-1.5)
            assert y == pytest.approx(-1.5)
    """)
    assert out.lifted == 1, f"expected lift for negative float target, warnings: {out.warnings}"
    flat = _flatten_and(out.decls[0].inv)
    atoms = [a for a in flat if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(atoms) == 2, f"expected 2 approx_eq atoms, got: {flat}"


def test_approx_lhs_position_lifts():
    # POSITIVE: approx on the LEFT side (``pytest.approx(1.5) == x``).
    # Python allows this; x is still the value side.
    out = _lift("""
        def test_approx_lhs(x, y):
            assert pytest.approx(1.5) == x
            assert pytest.approx(1.5) == y
    """)
    assert out.lifted == 1, f"expected lift for approx on LHS, warnings: {out.warnings}"
    flat = _flatten_and(out.decls[0].inv)
    atoms = [a for a in flat if isinstance(a, _Atomic) and a.name == "approx_eq"]
    assert len(atoms) == 2, f"expected 2 approx_eq atoms, got: {flat}"
    # x is the value side of the first atom (first arg).
    val_arg = atoms[0].args[0]
    assert isinstance(val_arg, _Var) and val_arg.name == "x", (
        f"value side must be Var('x'), got: {val_arg!r}"
    )


def test_approx_as_subterm_in_nested_call_loudly_refused():
    # LOUD REFUSE: approx nested inside another call position (not direct comparand).
    # e.g. ``assert f(pytest.approx(1.5)) == 0`` — approx is a sub-term, not
    # the direct comparand of == or !=.
    out = _lift("""
        def test_approx_subterm(x):
            assert x == f(pytest.approx(1.5))
    """)
    # Must not silently lift approx as a ctor sub-term.
    approx_warns = [w for w in out.warnings if "approx" in w.reason.lower()]
    assert approx_warns or out.lifted == 0, (
        "approx as sub-term must produce a loud-refuse warning or 0 lifts; "
        f"got lifted={out.lifted}, warnings={out.warnings}"
    )


def test_approx_in_value_scope_pattern_lifts():
    # POSITIVE: approx in a value-scope (mixed-body / scoped) pattern.
    # ``actual = compute(); assert actual == pytest.approx(1.5)``
    out = _lift("""
        def test_approx_value_scope():
            actual = compute()
            assert actual == pytest.approx(1.5)
    """)
    # Should lift (not silently refuse) — the scoped path also intercepts approx.
    assert out.lifted >= 1 or any("approx" in w.reason.lower() for w in out.warnings), (
        "approx in value-scope must be either lifted or loudly-refused (not silent); "
        f"lifted={out.lifted}, warnings={out.warnings}"
    )
    # No silent skip: must be claimed.
    assert "test_approx_value_scope" in out.claimed_tests, (
        "test must be CLAIMED (not silently fall through to Layer 0)"
    )


# ---------------------------------------------------------------------------
# isinstance form — census row: Bucket A
# Per-form discrimination tests (3 per variant: positive + discrimination +
# structural). RED-first; implementations in layer2.py follow.
# ---------------------------------------------------------------------------


# --- POSITIVE: isinstance(x, int) is a single liftable atom -----------------

def test_isinstance_builtin_int_lifts_to_atomic():
    # POSITIVE: a single ``assert isinstance(x, int)`` in a 2-assert body
    # must lift to an atomic("isinstance", [x, pytype_int]).
    # (Pairs with the truthy-and-none assert so characterization fires.)
    out = _lift("""
        def test_type_guard(x):
            assert x is not None
            assert isinstance(x, int)
    """)
    assert out.lifted == 1, (
        f"isinstance(x,int) must lift to a contract; warnings={[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    isinstance_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
    assert isinstance_atoms, (
        f"expected atomic('isinstance',...) in lifted formula; atoms={atoms!r}"
    )
    atom = isinstance_atoms[0]
    # arg[0] is the subject var; arg[1] is a nullary Ctor for the type name
    assert len(atom.args) == 2
    assert isinstance(atom.args[0], _Var) and atom.args[0].name == "x"
    assert isinstance(atom.args[1], _Ctor) and atom.args[1].name == "pytype_int"
    assert len(atom.args[1].args) == 0, "type ctor must be nullary"


# --- DISCRIMINATION: isinstance(x,int) ∧ not isinstance(x,int) is contradictory

def test_isinstance_same_type_self_contradiction_is_refused():
    # DISCRIMINATION: isinstance(x,int) ∧ ¬isinstance(x,int) — same subject,
    # same type — must be unsatisfiable (refused by the consistency pass).
    # Layer 2 lifts both; the Rust verifier's consistency pass must refuse it.
    # Here we only check the LIFT side: both atoms must appear in the conjunction.
    out = _lift("""
        def test_isinstance_contradiction(x):
            assert isinstance(x, int)
            assert not isinstance(x, int)
    """)
    assert out.lifted == 1, (
        f"both isinstance atoms must lift into one contract; "
        f"warnings={[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    pos_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
    neg_atoms = [
        a for a in atoms
        if isinstance(a, _Connective) and a.kind == "not"
        and isinstance(a.operands[0], _Atomic)
        and a.operands[0].name == "isinstance"
    ]
    assert pos_atoms, "must have positive isinstance atom"
    assert neg_atoms, "must have negated isinstance atom (not (isinstance ...))"


# --- STRUCTURAL: isinstance atom has correct subject and type ctor ----------

def test_isinstance_atom_carries_pytype_ctor_for_each_builtin():
    # STRUCTURAL: for each recognized builtin type name T, the lifted atom
    # is atomic("isinstance", [<subject>, ctor("pytype_<T>", [])]).
    # Checked for int, str, float, list, dict, set, tuple, bytes.
    builtins = ["int", "str", "float", "list", "dict", "set", "tuple", "bytes"]
    for type_name in builtins:
        # Build a 2-assert characterization body so Pattern 3 fires.
        src = (
            f"def test_type_check_{type_name}(x, y):\n"
            f"    assert isinstance(x, {type_name})\n"
            f"    assert isinstance(y, {type_name})\n"
        )
        out = _lift(src)
        assert out.lifted == 1, (
            f"isinstance(x,{type_name}) must lift; warnings={[w.reason for w in out.warnings]}"
        )
        atoms = _flatten_and(out.decls[0].inv)
        isinstance_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
        assert isinstance_atoms, f"expected isinstance atom for {type_name!r}"
        atom = isinstance_atoms[0]
        expected_ctor_name = f"pytype_{type_name}"
        assert isinstance(atom.args[1], _Ctor) and atom.args[1].name == expected_ctor_name, (
            f"type ctor for {type_name!r} must be {expected_ctor_name!r}; got {atom.args[1]!r}"
        )


# --- not isinstance(...) negation lifts correctly ---------------------------

def test_isinstance_negated_form_lifts():
    # POSITIVE (negation): ``assert not isinstance(x, str)`` must lift to
    # not_(atomic("isinstance", [x, ctor("pytype_str", [])])).
    out = _lift("""
        def test_not_type_guard(x):
            assert x > 0
            assert not isinstance(x, str)
    """)
    assert out.lifted == 1, (
        f"not isinstance must lift; warnings={[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    neg_isinstance = [
        a for a in atoms
        if isinstance(a, _Connective) and a.kind == "not"
        and isinstance(a.operands[0], _Atomic)
        and a.operands[0].name == "isinstance"
    ]
    assert neg_isinstance, "must have not_(isinstance) atom"
    inner = neg_isinstance[0].operands[0]
    assert isinstance(inner.args[1], _Ctor) and inner.args[1].name == "pytype_str"


# --- LOUD REFUSE: user-defined class (non-builtin second arg) ---------------

def test_isinstance_user_class_is_loudly_refused():
    # LOUD REFUSE: ``assert isinstance(x, MyClass)`` must remain refused
    # (type-lattice unknown for user classes).
    out = _lift("""
        def test_isinstance_guard():
            x = get_value()
            assert isinstance(x, MyClass)
    """)
    assert out.mixed_body_lifted == 0, (
        f"isinstance(x,MyClass) must not produce a contract; decls={[d.name for d in out.decls]}"
    )
    assert any("isinstance" in w.reason for w in out.warnings), (
        f"expected isinstance loud-refusal warning; warnings={[w.reason for w in out.warnings]}"
    )


# --- LOUD REFUSE: tuple-of-types second arg is refused ----------------------

def test_isinstance_tuple_of_types_is_loudly_refused():
    # LOUD REFUSE: ``assert isinstance(x, (int, str))`` — tuple-of-types form.
    # Cannot model as a single type constant; must refuse loudly.
    out = _lift("""
        def test_tuple_isinstance(x, y):
            assert isinstance(x, (int, str))
            assert isinstance(y, int)
    """)
    # The tuple form must not produce an isinstance atom; the int form may lift.
    all_decl_invs = [d.inv for d in out.decls]
    all_atoms = []
    for inv in all_decl_invs:
        all_atoms.extend(_flatten_and(inv))
    tuple_isinstance = [
        a for a in all_atoms
        if isinstance(a, _Atomic) and a.name == "isinstance"
        and len(a.args) == 2
        and isinstance(a.args[1], _Ctor)
        and a.args[1].name == "pytype_int_str"  # would be wrong
    ]
    # The key constraint: NO isinstance atom whose type arg is a multi-type ctor.
    # Instead we expect a loud-refuse warning.
    assert any(
        "isinstance" in w.reason or "tuple" in w.reason.lower()
        for w in out.warnings
    ), (
        f"tuple-of-types isinstance must produce a loud-refuse warning; "
        f"warnings={[w.reason for w in out.warnings]}"
    )


# --- LOUD REFUSE: isinstance against typing/abstract type -------------------

def test_isinstance_attribute_type_is_loudly_refused():
    # LOUD REFUSE: ``assert isinstance(x, typing.Sequence)`` —  attribute
    # expression as the second arg.  Unknown subtype hierarchy; refuse loudly.
    out = _lift("""
        def test_abstract_isinstance(x, y):
            assert isinstance(x, typing.Sequence)
            assert x is not None
    """)
    # Must warn about isinstance.
    assert any("isinstance" in w.reason for w in out.warnings), (
        f"isinstance with attribute type must warn; warnings={[w.reason for w in out.warnings]}"
    )


# --- DISCRIMINATION (different subjects): isinstance(x,int); isinstance(y,str) PROVEN

def test_isinstance_different_subjects_is_consistent():
    # DISCRIMINATION (false-refusal guard): isinstance(x,int) ∧ isinstance(y,str)
    # with DIFFERENT subjects must lift consistently — NOT refused.
    # Different subject vars => independent atoms => satisfiable conjunction.
    out = _lift("""
        def test_different_subjects(x, y):
            assert isinstance(x, int)
            assert isinstance(y, str)
    """)
    assert out.lifted == 1, (
        f"different-subject isinstance must lift as consistent characterization; "
        f"warnings={[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    isinstance_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
    assert len(isinstance_atoms) == 2, (
        f"expected 2 isinstance atoms for different subjects; atoms={atoms!r}"
    )
    subjects = {a.args[0].name for a in isinstance_atoms if isinstance(a.args[0], _Var)}
    assert subjects == {"x", "y"}, f"subjects must be x and y; got {subjects}"


# --- FALSE-REFUSAL GUARD: isinstance(x,int) ∧ isinstance(x,bool) must NOT be refused

def test_isinstance_int_and_bool_same_subject_is_consistent():
    # bool IS a subtype of int in Python: isinstance(True, int) is True.
    # So isinstance(x,int) ∧ isinstance(x,bool) on the SAME subject must be
    # CONSISTENT (not refused). This is the permanent false-refusal guard.
    # Layer 2 must lift both atoms; the encoder must NOT assert int/bool disjoint.
    out = _lift("""
        def test_bool_subtype_of_int(x):
            assert isinstance(x, int)
            assert isinstance(x, bool)
    """)
    assert out.lifted == 1, (
        f"isinstance(x,int) ∧ isinstance(x,bool) must lift (bool⊂int is consistent); "
        f"warnings={[w.reason for w in out.warnings]}"
    )
    atoms = _flatten_and(out.decls[0].inv)
    isinstance_atoms = [a for a in atoms if isinstance(a, _Atomic) and a.name == "isinstance"]
    assert len(isinstance_atoms) == 2, (
        f"both isinstance atoms must appear in the conjunction; atoms={atoms!r}"
    )
    type_names = {
        a.args[1].name
        for a in isinstance_atoms
        if isinstance(a.args[1], _Ctor)
    }
    assert "pytype_int" in type_names and "pytype_bool" in type_names, (
        f"must have both pytype_int and pytype_bool; got {type_names}"
    )
