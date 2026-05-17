"""Replayability tests for the v1.1 exam manifest generator."""

from __future__ import annotations

from collections import Counter
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
    "boundary-realization",
    "composition",
    "concept-realization",
    "effect-classification",
    "morphism",
    "sort-classification",
}
EXPECTED_SHAPES = {
    "BoundaryRealizationMemento",
    "BoundaryTagMemento",
    "EffectSignatureMemento",
    "MorphismMemento",
    "RealizationMemento",
    "SortMorphismMemento",
}
EXPECTED_KIND_COUNTS = Counter(
    {
        "boundary-realization": 11,
        "boundary-tag": 11,
        "concept-realization": 630,
        "effect-classification": 250,
        "morphism": 450,
        "sort-classification": 150,
    }
)
NATIVE_LIBRARY_LANGUAGE = {
    "java-httpclient": "java",
    "java-jdbc": "java",
    "javascript-fetch": "typescript",
    "libcurl": "c11",
    "python-aiosqlite": "python",
    "python-psycopg2": "python",
    "python-requests": "python",
    "python-sqlite3": "python",
    "python-urllib": "python",
    "rust-reqwest": "rust",
    "rust-sqlx": "rust",
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
    outputs = sorted(output_dir.glob("v1.1.blake3-512:*.json"))
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
    def test_generator_replays_byte_identical_v1_1_manifest_with_matching_cid(self) -> None:
        with tempfile.TemporaryDirectory() as left_dir, tempfile.TemporaryDirectory() as right_dir:
            left_path = _run_generator(Path(left_dir))
            right_path = _run_generator(Path(right_dir))

            left_bytes = left_path.read_bytes()
            right_bytes = right_path.read_bytes()

        self.assertEqual(left_bytes, right_bytes)

        manifest = json.loads(left_bytes)
        embedded_cid = manifest["header"]["cid"]
        self.assertEqual(left_path.name, f"v1.1.{embedded_cid}.json")
        self.assertEqual(manifest["metadata"], {"schemaVersion": "provekit-exam-manifest/v1.1"})

        cid_input = {
            "content": manifest["header"]["content"],
            "metadata": manifest["metadata"],
        }
        self.assertEqual(_blake3_512(_jcs_bytes(cid_input)), embedded_cid)

        questions = manifest["header"]["content"]["questions"]
        self.assertEqual(len(questions), sum(EXPECTED_KIND_COUNTS.values()))
        self.assertEqual(Counter(question["kind"] for question in questions), EXPECTED_KIND_COUNTS)
        self.assertEqual(
            manifest["header"]["content"]["question_kinds"],
            [
                "boundary-realization",
                "boundary-tag",
                "composition",
                "concept-realization",
                "effect-classification",
                "morphism",
                "sort-classification",
            ],
        )

        for question in questions:
            self.assertIn(question.get("kind"), QUESTION_KINDS)
            self.assertIsInstance(question.get("parameters"), dict)
            self.assertTrue(question.get("concept", "").startswith("concept:"))
            self.assertIn(question.get("expected_answer_shape"), EXPECTED_SHAPES)
            self.assertNotIn(question["kind"], {"effect", "realization", "sort"})

        boundary_realizations = [
            question for question in questions if question["kind"] == "boundary-realization"
        ]
        self.assertEqual(
            sorted(question["parameters"]["target_library"] for question in boundary_realizations),
            sorted(NATIVE_LIBRARY_LANGUAGE),
        )
        for question in boundary_realizations:
            parameters = question["parameters"]
            self.assertEqual(
                parameters["target_language"],
                NATIVE_LIBRARY_LANGUAGE[parameters["target_library"]],
            )
            self.assertIn("boundary_contract_cid", parameters)
            self.assertNotIn("target_concept", parameters)

        for question in questions:
            if question["kind"] == "boundary-tag":
                self.assertIn("target_boundary_contract", question["parameters"])
                self.assertNotIn("target_concept", question["parameters"])

if __name__ == "__main__":
    unittest.main()
