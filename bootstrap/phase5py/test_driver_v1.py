from __future__ import annotations

import ast
import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / "bootstrap/phase5py/driver_v1.py"
RECEIPT = ROOT / "bootstrap/phase5py/v1_receipt.json"
MODULE = ROOT / "bootstrap/phase5py/libprovekit_py_v1.py"
README = ROOT / "bootstrap/phase5py/README.md"
BODY_TEMPLATE = (
    ROOT
    / "menagerie/python-language-signature/specs/body-templates/"
    / "python-canonical-bodies-libprovekit.json"
)


class Phase5PyDriverV1Test(unittest.TestCase):
    def test_driver_records_libprovekit_value_constructor_self_trip(self) -> None:
        for path in (RECEIPT, MODULE, README):
            if path.exists():
                path.unlink()

        proc = subprocess.run(
            [sys.executable, str(DRIVER)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

        self.assertEqual(
            proc.returncode,
            0,
            msg=f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}",
        )
        self.assertTrue(RECEIPT.exists())
        self.assertTrue(MODULE.exists())
        self.assertTrue(README.exists())
        self.assertTrue(BODY_TEMPLATE.exists())

        catalog = json.loads(BODY_TEMPLATE.read_text(encoding="utf-8"))
        content = catalog["header"]["content"]
        self.assertEqual(content["template_name"], "python-canonical-bodies-libprovekit")
        self.assertEqual(content["target_language"], "python")
        self.assertEqual(
            [entry["concept_name"] for entry in content["entries"]],
            [
                "concept:value-null",
                "concept:value-boolean",
                "concept:value-integer",
                "concept:value-string",
            ],
        )

        source = MODULE.read_text(encoding="utf-8")
        ast.parse(source)
        self.assertNotIn("\u2014", source)

        receipt = json.loads(RECEIPT.read_text(encoding="utf-8"))
        self.assertEqual(receipt["arc"], "Phase-5-Py-v1")
        self.assertEqual(
            [case["name"] for case in receipt["cases"]],
            ["null", "boolean", "integer", "string"],
        )
        for case in receipt["cases"]:
            self.assertIn(case["verdict"], {"BYTE_IDENTICAL", "CHARACTERIZED_DIFF"})
            self.assertRegex(case["emitted_python_source_cid"], r"^blake3-512:[0-9a-f]{128}$")
            self.assertRegex(case["lift_python_output_cid"], r"^blake3-512:[0-9a-f]{128}$")
            self.assertRegex(case["rust_fixture_substrate_cid"], r"^blake3-512:[0-9a-f]{128}$")
            self.assertIn("kit_behavior_responsible", case)
            self.assertFalse(case["realize_python_is_stub"])

        verdicts = {case["verdict"] for case in receipt["cases"]}
        if receipt["all_substrate_cids_match"]:
            self.assertEqual(verdicts, {"BYTE_IDENTICAL"})
        else:
            self.assertEqual(verdicts, {"CHARACTERIZED_DIFF"})
            self.assertTrue(
                all(
                    case["diff"]["classification"]
                    == "lift-python-substrate-namespace-mismatch"
                    for case in receipt["cases"]
                )
            )

        readme = README.read_text(encoding="utf-8")
        self.assertNotIn("\u2014", readme)
        self.assertIn("Phase-5-Py-v1", readme)


if __name__ == "__main__":
    unittest.main()
