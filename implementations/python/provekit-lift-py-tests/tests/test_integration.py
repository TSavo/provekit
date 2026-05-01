# SPDX-License-Identifier: Apache-2.0
#
# Layer 2 integration test. Drives the adapter over
# tests/fixtures/layer2_sample.py and asserts:
#
#   - Bounded-loop, helper-inlining, characterization, AND parametrize
#     patterns each produce mementos.
#   - The deliberately-skipped nested loop logs a structured warning.
#   - The fixture mints >= 10 distinct mementos with distinct CIDs,
#     proving Layer 2 produces real content-addressed claim material.
#   - The claim set names every test the adapter took ownership of.

from __future__ import annotations

import os

from provekit_lift_py_tests import (
    encode_jcs,
    formula_to_value,
    jcs_hash,
    lift_file_layer2,
)


_HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURE = os.path.join(_HERE, "fixtures", "layer2_sample.py")


def _load_fixture():
    with open(FIXTURE, "r", encoding="utf-8") as f:
        return f.read()


def test_layer2_sample_lifts_all_four_patterns():
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)

    # Pattern 1: 4 bounded loops, of which 1 is the nested-loop skip.
    assert out.bounded_loop_lifted == 4, f"got {out.bounded_loop_lifted}: {out.warnings}"
    assert out.bounded_loop_skipped == 1
    # Pattern 2: 3 + 2 = 5 inlined helper-call mementos.
    assert out.helper_inlined_lifted == 5
    # Pattern 3: 2 characterization tests (pytest free + unittest method).
    assert out.characterization_lifted == 2
    # Pattern 4: 1 parametrize.
    assert out.parametrize_lifted == 1
    # Total mementos
    assert out.lifted == 4 + 5 + 2 + 1


def test_layer2_sample_emits_at_least_10_distinct_mementos():
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)
    assert len(out.decls) >= 10
    # Hash each contract's invariant and confirm distinct CIDs.
    cids = {jcs_hash(formula_to_value(d.inv)) for d in out.decls if d.inv is not None}
    assert len(cids) >= 10, f"expected >=10 distinct CIDs, got {len(cids)}"


def test_layer2_sample_warns_on_nested_loop_with_structured_reason():
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)
    matches = [w for w in out.warnings if w.item_name == "test_nested_loop_skipped"]
    assert matches, f"expected a warning for test_nested_loop_skipped: {out.warnings}"
    assert any("nested" in w.reason for w in matches)


def test_layer2_claim_set_covers_every_owned_test():
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)
    expected = {
        "test_squares_are_nonneg",
        "test_divmod_in_range",
        "test_small_window",
        "test_literal_list_iter",
        "test_many_42s",
        "test_ranges_ok",
        "test_parse_int_characterization",
        "test_three_facts",
        "test_v_nonneg",
        "test_nested_loop_skipped",
    }
    assert expected.issubset(out.claimed_tests), (
        f"missing claims: {expected - out.claimed_tests}"
    )


def test_layer2_memento_naming_matches_spec():
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)
    names = {d.name for d in out.decls}
    # Pattern 1: <test>::loop::<var>
    assert "test_squares_are_nonneg::loop::x" in names
    assert "test_literal_list_iter::loop::v" in names
    # Pattern 2: <test>::call::<i>
    assert "test_many_42s::call::0" in names
    assert "test_many_42s::call::1" in names
    assert "test_many_42s::call::2" in names
    # Pattern 3: <test> bare
    assert "test_parse_int_characterization" in names
    assert "test_three_facts" in names
    # Pattern 4: <test>::parametrize::<params>
    assert "test_v_nonneg::parametrize::v" in names


def test_layer2_pattern1_jcs_canonical_byte_stable():
    """Sanity: the small_window memento's JCS encoding has the kind/name/sort
    keys in JCS-sorted order. Cross-language conformance test pins the bytes;
    this test guards the stable shape on the integration path.
    """
    src = _load_fixture()
    out = lift_file_layer2(src, FIXTURE)
    target = next(d for d in out.decls if d.name == "test_small_window::loop::x")
    j = encode_jcs(formula_to_value(target.inv))
    # Top-level forall-quantifier object has body, kind, name, sort keys
    # in that JCS-sorted order.
    assert j.startswith('{"body":')
    assert '"kind":"forall"' in j
    assert '"name":"x"' in j
    assert '"sort":{"kind":"primitive","name":"Int"}' in j
