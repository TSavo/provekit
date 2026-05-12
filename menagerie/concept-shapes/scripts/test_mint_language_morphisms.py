"""Regression tests for try_structural_subsumption in mint_language_morphisms.

Key regression: after the wp-as-formula rename (PR2), concept ops carry
post.wp_note while language ops still carry post.wp.  When both hold the same
value, a pure value comparison (after_wp == concept_wp) returns True, so the
early-exit and the needs-guard both fire, and the function returns None without
reaching _make_relaxed.  The result: morphisms that require only a key-rename
(wp -> wp_note) are silently dropped.

Fix: use key+value equality (wp_byte_same) instead of value-only (wp_matches)
for both guards.  The tests below verify the fix is in place and will catch any
future regression of this kind.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_language_morphisms as m


def _make_spec(wp_key=None, wp_value=None, pre=None, effects=None):
    """Build a minimal algorithm spec for subsumption testing."""
    post = {
        "arity_shape": {
            "slots": [
                {"name": "a", "evaluation": "evaluated"},
                {"name": "b", "evaluation": "evaluated"},
            ]
        }
    }
    if wp_key is not None and wp_value is not None:
        post[wp_key] = wp_value
    spec = {
        "kind": "algorithm",
        "fn_name": "test:op",
        "formals": ["a", "b"],
        "formal_sorts": [m.primitive("Int"), m.primitive("Int")],
        "return_sort": m.primitive("Int"),
        "pre": pre if pre is not None else m.true_formula(),
        "post": post,
        "effects": effects if effects is not None else m.empty_effects(),
    }
    return spec


class TryStructuralSubsumptionTests(unittest.TestCase):

    def test_wp_note_vs_wp_same_value_produces_morphism(self):
        """The core regression: concept has post.wp_note, lang has post.wp,
        both with the same value.  Before the fix, try_structural_subsumption
        returned None (morphism silently dropped).  After the fix it must return
        a method tuple (morphism discharged via wp-abstraction).
        """
        concept_spec = _make_spec(wp_key="wp_note", wp_value="result == a + b")
        lang_spec = _make_spec(wp_key="wp", wp_value="result == a + b")

        result = m.try_structural_subsumption(lang_spec, concept_spec)

        self.assertIsNotNone(
            result,
            "try_structural_subsumption must not return None when concept has "
            "post.wp_note and lang has post.wp with the same value: the key "
            "difference requires wp-abstraction, not early-exit.",
        )
        method, _relax_pre, relax_wp = result
        self.assertTrue(relax_wp, "morphism must be discharged via wp relaxation")
        self.assertIn("wp-abstraction", method)

    def test_wp_note_vs_wp_note_same_value_returns_none(self):
        """When both sides have wp_note with the same value (and all other
        fields match), byte-equality fires upstream; try_structural_subsumption
        should return None (nothing to relax).
        """
        concept_spec = _make_spec(wp_key="wp_note", wp_value="result == a + b")
        lang_spec = _make_spec(wp_key="wp_note", wp_value="result == a + b")

        result = m.try_structural_subsumption(lang_spec, concept_spec)

        self.assertIsNone(
            result,
            "When keys AND values agree, byte-equality already discharges; "
            "structural subsumption should return None (nothing to relax).",
        )

    def test_wp_note_vs_wp_different_value_produces_morphism(self):
        """When concept has post.wp_note and lang has post.wp with DIFFERENT
        values, subsumption should still discharge (wp is documentation prose,
        not semantic; abstraction is always valid).
        """
        concept_spec = _make_spec(wp_key="wp_note", wp_value="result == a + b")
        lang_spec = _make_spec(wp_key="wp", wp_value="adds a and b")

        result = m.try_structural_subsumption(lang_spec, concept_spec)

        self.assertIsNotNone(
            result,
            "When values differ but structure matches, wp-abstraction must "
            "still discharge the morphism (wp is documentation, not semantic).",
        )
        method, _relax_pre, relax_wp = result
        self.assertTrue(relax_wp)
        self.assertIn("wp-abstraction", method)

    def test_no_wp_on_either_side_with_pre_mismatch_still_works(self):
        """Sanity: pre-weakening still discharges when lang pre is true and no
        wp fields are involved.  Guards the regression fix doesn't break the
        existing pre-weakening path.
        """
        concept_pre = {
            "kind": "atomic",
            "name": "gt",
            "args": [{"kind": "var", "name": "a"}, {"kind": "lit", "value": 0}],
        }
        concept_spec = _make_spec(pre=concept_pre)
        lang_spec = _make_spec(pre=m.true_formula())  # trivially true -> can weaken

        result = m.try_structural_subsumption(lang_spec, concept_spec)

        self.assertIsNotNone(
            result,
            "pre-weakening path must discharge when lang pre is true.",
        )
        method, relax_pre, _relax_wp = result
        self.assertTrue(relax_pre)
        self.assertIn("pre-weakening", method)


if __name__ == "__main__":
    unittest.main()
