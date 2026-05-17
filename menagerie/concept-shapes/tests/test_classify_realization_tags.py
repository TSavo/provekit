"""Replayability tests for the realization tag classification audit."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "tools" / "classify-realization-tags.py"
MANIFEST_CID = "blake3-512:0e012db4ce35b235b8482344795ccbe8bccad51522825b5c495a862648736936497b11a940cf0ba9170ee6202849e9a8dc9eca5cb3021261ffa2f4ac4df6edc1"


def _run_classification(output: Path) -> bytes:
    env = os.environ.copy()
    env["REALIZATION_TAG_CLASSIFICATION_OUTPUT_PATH"] = str(output)
    subprocess.run(
        [sys.executable, str(SCRIPT)],
        cwd=ROOT,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )
    return output.read_bytes()


class RealizationTagClassificationTests(unittest.TestCase):
    def test_classification_audit_replays_byte_identical_report(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            left = Path(temp_dir) / "left.md"
            right = Path(temp_dir) / "right.md"
            left_bytes = _run_classification(left)
            right_bytes = _run_classification(right)

        self.assertEqual(left_bytes, right_bytes)
        self.assertNotIn(b"\xe2\x80\x94", left_bytes)
        self.assertNotIn(b"\xe2\x80\x93", left_bytes)

        report = left_bytes.decode("utf-8")
        self.assertIn(f"**Question reference**: `{MANIFEST_CID}`", report)
        self.assertIn("| active languages | 10 |", report)
        self.assertIn("| primitive concepts | 45 |", report)
        self.assertIn("| abstraction concepts | 18 |", report)
        self.assertIn("| total rows | 630 |", report)
        for language in (
            "c11",
            "csharp",
            "go",
            "java",
            "php",
            "python",
            "ruby",
            "rust",
            "typescript",
            "zig",
        ):
            self.assertIn(f"### {language}", report)


if __name__ == "__main__":
    unittest.main()
