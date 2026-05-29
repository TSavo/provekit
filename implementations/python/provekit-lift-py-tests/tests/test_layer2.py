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
    assert out.decls[0].name == "test_three"


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
    assert out.decls[0].name == "test_native_assertions"
    inv = out.decls[0].inv
    assert isinstance(inv, _Connective)
    assert inv.kind == "and"
    atoms = list(inv.operands)
    assert len(atoms) == 5
    assert [atom.name for atom in atoms] == ["=", "≠", ">", "=", "≠"]
    none_atoms = [
        atom
        for atom in atoms
        if atom.name in {"=", "≠"} and isinstance(atom.args[1], _Ctor)
    ]
    assert [atom.args[1].name for atom in none_atoms] == ["None", "None"]


def test_unittest_unsupported_assertion_warns_without_fake_contract():
    out = _lift("""
        import unittest

        class RegexTest(unittest.TestCase):
            def test_regex(self):
                self.assertRegex("abc", "a.*")
    """)
    assert out.lifted == 0
    assert "test_regex" in out.claimed_tests
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


# --- No pattern fires ----------------------------------------------------


def test_single_literal_assert_test_falls_through_to_layer0():
    out = _lift("""
        def test_one():
            assert 1 == 1
    """)
    assert out.lifted == 0
    assert not out.claimed_tests
