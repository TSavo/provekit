from __future__ import annotations

import ast
import sys
import textwrap
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/sugar-lift-python-source/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from sugar_lift_python_source.ir import bool_const, ctor, int_const, none_const, str_const
from sugar_lift_python_source.lifter import lift_source
from sugar_lift_python_source.value_pins import (
    VALUE_PIN_REFUSAL_KIND,
    scan_module_value_pins,
)


def _scan(source: str):
    return scan_module_value_pins(ast.parse(textwrap.dedent(source)))


def _lift(source: str):
    return lift_source(textwrap.dedent(source), "mod.py")


def _tree_contains(obj, needle) -> bool:
    if obj == needle:
        return True
    if isinstance(obj, dict):
        return any(_tree_contains(value, needle) for value in obj.values())
    if isinstance(obj, list):
        return any(_tree_contains(item, needle) for item in obj)
    return False


def _has_reads_effect(result, name: str) -> bool:
    return _tree_contains(result.ir, {"kind": "reads", "target": name})


def _pin_refusals(refusals):
    return [r for r in refusals if r.get("kind") == VALUE_PIN_REFUSAL_KIND]


# --- positive: admitted pins substitute the value and drop the fog read ---


def test_str_constant_pins_value_at_use_site():
    result = _lift(
        """
        X = "abc"

        def f():
            return X
        """
    )
    assert _tree_contains(result.ir, str_const("abc"))
    assert not _has_reads_effect(result, "X")
    assert not _pin_refusals(result.refusals)


def test_int_bool_none_negative_pins():
    scan = _scan(
        """
        A = 5
        B = True
        C = None
        D = -7
        """
    )
    assert scan.pins["A"].term == int_const(5)
    assert scan.pins["B"].term == bool_const(True)
    assert scan.pins["C"].term == none_const()
    assert scan.pins["D"].term == int_const(-7)
    assert scan.totality_holds()


def test_tuple_of_literals_pins():
    scan = _scan('X = (1, "a")\n')
    assert scan.pins["X"].term == ctor("python:tuple", int_const(1), str_const("a"))


def test_final_confession_pins():
    scan = _scan(
        """
        from typing import Final

        X: Final = 5
        Y: Final[int] = 6
        """
    )
    assert scan.pins["X"].confession == "typing.Final"
    assert scan.pins["Y"].term == int_const(6)


def test_plain_annotation_still_pins_as_derived():
    scan = _scan('X: str = "abc"\n')
    assert scan.pins["X"].confession is None
    assert scan.pins["X"].term == str_const("abc")


# --- discrimination: every rebinding shape refuses, with the named reason ---


def _assert_single_refusal(source: str, reason_fragment: str, name: str = "X"):
    scan = _scan(source)
    assert name not in scan.pins
    refusals = _pin_refusals(scan.refusals)
    assert len(refusals) == 1, refusals
    assert refusals[0]["name"] == name
    assert reason_fragment in refusals[0]["reason"], refusals[0]["reason"]
    assert scan.totality_holds()


def test_second_assignment_refuses():
    _assert_single_refusal('X = 1\nX = 2\n', "rebound: assignment")


def test_augmented_assignment_refuses():
    _assert_single_refusal('X = 1\nX += 1\n', "augmented assignment")


def test_loop_body_assignment_refuses():
    _assert_single_refusal(
        """
        X = 1
        for i in range(2):
            X = i
        """,
        "rebound: assignment",
    )


def test_conditional_rebinding_refuses():
    _assert_single_refusal(
        """
        X = 1
        if True:
            X = 2
        """,
        "rebound: assignment",
    )


def test_walrus_refuses():
    _assert_single_refusal('X = 1\nY = (X := 2)\n', "walrus rebinding")


def test_del_refuses():
    _assert_single_refusal('X = 1\ndel X\n', "deletion")


def test_import_rebinding_refuses():
    _assert_single_refusal(
        'X = 1\nfrom os import path as X\n', "import rebinding"
    )


