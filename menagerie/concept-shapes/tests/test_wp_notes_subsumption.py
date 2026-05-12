"""Regression tests for notes-tolerant structural subsumption (issue #686).

Ruby's op_source-unit.spec.json carries a `notes` field in `post` that
the concept hub spec does not.  Before the fix, the wp-abstraction path in
try_structural_subsumption replaced post.wp but left post.notes intact,
so the relaxed CID never matched the concept CID, discharge failed, and
discharge.py's sweep wiped the ruby:source-unit morphism on every mint run.

These tests verify the normalization logic without requiring the Rust binary:
they mock canonical_cid_spec to return a deterministic value based on the
JSON representation of the payload, so the subsumption comparison is testable
in isolation.
"""
import copy
import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import mint_language_morphisms as mlm


def _make_post(wp, notes=None, extra_structural_field=None):
    """Build a minimal post dict for testing."""
    post = {
        "kind": "operation-contract",
        "operator": "source-unit",
        "arity": ["String", "Stmt"],
        "arity_shape": {
            "kind": "named",
            "slots": [
                {"name": "bytes", "evaluation": "unevaluated", "slot_sort": "literal"},
                {"name": "operational_term"},
            ],
        },
        "result": "Stmt",
        "wp": wp,
    }
    if notes is not None:
        post["notes"] = notes
    if extra_structural_field is not None:
        post["operator"] = extra_structural_field
    return post


def _make_spec(wp, notes=None, extra_structural_field=None):
    """Build a minimal function-contract spec for testing."""
    return {
        "kind": "algorithm",
        "fn_name": "concept:source-unit",
        "formals": ["bytes", "operational_term"],
        "formal_sorts": [
            {"kind": "ctor", "name": "String", "args": []},
            {"kind": "ctor", "name": "Stmt", "args": []},
        ],
        "return_sort": {"kind": "ctor", "name": "Stmt", "args": []},
        "pre": {"kind": "atomic", "name": "true", "args": []},
        "post": _make_post(wp, notes=notes, extra_structural_field=extra_structural_field),
        "effects": {"effects": []},
    }


def _canonical_key(spec):
    """Deterministic mock CID: canonical JSON of algorithm_payload."""
    payload = {
        "fn_name": spec.get("fn_name"),
        "formals": spec.get("formals", []),
        "formal_sorts": spec.get("formal_sorts", []),
        "pre": spec.get("pre"),
        "post": spec.get("post"),
        "effects": spec.get("effects"),
        "return_sort": spec.get("return_sort"),
    }
    return json.dumps(payload, sort_keys=True, separators=(",", ":"))


class NotesSubsumptionTests(unittest.TestCase):
    """Verify that post.notes is treated as documentation and doesn't block discharge."""

    def setUp(self):
        # Patch canonical_cid_spec in the mint_language_morphisms module so
        # tests run without the Rust binary.
        self.patcher = patch.object(mlm, "canonical_cid_spec", side_effect=_canonical_key)
        self.patcher.start()

    def tearDown(self):
        self.patcher.stop()

    def test_notes_only_difference_discharges_via_wp_abstraction(self):
        """wp texts match; only notes differs.  Should discharge (notes is doc-only)."""
        shared_wp = "lossless source wrapper; the source bytes are recoverable"
        concept = _make_spec(wp=shared_wp)
        lang = _make_spec(wp=shared_wp, notes="Language-specific implementation detail.")
        result = mlm.try_structural_subsumption(lang, concept)
        self.assertIsNotNone(
            result,
            "notes-only difference should discharge via wp-abstraction; got None (gap)"
        )
        method, pre_relaxed, wp_abstracted = result
        self.assertIn("wp-abstraction", method, f"unexpected discharge method: {method}")
        self.assertFalse(pre_relaxed, "pre should not be relaxed when pre already matches")

    def test_wp_and_notes_both_differ_discharges_via_wp_abstraction(self):
        """Both wp text and notes differ (the ruby:source-unit exact failure mode).

        This is the regression case from issue #686: ruby's op_source-unit.spec.json
        has a language-specific wp AND a notes field; the concept hub has neither.
        Structural subsumption must discharge.
        """
        concept = _make_spec(
            wp="lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"
        )
        lang = _make_spec(
            wp="lossless Ruby source wrapper; project_effects descends to operational_term",
            notes="The bytes slot carries the original UTF-8 Ruby source as a string.",
        )
        result = mlm.try_structural_subsumption(lang, concept)
        self.assertIsNotNone(
            result,
            "wp+notes difference (ruby:source-unit exact case) should discharge; got None (gap)"
        )
        method, pre_relaxed, wp_abstracted = result
        self.assertIn("wp-abstraction", method, f"unexpected discharge method: {method}")

    def test_structural_field_mismatch_does_not_discharge(self):
        """A real semantic difference (operator name) must NOT discharge.

        Soundness gate: notes tolerance must not cause false-positive subsumption.
        """
        concept = _make_spec(wp="lossless source wrapper")
        # Inject a structural difference: different operator value (not documentation).
        lang = _make_spec(
            wp="lossless Ruby source wrapper",
            notes="Some note.",
            extra_structural_field="different-operator",
        )
        result = mlm.try_structural_subsumption(lang, concept)
        self.assertIsNone(
            result,
            "structural operator mismatch must NOT discharge even when notes/wp differ"
        )

    def test_no_notes_no_regression_wp_only_still_works(self):
        """Pre-existing wp-only abstraction continues to work with no notes field."""
        concept = _make_spec(wp="concept wp text")
        lang = _make_spec(wp="language-specific wp text")
        result = mlm.try_structural_subsumption(lang, concept)
        self.assertIsNotNone(
            result,
            "pre-existing wp-only difference must still discharge"
        )
        method, _, _ = result
        self.assertIn("wp-abstraction", method)

    def test_identical_specs_returns_none(self):
        """Byte-equal specs trigger the early-return guard (no relaxation needed)."""
        spec = _make_spec(wp="exact wp")
        result = mlm.try_structural_subsumption(copy.deepcopy(spec), copy.deepcopy(spec))
        self.assertIsNone(
            result,
            "byte-equal specs must return None (byte-equality path handles them)"
        )


if __name__ == "__main__":
    unittest.main()
