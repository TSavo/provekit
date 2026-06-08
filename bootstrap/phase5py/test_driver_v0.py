from __future__ import annotations

import ast
import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / "bootstrap/phase5py/driver_v0.py"
RECEIPT = ROOT / "bootstrap/phase5py/v0_receipt.json"
MODULE = ROOT / "bootstrap/phase5py/libsugar_py_v0.py"
README = ROOT / "bootstrap/phase5py/README.md"


class Phase5PyDriverV0Test(unittest.TestCase):
    def test_driver_records_python_value_constructor_self_trip(self) -> None:
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

        source = MODULE.read_text(encoding="utf-8")
        ast.parse(source)
        self.assertNotIn("\u2014", source)

        receipt = json.loads(RECEIPT.read_text(encoding="utf-8"))
        self.assertEqual(receipt["arc"], "Phase-5-Py-v0")
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

        self.assertEqual(
            {case["verdict"] for case in receipt["cases"]},
            {"CHARACTERIZED_DIFF"},
        )
        self.assertTrue(
            all(
                case["diff"]["classification"] == "realize-python-template-gap"
                for case in receipt["cases"]
            )
        )

        readme_lines = README.read_text(encoding="utf-8").splitlines()
        self.assertGreaterEqual(len(readme_lines), 40)
        self.assertLessEqual(len(readme_lines), 60)
        self.assertNotIn("\u2014", "\n".join(readme_lines))


if __name__ == "__main__":
    unittest.main()