def test_def_shadow_refuses():
    _assert_single_refusal('X = 1\ndef X():\n    pass\n', "function definition")


def test_global_writer_in_nested_function_refuses():
    _assert_single_refusal(
        """
        X = 1

        def g():
            global X
            X = 2
        """,
        "global declaration in nested scope",
    )


def test_with_as_rebinding_refuses():
    _assert_single_refusal(
        """
        X = 1
        with open("f") as X:
            pass
        """,
        "with-as binding",
    )


def test_try_except_as_rebinding_refuses():
    _assert_single_refusal(
        """
        X = 1
        try:
            pass
        except ValueError as X:
            pass
        """,
        "except-as binding",
    )


# --- mutable / unrepresentable values refuse by name ---


def test_list_refuses_as_mutable():
    _assert_single_refusal('X = [1]\n', "mutable value (list) cannot pin")


def test_set_refuses_as_mutable():
    _assert_single_refusal('X = {1}\n', "mutable value (set) cannot pin")


def test_dict_refuses_as_mutable():
    _assert_single_refusal('X = {"a": 1}\n', "mutable value (dict) cannot pin")


def test_bytes_refuses_no_term_shape():
    _assert_single_refusal('X = b"x"\n', "no IR term shape for bytes")


def test_tuple_containing_list_refuses():
    # Literal-shaped (so a candidate) but not immutable all the way down.
    _assert_single_refusal('X = (1, [2])\n', "mutable value (list) cannot pin")


# --- confessions are still scanned: a contradicted oath refuses loudly ---


def test_final_then_mutated_refuses_with_contradiction_message():
    scan = _scan(
        """
        from typing import Final

        X: Final = 5
        X = 6
        """
    )
    refusals = _pin_refusals(scan.refusals)
    assert len(refusals) == 1
    assert "vendor contradicted their own typing.Final confession" in refusals[0]["reason"]


# --- structural: the refusal record shape and the totality arithmetic ---


def test_refusal_record_is_structural():
    scan = _scan('X = 1\nX = 2\n')
    record = _pin_refusals(scan.refusals)[0]
    assert record["kind"] == VALUE_PIN_REFUSAL_KIND
    assert record["name"] == "X"
    assert isinstance(record["line"], int)
    assert isinstance(record["reason"], str) and record["reason"]


def test_totality_candidates_equal_admitted_plus_refused():
    scan = _scan(
        """
        A = "ok"
        B = 2
        L = [1]
        M = 1
        M += 1
        Z = b"x"
        """
    )
    assert scan.candidates == 5
    assert len(scan.pins) == 2
    assert len(_pin_refusals(scan.refusals)) == 3
    assert scan.totality_holds()


def test_non_literal_bindings_are_not_candidates():
    # Fog was never a candidate: no pin, no refusal owed.
    scan = _scan('X = f()\nY = X\n')
    assert scan.candidates == 0
    assert not scan.pins
    assert not scan.refusals


# --- the house's walls: locals shadow pins; refused names stay symbolic ---


def test_local_shadow_wins_over_pin():
    result = _lift(
        """
        X = "abc"

        def f(X):
            return X
        """
    )
    assert not _tree_contains(result.ir, str_const("abc"))


def test_refused_name_keeps_symbolic_read():
    result = _lift(
        """
        X = 1
        X = 2

        def f():
            return X
        """
    )
    assert _has_reads_effect(result, "X")
    assert _pin_refusals(result.refusals)


# --- the bad-twin flip: the pin carries the value, not decoration ---


def test_bad_twin_flip_changes_the_lifted_term():
    good = _lift('X = "abc"\n\ndef f():\n    return X\n')
    twin = _lift('X = "abd"\n\ndef f():\n    return X\n')
    assert _tree_contains(good.ir, str_const("abc"))
    assert _tree_contains(twin.ir, str_const("abd"))
    assert not _tree_contains(twin.ir, str_const("abc"))
