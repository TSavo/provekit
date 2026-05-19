"""Executable checks for the v1.1 ExamManifestMemento wire schema."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
SPEC_PATH = ROOT / "menagerie" / "concept-shapes" / "specs" / "exam-manifest_shape.spec.json"

SCHEMA_VERSION = "provekit-exam-manifest/v1.1"
LEGACY_SCHEMA_VERSION = "provekit-exam-manifest/v1"

QUESTION_KINDS = [
    "concept-realization",
    "boundary-realization",
    "boundary-tag",
    "sort-classification",
    "effect-classification",
    "morphism",
    "composition",
    "literal-encoding",
]
LIVE_V1_1_QUESTION_KINDS = sorted(set(QUESTION_KINDS) - {"composition"})
LEGACY_V1_QUESTION_KINDS = [
    "boundary-tag",
    "composition",
    "effect",
    "morphism",
    "realization",
    "sort",
]
LEGACY_KIND_LABELS = {"effect", "realization", "sort"}

PARAMETERS_BY_KIND = {
    "boundary-realization": {
        "boundary_contract_cid",
        "target_language",
        "target_library",
    },
    "boundary-tag": {
        "api",
        "library",
        "target_boundary_contract",
    },
    "composition": set(),
    "concept-realization": {"language"},
    "effect-classification": {"language"},
    "literal-encoding": {"language"},
    "morphism": {"from_language"},
    "sort-classification": {"language"},
}
EXPECTED_ANSWER_SHAPE_BY_KIND = {
    "boundary-realization": "BoundaryRealizationMemento",
    "boundary-tag": "BoundaryTagMemento",
    "concept-realization": "RealizationMemento",
    "effect-classification": "EffectSignatureMemento",
    "literal-encoding": "LiteralEncodingMemento",
    "morphism": "MorphismMemento",
    "sort-classification": "SortMorphismMemento",
}


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _parse_manifest(raw: str) -> dict[str, Any]:
    manifest = json.loads(raw)
    if not isinstance(manifest, dict):
        raise TypeError("exam manifest must be a JSON object")
    return manifest


def _small_v1_1_manifest() -> dict[str, Any]:
    return {
        "envelope": {
            "declaredAt": "2026-05-17T00:00:00Z",
            "signature": "UNSIGNED_DEV_ONLY",
            "signer": "UNSIGNED_DEV_ONLY",
        },
        "header": {
            "cid": "blake3-512:" + ("0" * 128),
            "content": {
                "concept_hub_version": "v1.7.0",
                "question_kinds": sorted(QUESTION_KINDS),
                "questions": [
                    {
                        "concept": "concept:http-request",
                        "expected_answer_shape": "BoundaryRealizationMemento",
                        "kind": "boundary-realization",
                        "parameters": {
                            "boundary_contract_cid": "blake3-512:" + ("1" * 128),
                            "target_language": "python",
                            "target_library": "python-requests",
                        },
                    },
                    {
                        "concept": "concept:http-request",
                        "expected_answer_shape": "BoundaryTagMemento",
                        "kind": "boundary-tag",
                        "parameters": {
                            "api": "requests.get",
                            "library": "python-requests",
                            "target_boundary_contract": "blake3-512:" + ("1" * 128),
                        },
                    },
                    {
                        "concept": "concept:http-request",
                        "expected_answer_shape": "RealizationMemento",
                        "kind": "concept-realization",
                        "parameters": {
                            "language": "python",
                        },
                    },
                    {
                        "concept": "concept:NetworkRequest",
                        "expected_answer_shape": "EffectSignatureMemento",
                        "kind": "effect-classification",
                        "parameters": {
                            "language": "python",
                        },
                    },
                    {
                        "concept": "concept:add",
                        "expected_answer_shape": "MorphismMemento",
                        "kind": "morphism",
                        "parameters": {
                            "from_language": "python",
                        },
                    },
                    {
                        "concept": "concept:Int",
                        "expected_answer_shape": "SortMorphismMemento",
                        "kind": "sort-classification",
                        "parameters": {
                            "language": "python",
                        },
                    },
                    {
                        "concept": "concept:Int",
                        "expected_answer_shape": "LiteralEncodingMemento",
                        "kind": "literal-encoding",
                        "parameters": {
                            "language": "python",
                        },
                    },
                ],
            },
        },
        "metadata": {
            "schemaVersion": SCHEMA_VERSION,
        },
    }


class ExamManifestSchemaV11Tests(unittest.TestCase):
    def test_shape_spec_declares_v1_1_wire_contract(self) -> None:
        spec = _load_json(SPEC_PATH)
        wire_format = spec["wire_format"]

        self.assertEqual(spec["fn_name"], "concept:exam-manifest")
        self.assertEqual(spec["schemaVersion"], SCHEMA_VERSION)
        self.assertEqual(
            wire_format["schema_versions"],
            [LEGACY_SCHEMA_VERSION, SCHEMA_VERSION],
        )
        self.assertEqual(wire_format["question_kinds"], QUESTION_KINDS)
        self.assertFalse(LEGACY_KIND_LABELS & set(wire_format["question_kinds"]))
        self.assertEqual(wire_format["question_parameter_schemas"], {
            kind: sorted(keys) for kind, keys in PARAMETERS_BY_KIND.items()
        })

        wp_note = spec["post"]["wp_note"]
        self.assertIn(SCHEMA_VERSION, wp_note)
        self.assertIn("concept-realization", wp_note)
        self.assertIn("boundary-realization", wp_note)

    def test_parse_small_v1_1_manifest_and_verify_question_wire_format(self) -> None:
        raw = json.dumps(
            _small_v1_1_manifest(),
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        )
        manifest = _parse_manifest(raw)

        self.assertEqual(manifest["metadata"], {"schemaVersion": SCHEMA_VERSION})
        content = manifest["header"]["content"]
        self.assertEqual(content["question_kinds"], sorted(QUESTION_KINDS))

        questions = content["questions"]
        self.assertEqual(
            sorted(question["kind"] for question in questions),
            LIVE_V1_1_QUESTION_KINDS,
        )
        self.assertNotIn("composition", {question["kind"] for question in questions})

        for question in questions:
            kind = question["kind"]
            self.assertEqual(
                set(question["parameters"]),
                PARAMETERS_BY_KIND[kind],
            )
            self.assertEqual(
                question["expected_answer_shape"],
                EXPECTED_ANSWER_SHAPE_BY_KIND[kind],
            )

    def test_v1_manifest_version_remains_parseable(self) -> None:
        legacy_manifest = _small_v1_1_manifest()
        legacy_manifest["metadata"] = {"schemaVersion": LEGACY_SCHEMA_VERSION}
        legacy_manifest["header"]["content"]["question_kinds"] = LEGACY_V1_QUESTION_KINDS
        legacy_manifest["header"]["content"]["questions"] = []

        raw = json.dumps(
            legacy_manifest,
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        )
        manifest = _parse_manifest(raw)

        self.assertEqual(manifest["metadata"]["schemaVersion"], LEGACY_SCHEMA_VERSION)
        self.assertEqual(
            manifest["header"]["content"]["question_kinds"],
            LEGACY_V1_QUESTION_KINDS,
        )


if __name__ == "__main__":
    unittest.main()
