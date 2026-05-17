"""Replayability tests for manifest-driven concept-library probes."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
GENERAL_SCRIPT = ROOT / "tools" / "concept-library-completeness-probe.py"
OPERATION_SCRIPT = ROOT / "tools" / "concept-library-completeness-probe-operation-layer.py"
EXAM_DIR = ROOT / "menagerie" / "concept-shapes" / "exams"


def _manifest_path() -> Path:
    paths = sorted(EXAM_DIR.glob("v1.1.blake3-512:*.json"))
    assert len(paths) == 1, paths
    return paths[0]


def _manifest_cid(path: Path) -> str:
    return json.loads(path.read_text(encoding="utf-8"))["header"]["cid"]


def _run_probe(script: Path, output: Path) -> bytes:
    env = os.environ.copy()
    env["PROBE_OUTPUT_PATH"] = str(output)
    subprocess.run(
        [sys.executable, str(script)],
        cwd=ROOT,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )
    return output.read_bytes()


class ConceptLibraryCompletenessProbeTests(unittest.TestCase):
    def test_general_probe_replays_byte_identical_latest_manifest_report(self) -> None:
        manifest = _manifest_path()
        manifest_cid = _manifest_cid(manifest)

        with tempfile.TemporaryDirectory() as temp_dir:
            left = Path(temp_dir) / "left.md"
            right = Path(temp_dir) / "right.md"
            left_bytes = _run_probe(GENERAL_SCRIPT, left)
            right_bytes = _run_probe(GENERAL_SCRIPT, right)

        self.assertEqual(left_bytes, right_bytes)
        report = left_bytes.decode("utf-8")
        self.assertIn(f"**Manifest CID**: `{manifest_cid}`", report)
        self.assertIn("| total | 1502 |", report)

    def test_operation_layer_probe_replays_byte_identical_latest_manifest_report(self) -> None:
        manifest = _manifest_path()
        manifest_cid = _manifest_cid(manifest)

        with tempfile.TemporaryDirectory() as temp_dir:
            left = Path(temp_dir) / "left-operation.md"
            right = Path(temp_dir) / "right-operation.md"
            left_bytes = _run_probe(OPERATION_SCRIPT, left)
            right_bytes = _run_probe(OPERATION_SCRIPT, right)

        self.assertEqual(left_bytes, right_bytes)
        report = left_bytes.decode("utf-8")
        self.assertIn(f"**Manifest CID**: `{manifest_cid}`", report)
        self.assertIn("| morphism |", report)


if __name__ == "__main__":
    unittest.main()
