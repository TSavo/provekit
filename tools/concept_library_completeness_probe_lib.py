"""Shared helpers for manifest-driven concept-library completeness probes."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


REPO = Path(__file__).resolve().parent.parent
CONCEPT_SHAPES = REPO / "menagerie" / "concept-shapes"
EXAM_DIR = CONCEPT_SHAPES / "exams"
ALGORITHM_DIR = CONCEPT_SHAPES / "catalog" / "algorithms"
REALIZATION_DIR = CONCEPT_SHAPES / "catalog" / "realizations"
GAP_DIR = CONCEPT_SHAPES / "gaps"
DOCS_AUDITS = REPO / "docs" / "audits"

GENERAL_OUT = DOCS_AUDITS / "2026-05-17-concept-library-completeness-probe.md"
OPERATION_OUT = DOCS_AUDITS / "2026-05-17-concept-library-completeness-probe-operation-layer.md"


@dataclass(frozen=True)
class CatalogRecord:
    cid: str
    fn_name: str
    kind: str
    path: Path

    @property
    def rel_path(self) -> str:
        return self.path.relative_to(REPO).as_posix()


@dataclass(frozen=True)
class GapRecord:
    cid: str
    fn_name: str
    gap_kind: str
    source_lang: str
    target_concept: str
    path: Path

    @property
    def rel_path(self) -> str:
        return self.path.relative_to(REPO).as_posix()


@dataclass(frozen=True)
class QuestionResult:
    question: dict[str, Any]
    status: str
    answers: tuple[CatalogRecord, ...]
    gaps: tuple[GapRecord, ...]


@dataclass(frozen=True)
class ManifestBundle:
    path: Path
    cid: str
    content: dict[str, Any]
    questions: list[dict[str, Any]]


class Catalog:
    def __init__(self) -> None:
        self.morphisms: dict[tuple[str, str], list[CatalogRecord]] = defaultdict(list)
        self.sort_morphisms: dict[tuple[str, str], list[CatalogRecord]] = defaultdict(list)
        self.effect_signatures: dict[tuple[str, str], list[CatalogRecord]] = defaultdict(list)
        self.realizations: dict[tuple[str, str, str], list[CatalogRecord]] = defaultdict(list)
        self.boundary_tags: dict[tuple[str, str, str], list[CatalogRecord]] = defaultdict(list)
        self.gaps: dict[tuple[str, str], list[GapRecord]] = defaultdict(list)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def catalog_name_from_path(path: Path) -> str:
    return path.name.split(".blake3-512:", 1)[0]


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


def version_key(path: Path) -> tuple[int, str]:
    match = re.match(r"v(\d+)\.blake3-512:", path.name)
    version = int(match.group(1)) if match else -1
    return (version, path.name)


def discover_manifest() -> Path:
    override = os.environ.get("EXAM_MANIFEST_PATH")
    if override:
        path = Path(override).resolve()
        if not path.is_file():
            raise SystemExit(f"EXAM_MANIFEST_PATH does not exist: {path}")
        return path

    paths = sorted(EXAM_DIR.glob("v*.blake3-512:*.json"), key=version_key)
    if not paths:
        raise SystemExit(f"no exam manifests found under {EXAM_DIR}")
    return paths[-1]


def load_manifest() -> ManifestBundle:
    path = discover_manifest()
    manifest = load_json(path)
    header = manifest["header"]
    content = header["content"]
    questions = content["questions"]
    return ManifestBundle(path=path, cid=header["cid"], content=content, questions=questions)


def output_path(default: Path) -> Path:
    override = os.environ.get("PROBE_OUTPUT_PATH")
    return Path(override).resolve() if override else default


def load_catalog() -> Catalog:
    catalog = Catalog()
    for path in sorted(ALGORITHM_DIR.glob("*.json")):
        payload = load_json(path)
        record = catalog_record(path, payload)
        index_algorithm(catalog, record)

    for path in sorted(REALIZATION_DIR.glob("*.json")):
        payload = load_json(path)
        record = catalog_record(path, payload)
        index_realization(catalog, record)

    for path in sorted(GAP_DIR.glob("*.json")):
        payload = load_json(path)
        gap = gap_record(path, payload)
        if gap.source_lang and gap.target_concept:
            catalog.gaps[(gap.source_lang, gap.target_concept)].append(gap)

    sort_record_lists(catalog.morphisms.values())
    sort_record_lists(catalog.sort_morphisms.values())
    sort_record_lists(catalog.effect_signatures.values())
    sort_record_lists(catalog.realizations.values())
    sort_record_lists(catalog.boundary_tags.values())
    sort_gap_lists(catalog.gaps.values())
    return catalog


def sort_record_lists(lists: Iterable[list[CatalogRecord]]) -> None:
    for records in lists:
        records.sort(key=lambda item: (item.fn_name, item.cid, item.rel_path))


def sort_gap_lists(lists: Iterable[list[GapRecord]]) -> None:
    for records in lists:
        records.sort(key=lambda item: (item.fn_name, item.cid, item.rel_path))


def catalog_record(path: Path, payload: dict[str, Any]) -> CatalogRecord:
    memento = payload.get("memento", payload)
    fn_name = memento.get("fn_name")
    if not isinstance(fn_name, str) or not fn_name:
        fn_name = catalog_name_from_path(path)
    kind = memento.get("kind")
    if not isinstance(kind, str):
        kind = ""
    cid = payload.get("cid")
    if not isinstance(cid, str) or not cid:
        cid = blake3_512(encode_jcs(payload).encode("utf-8"))
    return CatalogRecord(cid=cid, fn_name=fn_name, kind=kind, path=path)


def gap_record(path: Path, payload: dict[str, Any]) -> GapRecord:
    cid = payload.get("cid")
    if not isinstance(cid, str) or not cid:
        cid = blake3_512(encode_jcs(payload).encode("utf-8"))
    fn_name = payload.get("fn_name")
    if not isinstance(fn_name, str):
        fn_name = catalog_name_from_path(path)
    gap_kind = payload.get("gap_kind")
    if not isinstance(gap_kind, str):
        gap_kind = ""
    source_lang = payload.get("source_lang")
    if not isinstance(source_lang, str):
        source_lang = ""
    target_concept = payload.get("target_concept_op")
    if not isinstance(target_concept, str):
        target_concept = ""
    return GapRecord(
        cid=cid,
        fn_name=fn_name,
        gap_kind=gap_kind,
        source_lang=source_lang,
        target_concept=target_concept,
        path=path,
    )


def index_algorithm(catalog: Catalog, record: CatalogRecord) -> None:
    fn_name = record.fn_name
    if fn_name.startswith("morphism:") and ":to:concept:" in fn_name:
        left, concept_name = fn_name.rsplit(":to:concept:", 1)
        parts = left.split(":")
        if len(parts) >= 3:
            language = normalize_language(parts[1])
            catalog.morphisms[(language, f"concept:{concept_name}")].append(record)
        return

    if fn_name.startswith("sort-morphism:"):
        parts = fn_name.split(":")
        if len(parts) >= 3:
            language = normalize_language(parts[1])
            sort_name = parts[-1]
            catalog.sort_morphisms[(language, f"concept:{sort_name}")].append(record)
        return

    if fn_name.startswith("morphism:") and ":to:sort:" in fn_name:
        left, sort_name = fn_name.rsplit(":to:sort:", 1)
        parts = left.split(":")
        if len(parts) >= 2:
            language = normalize_language(parts[1])
            catalog.sort_morphisms[(language, f"concept:{sort_name}")].append(record)
        return

    if fn_name.startswith("effect-signature:"):
        parts = fn_name.split(":")
        if len(parts) >= 3:
            language = normalize_language(parts[1])
            effect_name = parts[-1]
            catalog.effect_signatures[(language, f"concept:{effect_name}")].append(record)
        return

    if fn_name.startswith("boundary-tag:"):
        parts = fn_name.split(":")
        if len(parts) >= 4:
            library = parts[1]
            api = parts[2]
            concept = ":".join(parts[3:])
            catalog.boundary_tags[(library, api, concept)].append(record)


def index_realization(catalog: Catalog, record: CatalogRecord) -> None:
    fn_name = record.fn_name
    if "->" not in fn_name:
        return
    left, right = fn_name.split("->", 1)

    if left.startswith("concept:") and ":" in right:
        language, target = right.split(":", 1)
        catalog.realizations[
            (left, normalize_language(language), normalize_target_name(target))
        ].append(record)
        return

    if right.startswith("concept:") and ":" in left:
        language, source = left.split(":", 1)
        catalog.realizations[
            (right, normalize_language(language), normalize_target_name(source))
        ].append(record)


def normalize_language(language: str) -> str:
    return {"c": "c11", "jvm": "java"}.get(language, language)


def normalize_target_name(name: str) -> str:
    return name.strip()


def classify_question(question: dict[str, Any], catalog: Catalog) -> QuestionResult:
    kind = question["kind"]
    concept = question["concept"]
    parameters = question.get("parameters", {})
    answers: list[CatalogRecord] = []
    gaps: list[GapRecord] = []

    if kind == "morphism":
        language = normalize_language(parameters.get("from_language", ""))
        answers = catalog.morphisms.get((language, concept), [])
        gaps = catalog.gaps.get((language, concept), [])
    elif kind == "sort":
        language = normalize_language(parameters.get("language", ""))
        answers = catalog.sort_morphisms.get((language, concept), [])
    elif kind == "effect":
        language = normalize_language(parameters.get("language", ""))
        answers = catalog.effect_signatures.get((language, concept), [])
    elif kind == "realization":
        language = normalize_language(parameters.get("target_language", ""))
        library = normalize_target_name(parameters.get("target_library", ""))
        answers = catalog.realizations.get((concept, language, library), [])
    elif kind == "boundary-tag":
        library = parameters.get("library", "")
        api = parameters.get("api", "")
        target_concept = parameters.get("target_concept", concept)
        answers = catalog.boundary_tags.get((library, api, target_concept), [])

    if answers:
        status = "answered"
    elif gaps:
        status = "refused"
    else:
        status = "open"
    return QuestionResult(
        question=question,
        status=status,
        answers=tuple(answers),
        gaps=tuple(gaps),
    )


def classify_questions(
    questions: Iterable[dict[str, Any]], catalog: Catalog
) -> list[QuestionResult]:
    return [classify_question(question, catalog) for question in questions]


def question_language(question: dict[str, Any]) -> str:
    parameters = question.get("parameters", {})
    for key in ("from_language", "target_language", "language"):
        value = parameters.get(key)
        if isinstance(value, str):
            return normalize_language(value)
    return ""


def question_label(question: dict[str, Any]) -> str:
    concept = question["concept"]
    parameters = question.get("parameters", {})
    kind = question["kind"]
    if kind == "morphism":
        return f"{parameters.get('from_language', '')} -> {concept}"
    if kind == "sort":
        return f"{parameters.get('language', '')} -> {concept}"
    if kind == "effect":
        return f"{parameters.get('language', '')} -> {concept}"
    if kind == "realization":
        return (
            f"{concept} -> {parameters.get('target_language', '')}/"
            f"{parameters.get('target_library', '')}"
        )
    if kind == "boundary-tag":
        return (
            f"{parameters.get('library', '')}/"
            f"{parameters.get('api', '')} -> {parameters.get('target_concept', concept)}"
        )
    return f"{kind} -> {concept}"


def md_cell(value: object) -> str:
    text = str(value)
    text = text.replace("\n", "<br>")
    text = text.replace("|", "\\|")
    return text


def answer_cids(result: QuestionResult) -> str:
    if not result.answers:
        return ""
    return "<br>".join(record.cid for record in result.answers)


def gap_cids(result: QuestionResult) -> str:
    if result.status != "refused" or not result.gaps:
        return ""
    return "<br>".join(record.cid for record in result.gaps)


def gap_kinds(result: QuestionResult) -> str:
    if result.status != "refused" or not result.gaps:
        return ""
    return ", ".join(sorted({record.gap_kind for record in result.gaps if record.gap_kind}))


def table(lines: list[str], headers: list[str], rows: Iterable[Iterable[object]]) -> None:
    lines.append("| " + " | ".join(md_cell(header) for header in headers) + " |")
    lines.append("|" + "|".join(" --- " for _ in headers) + "|")
    for row in rows:
        lines.append("| " + " | ".join(md_cell(col) for col in row) + " |")
    lines.append("")


def percent(numerator: int, denominator: int) -> str:
    if denominator == 0:
        return "n/a"
    return f"{100 * numerator / denominator:.1f}%"


def status_counts(results: Iterable[QuestionResult]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for result in results:
        counts[result.status] += 1
    return counts


def build_general_report(bundle: ManifestBundle, results: list[QuestionResult]) -> str:
    lines: list[str] = []
    counts = status_counts(results)
    total = len(results)

    lines.extend(
        [
            "# Concept-Library Completeness Probe",
            "",
            "**Audit date**: 2026-05-17",
            f"**Manifest CID**: `{bundle.cid}`",
            f"**Manifest path**: `{bundle.path.relative_to(REPO).as_posix()}`",
            f"**Question count**: {total}",
            "**Generated by**: `tools/concept-library-completeness-probe.py`",
            "**Replayability**: no timestamps; output is sorted by manifest order and stable catalog walks.",
            "",
            "This probe consumes the exam manifest as the question source. Each manifest question is marked answered, refused, or open by looking up catalog answers and transport gap records.",
            "",
            "## 1. Manifest Coverage Summary",
            "",
        ]
    )
    table(
        lines,
        ["status", "questions"],
        [
            ("answered", counts["answered"]),
            ("refused", counts["refused"]),
            ("open", counts["open"]),
            ("total", total),
        ],
    )

    lines.extend(["## 2. Coverage By Kind", ""])
    kind_rows = []
    for kind in sorted({result.question["kind"] for result in results}):
        subset = [result for result in results if result.question["kind"] == kind]
        subset_counts = status_counts(subset)
        kind_rows.append(
            (
                kind,
                len(subset),
                subset_counts["answered"],
                subset_counts["refused"],
                subset_counts["open"],
                percent(subset_counts["answered"], len(subset)),
            )
        )
    table(lines, ["kind", "total", "answered", "refused", "open", "answered %"], kind_rows)

    lines.extend(["## 3. Coverage By Language", ""])
    language_rows = []
    languages = sorted({question_language(result.question) for result in results if question_language(result.question)})
    for language in languages:
        subset = [result for result in results if question_language(result.question) == language]
        subset_counts = status_counts(subset)
        language_rows.append(
            (
                language,
                len(subset),
                subset_counts["answered"],
                subset_counts["refused"],
                subset_counts["open"],
                percent(subset_counts["answered"], len(subset)),
            )
        )
    table(lines, ["language", "total", "answered", "refused", "open", "answered %"], language_rows)

    lines.extend(["## 4. Manifest Question Table", ""])
    table(
        lines,
        ["kind", "question", "status", "answer CID", "gap-record CID", "gap kind"],
        (
            (
                result.question["kind"],
                question_label(result.question),
                result.status,
                answer_cids(result),
                gap_cids(result),
                gap_kinds(result),
            )
            for result in results
        ),
    )

    return "\n".join(lines).rstrip() + "\n"


def build_operation_report(bundle: ManifestBundle, results: list[QuestionResult]) -> str:
    morphism_results = [result for result in results if result.question["kind"] == "morphism"]
    counts = status_counts(morphism_results)
    total = len(morphism_results)
    lines: list[str] = [
        "# Concept-Library Completeness Probe - Operation Layer",
        "",
        "**Audit date**: 2026-05-17",
        f"**Manifest CID**: `{bundle.cid}`",
        f"**Manifest path**: `{bundle.path.relative_to(REPO).as_posix()}`",
        f"**Morphism questions**: {total}",
        "**Generated by**: `tools/concept-library-completeness-probe-operation-layer.py`",
        "**Replayability**: no timestamps; output is sorted by manifest order and stable catalog walks.",
        "",
        "This operation-layer probe consumes only the manifest's `morphism` questions. A row is answered when a catalog morphism reaches the target concept, refused when a transport gap record exists and no answer is present, and open otherwise.",
        "",
        "## 1. Operation-Layer Summary",
        "",
    ]
    table(
        lines,
        ["status", "morphism questions"],
        [
            ("answered", counts["answered"]),
            ("refused", counts["refused"]),
            ("open", counts["open"]),
            ("total", total),
        ],
    )

    lines.extend(["## 2. Per-Language Operation Counts", ""])
    languages = sorted({question_language(result.question) for result in morphism_results})
    rows = []
    for language in languages:
        subset = [result for result in morphism_results if question_language(result.question) == language]
        subset_counts = status_counts(subset)
        rows.append(
            (
                language,
                len(subset),
                subset_counts["answered"],
                subset_counts["refused"],
                subset_counts["open"],
                percent(subset_counts["answered"], len(subset)),
            )
        )
    table(lines, ["language", "total", "answered", "refused", "open", "answered %"], rows)

    lines.extend(["## 3. Per-Language Morphism Tables", ""])
    for language in languages:
        lines.extend([f"### 3.{languages.index(language) + 1} {language}", ""])
        subset = [result for result in morphism_results if question_language(result.question) == language]
        table(
            lines,
            ["morphism", "status", "answer CID", "gap-record CID", "gap kind"],
            (
                (
                    result.question["concept"],
                    result.status,
                    answer_cids(result),
                    gap_cids(result),
                    gap_kinds(result),
                )
                for result in subset
            ),
        )

    lines.extend(["## 4. Cross-Language Morphism Matrix", ""])
    concepts = sorted({result.question["concept"] for result in morphism_results})
    by_pair = {
        (question_language(result.question), result.question["concept"]): result
        for result in morphism_results
    }
    table(
        lines,
        ["concept", *languages],
        (
            [
                concept,
                *[
                    by_pair[(language, concept)].status
                    if (language, concept) in by_pair
                    else ""
                    for language in languages
                ],
            ]
            for concept in concepts
        ),
    )

    return "\n".join(lines).rstrip() + "\n"


def write_report(path: Path, report: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(report, encoding="utf-8")


def run_general_probe() -> Path:
    bundle = load_manifest()
    catalog = load_catalog()
    results = classify_questions(bundle.questions, catalog)
    path = output_path(GENERAL_OUT)
    write_report(path, build_general_report(bundle, results))
    return path


def run_operation_probe() -> Path:
    bundle = load_manifest()
    catalog = load_catalog()
    results = classify_questions(bundle.questions, catalog)
    path = output_path(OPERATION_OUT)
    write_report(path, build_operation_report(bundle, results))
    return path
