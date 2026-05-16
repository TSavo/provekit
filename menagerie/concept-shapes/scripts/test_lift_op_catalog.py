#!/usr/bin/env python3
"""Focused tests for op catalog lifting."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import lift_op_catalog


class LiftOpCatalogTests(unittest.TestCase):
    def test_build_citation_uses_bare_sorts_effects_and_wp_note(self) -> None:
        spec = {
            "effects": {
                "effects": [
                    {"kind": "effect-signature", "name": "Trap"},
                    {"kind": "effect-signature", "name": "Panic"},
                    {"kind": "CriticalSection", "balanced": True},
                ]
            },
            "fn_name": "concept:add",
            "formal_sorts": [
                {"args": [], "kind": "ctor", "name": "Int"},
                {"args": [], "kind": "ctor", "name": "Int"},
            ],
            "formals": ["lhs", "rhs"],
            "post": {"wp_note": "adds when no overflow holds"},
            "return_sort": {"args": [], "kind": "ctor", "name": "Int"},
        }

        citation = lift_op_catalog.build_citation(
            spec,
            op_definition_cid="blake3-512:" + "0" * 128,
        )

        self.assertEqual(
            citation,
            {
                "args": {
                    "arg_sorts": ["Int", "Int"],
                    "effects": ["Trap", "Panic", "CriticalSection"],
                    "name": "concept:add",
                    "return_sort": "Int",
                    "wp_rule": "adds when no overflow holds",
                },
                "kind": "op-application",
                "op_definition_cid": "blake3-512:" + "0" * 128,
            },
        )

    def test_lift_catalog_writes_canonical_artifact_and_index(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            spec_dir = Path(tmp) / "specs"
            spec_dir.mkdir()
            (spec_dir / "add_shape.spec.json").write_text(
                json.dumps(
                    {
                        "effects": {"effects": []},
                        "fn_name": "concept:add",
                        "formal_sorts": [{"args": [], "kind": "ctor", "name": "Int"}],
                        "formals": ["x"],
                        "post": {},
                        "return_sort": {"args": [], "kind": "ctor", "name": "Int"},
                    }
                ),
                encoding="utf-8",
            )

            rows = lift_op_catalog.lift_catalog(
                spec_dir=spec_dir,
                out_dir=spec_dir / "op-definitions",
                op_definition_cid="blake3-512:" + "1" * 128,
                cid_for_value=lambda value: "blake3-512:" + "2" * 128,
            )

            self.assertEqual(len(rows), 1)
            artifact = spec_dir / "op-definitions" / "concept:add.op-def.ccl.json"
            index = spec_dir / "op-definitions" / "index.cids.json"
            self.assertTrue(artifact.exists())
            self.assertTrue(index.exists())
            self.assertEqual(artifact.read_text(encoding="utf-8").count("\n"), 0)
            self.assertIn('"wp_rule":""', artifact.read_text(encoding="utf-8"))
            self.assertEqual(
                json.loads(index.read_text(encoding="utf-8")),
                {"concept:add": rows[0]["cid"]},
            )


if __name__ == "__main__":
    unittest.main()
