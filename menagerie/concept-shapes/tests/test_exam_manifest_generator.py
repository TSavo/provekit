"""Replayability tests for the v1 exam manifest generator."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "menagerie" / "concept-shapes" / "scripts" / "mint_exam_manifest.py"

QUESTION_KINDS = {
    "boundary-tag",
    "composition",
    "effect",
    "morphism",
    "realization",
    "sort",
}
EXPECTED_SHAPES = {
    "BoundaryTagMemento",
    "EffectSignatureMemento",
    "MorphismMemento",
    "RealizationMemento",
    "SortMorphismMemento",
}


def _run_generator(output_dir: Path) -> Path:
    subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--output-dir",
            str(output_dir),
            "--skip-index",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    outputs = sorted(output_dir.glob("v1.blake3-512:*.json"))
    assert len(outputs) == 1, outputs
    return outputs[0]


def _jcs_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def _blake3_512(data: bytes) -> str:
    proc = subprocess.run(
        ["b3sum", "--length", "64"],
        input=data,
        check=True,
        capture_output=True,
    )
    digest = proc.stdout.decode("utf-8").split()[0]
    return f"blake3-512:{digest}"


class ExamManifestGeneratorTests(unittest.TestCase):
    def test_generator_replays_byte_identical_manifest_with_matching_cid(self) -> None:
        with tempfile.TemporaryDirectory() as left_dir, tempfile.TemporaryDirectory() as right_dir:
            left_path = _run_generator(Path(left_dir))
            right_path = _run_generator(Path(right_dir))

            left_bytes = left_path.read_bytes()
            right_bytes = right_path.read_bytes()

        self.assertEqual(left_bytes, right_bytes)

        manifest = json.loads(left_bytes)
        embedded_cid = manifest["header"]["cid"]
        self.assertEqual(left_path.name, f"v1.{embedded_cid}.json")

        cid_input = {
            "content": manifest["header"]["content"],
            "metadata": manifest["metadata"],
        }
        self.assertEqual(_blake3_512(_jcs_bytes(cid_input)), embedded_cid)

        questions = manifest["header"]["content"]["questions"]
        self.assertGreaterEqual(len(questions), 500)
        self.assertEqual(
            manifest["header"]["content"]["question_kinds"],
            ["boundary-tag", "composition", "effect", "morphism", "realization", "sort"],
        )

        for question in questions:
            self.assertIn(question.get("kind"), QUESTION_KINDS)
            self.assertIsInstance(question.get("parameters"), dict)
            self.assertTrue(question.get("concept", "").startswith("concept:"))
            self.assertIn(question.get("expected_answer_shape"), EXPECTED_SHAPES)


if __name__ == "__main__":
    unittest.main()
