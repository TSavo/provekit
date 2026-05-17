#!/usr/bin/env python3
"""Mint the v1 ExamManifestMemento from concept-shapes catalog state."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
SPEC_DIR = BASE / "specs"
CATALOG_DIR = BASE / "catalog"
ABSTRACTION_DIR = CATALOG_DIR / "abstractions"
ALGORITHM_DIR = CATALOG_DIR / "algorithms"
EXAM_DIR = BASE / "exams"
INDEX_PATH = CATALOG_DIR / "index.json"

SCHEMA_VERSION = "provekit-exam-manifest/v1"
CONCEPT_HUB_VERSION = "v1.7.0"

# Fixed v1 mint timestamp. It is intentionally stable so replayed manifest
# files are byte-identical even before the placeholder signature is replaced.
DECLARED_AT = "2026-05-16T00:00:00Z"
PLACEHOLDER_SIGNATURE = "UNSIGNED_DEV_ONLY"
PLACEHOLDER_SIGNER = "UNSIGNED_DEV_ONLY"

LANGUAGES = [
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
]

QUESTION_KINDS = [
    "boundary-tag",
    "composition",
    "effect",
    "morphism",
    "realization",
    "sort",
]

KNOWN_LIBRARIES_BY_CONCEPT = {
    "concept:http-request": [
        {"library": "libcurl", "api": "curl_easy_perform"},
        {"library": "java-httpclient", "api": "java.net.http.HttpClient.send"},
        {"library": "python-urllib", "api": "urllib.request.urlopen"},
        {"library": "javascript-fetch", "api": "fetch"},
        {"library": "python-requests", "api": "requests.get"},
        {"library": "rust-reqwest", "api": "reqwest.get"},
    ],
    "concept:sql-query": [
        {"library": "python-sqlite3", "api": "sqlite3.Cursor.execute"},
        {"library": "python-psycopg2", "api": "psycopg2.cursor.execute"},
        {"library": "python-aiosqlite", "api": "aiosqlite.Connection.execute"},
        {"library": "rust-sqlx", "api": "sqlx.query"},
        {"library": "java-jdbc", "api": "java.sql.PreparedStatement.executeQuery"},
    ],
}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def encode_jcs(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int) and not isinstance(value, bool):
        return str(value)
    if isinstance(value, str):
        return encode_jcs_string(value)
    if isinstance(value, list):
        return "[" + ",".join(encode_jcs(item) for item in value) + "]"
    if isinstance(value, dict):
        items = []
        for key in sorted(value):
            if not isinstance(key, str):
                raise TypeError(f"JCS object key must be a string, got {key!r}")
            items.append(f"{encode_jcs_string(key)}:{encode_jcs(value[key])}")
        return "{" + ",".join(items) + "}"
    raise TypeError(f"JCS cannot encode value {value!r}")


def encode_jcs_string(value: str) -> str:
    out = ['"']
    for char in value:
        code = ord(char)
        if char == '"':
            out.append('\\"')
        elif char == "\\":
            out.append("\\\\")
        elif code < 0x20:
            out.append(f"\\u{code:04x}")
        else:
            out.append(char)
    out.append('"')
    return "".join(out)


def blake3_512(data: bytes) -> str:
    try:
        import blake3  # type: ignore

        digest = blake3.blake3(data).digest(length=64).hex()
        return f"blake3-512:{digest}"
    except ModuleNotFoundError:
        pass

    b3sum = shutil.which("b3sum")
    if b3sum is None:
        raise SystemExit("BLAKE3 unavailable: install python blake3 or provide b3sum")
    proc = subprocess.run(
        [b3sum, "--length", "64"],
        input=data,
        check=True,
        capture_output=True,
    )
    digest = proc.stdout.decode("utf-8").split()[0]
    return f"blake3-512:{digest}"


def catalog_name_from_path(path: Path) -> str:
    return path.name.split(".blake3-512:", 1)[0]


def load_primitive_concepts() -> list[str]:
    shape_concepts: set[str] = set()
    for path in sorted(SPEC_DIR.glob("*_shape.spec.json")):
        if path.name.startswith("morphism_"):
            continue
        spec = load_json(path)
        fn_name = spec.get("fn_name")
        if isinstance(fn_name, str) and fn_name.startswith("concept:"):
            shape_concepts.add(fn_name)

    morphism_targets: set[str] = set()
    for path in sorted(ALGORITHM_DIR.glob("morphism:*:to:concept:*.json")):
        name = catalog_name_from_path(path)
        if ":to:concept:" in name:
            morphism_targets.add("concept:" + name.rsplit(":to:concept:", 1)[1])

    missing_shapes = sorted(morphism_targets - shape_concepts)
    if missing_shapes:
        raise SystemExit(
            "morphism targets missing shape specs: " + ", ".join(missing_shapes)
        )

    return sorted(shape_concepts & morphism_targets)


def load_abstraction_concepts() -> list[str]:
    concepts: set[str] = set()
    for path in sorted(ABSTRACTION_DIR.glob("*.json")):
        payload = load_json(path)
        memento = payload.get("memento", payload)
        name = memento.get("operator") or memento.get("name")
        if isinstance(name, str) and name.startswith("concept:"):
            concepts.add(name)
    return sorted(concepts)


def load_known_library_concepts() -> list[str]:
    shape_concepts = {
        load_json(path).get("fn_name")
        for path in sorted(SPEC_DIR.glob("*.spec.json"))
    }
    concepts = sorted(
        concept
        for concept in KNOWN_LIBRARIES_BY_CONCEPT
        if concept in shape_concepts or concept in load_abstraction_concepts()
    )
    missing = sorted(set(KNOWN_LIBRARIES_BY_CONCEPT) - set(concepts))
    if missing:
        raise SystemExit("known library concepts missing from inputs: " + ", ".join(missing))
    return concepts


def load_sort_concepts() -> list[str]:
    sorts: set[str] = set()
    for path in sorted(SPEC_DIR.glob("sort-instance_*.spec.json")):
        spec = load_json(path)
        post = spec.get("post", {})
        name = post.get("name") or spec.get("fn_name")
        if isinstance(name, str) and name:
            sorts.add(f"concept:{name}")
    return sorted(sorts)


def load_covered_sort_pairs() -> set[tuple[str, str]]:
    covered: set[tuple[str, str]] = set()
    for path in sorted(ALGORITHM_DIR.glob("*.json")):
        name = catalog_name_from_path(path)
        if name.startswith("sort-morphism:"):
            parts = name.split(":")
            if len(parts) >= 3:
                covered.add((parts[2], f"concept:{parts[-1]}"))
        elif name.startswith("morphism:") and ":to:sort:" in name:
            left, sort_name = name.rsplit(":to:sort:", 1)
            parts = left.split(":")
            if len(parts) >= 2:
                covered.add((parts[1], f"concept:{sort_name}"))
    return covered


def load_effect_concepts() -> list[str]:
    effects: set[str] = set()
    for path in sorted(SPEC_DIR.glob("*.spec.json")):
        spec = load_json(path)
        effect_items = (spec.get("effects") or {}).get("effects") or []
        for item in effect_items:
            if not isinstance(item, dict):
                continue
            kind = item.get("kind")
            if kind == "effect-signature" and isinstance(item.get("name"), str):
                effects.add(item["name"])
            elif isinstance(kind, str) and kind != "effect-polymorphic":
                effects.add(kind)
    return [f"concept:{name}" for name in sorted(effects)]


def make_question(
    kind: str,
    concept: str,
    parameters: dict[str, Any],
    expected_answer_shape: str,
) -> dict[str, Any]:
    return {
        "concept": concept,
        "expected_answer_shape": expected_answer_shape,
        "kind": kind,
        "parameters": parameters,
    }


def morphism_questions() -> list[dict[str, Any]]:
    return [
        make_question(
            "morphism",
            concept,
            {"from_language": language},
            "MorphismMemento",
        )
        for concept in load_primitive_concepts()
        for language in LANGUAGES
    ]


def realization_questions() -> list[dict[str, Any]]:
    questions = []
    abstraction_concepts = set(load_abstraction_concepts())
    for concept in load_known_library_concepts():
        if concept not in abstraction_concepts and concept not in KNOWN_LIBRARIES_BY_CONCEPT:
            continue
        for language in LANGUAGES:
            for library_entry in KNOWN_LIBRARIES_BY_CONCEPT[concept]:
                questions.append(
                    make_question(
                        "realization",
                        concept,
                        {
                            "target_language": language,
                            "target_library": library_entry["library"],
                        },
                        "RealizationMemento",
                    )
                )
    return questions


def sort_questions() -> list[dict[str, Any]]:
    questions = []
    covered = load_covered_sort_pairs()
    for concept in load_sort_concepts():
        for language in LANGUAGES:
            if (language, concept) in covered:
                continue
            questions.append(
                make_question(
                    "sort",
                    concept,
                    {"language": language},
                    "SortMorphismMemento",
                )
            )
    return questions


def effect_questions() -> list[dict[str, Any]]:
    return [
        make_question(
            "effect",
            concept,
            {"language": language},
            "EffectSignatureMemento",
        )
        for concept in load_effect_concepts()
        for language in LANGUAGES
    ]


def boundary_tag_questions() -> list[dict[str, Any]]:
    questions = []
    for concept in load_known_library_concepts():
        for library_entry in KNOWN_LIBRARIES_BY_CONCEPT[concept]:
            questions.append(
                make_question(
                    "boundary-tag",
                    concept,
                    {
                        "api": library_entry["api"],
                        "library": library_entry["library"],
                        "target_concept": concept,
                    },
                    "BoundaryTagMemento",
                )
            )
    return questions


def question_sort_key(question: dict[str, Any]) -> tuple[str, str, str, str]:
    return (
        question["kind"],
        question["concept"],
        encode_jcs(question["parameters"]),
        question["expected_answer_shape"],
    )


def build_questions() -> list[dict[str, Any]]:
    questions = []
    questions.extend(boundary_tag_questions())
    questions.extend(effect_questions())
    questions.extend(morphism_questions())
    questions.extend(realization_questions())
    questions.extend(sort_questions())
    return sorted(questions, key=question_sort_key)


def build_content() -> dict[str, Any]:
    return {
        "concept_hub_version": CONCEPT_HUB_VERSION,
        "question_kinds": sorted(QUESTION_KINDS),
        "questions": build_questions(),
    }


def manifest_cid(metadata: dict[str, Any], content: dict[str, Any]) -> str:
    payload = {"content": content, "metadata": metadata}
    return blake3_512(encode_jcs(payload).encode("utf-8"))


def build_manifest() -> dict[str, Any]:
    metadata = {"schemaVersion": SCHEMA_VERSION}
    content = build_content()
    cid = manifest_cid(metadata, content)
    return {
        "envelope": {
            "declaredAt": DECLARED_AT,
            "signature": PLACEHOLDER_SIGNATURE,
            "signer": PLACEHOLDER_SIGNER,
        },
        "header": {
            "cid": cid,
            "content": content,
        },
        "metadata": metadata,
    }


def write_manifest(output_dir: Path, manifest: dict[str, Any]) -> Path:
    cid = manifest["header"]["cid"]
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"v1.{cid}.json"
    output_bytes = encode_jcs(manifest).encode("utf-8")
    output_path.write_bytes(output_bytes)
    for old_path in output_dir.glob("v1.blake3-512:*.json"):
        if old_path != output_path:
            old_path.unlink()
    return output_path


def update_catalog_index(cid: str, manifest_path: Path) -> None:
    index = load_json(INDEX_PATH)
    entries = dict(index.get("entries", {}))
    rel_path = manifest_path.relative_to(BASE).as_posix()
    entries[cid] = {
        "cid": cid,
        "kind": "exam",
        "name": "exam-manifest-v1",
        "path": rel_path,
    }
    index["entries"] = {key: entries[key] for key in sorted(entries)}
    INDEX_PATH.write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=EXAM_DIR,
        help="directory for the v1.<cid>.json output",
    )
    parser.add_argument(
        "--skip-index",
        action="store_true",
        help="do not update catalog/index.json",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    output_dir = args.output_dir.resolve()
    default_dir = EXAM_DIR.resolve()
    if output_dir != default_dir and not args.skip_index:
        raise SystemExit("--skip-index is required when --output-dir is not the catalog exam dir")

    manifest = build_manifest()
    output_path = write_manifest(output_dir, manifest)
    cid = manifest["header"]["cid"]
    if not args.skip_index:
        update_catalog_index(cid, output_path)

    question_count = len(manifest["header"]["content"]["questions"])
    try:
        display_path = output_path.relative_to(ROOT)
    except ValueError:
        display_path = output_path
    print(f"exam_manifest\t{cid}\t{display_path}\t{question_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
