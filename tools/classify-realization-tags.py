#!/usr/bin/env python3
"""Generate the realization tag-kind classification audit."""

from __future__ import annotations

import json
import os
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from concept_library_completeness_probe_lib import blake3_512, encode_jcs, md_cell, table


REPO = Path(__file__).resolve().parent.parent
CONCEPT_SHAPES = REPO / "menagerie" / "concept-shapes"
SPEC_DIR = CONCEPT_SHAPES / "specs"
ALGORITHM_DIR = CONCEPT_SHAPES / "catalog" / "algorithms"
REALIZATION_DIR = CONCEPT_SHAPES / "catalog" / "realizations"
GAP_DIR = CONCEPT_SHAPES / "gaps"
AUDIT_OUT = REPO / "docs" / "audits" / "2026-05-17-realization-tag-classification.md"

QUESTION_REFERENCE_CID = "blake3-512:0e012db4ce35b235b8482344795ccbe8bccad51522825b5c495a862648736936497b11a940cf0ba9170ee6202849e9a8dc9eca5cb3021261ffa2f4ac4df6edc1"
LANGUAGES = (
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
)
TAG_KIND_ORDER = ("first-class", "composition", "boundary", "sugar-carrier", "absent")
COMPOSITION_DEPS = {
    "concept:ge": ("concept:lt", "concept:not"),
    "concept:gt": ("concept:lt",),
    "concept:le": ("concept:gt", "concept:not"),
    "concept:ne": ("concept:eq", "concept:not"),
    "concept:postdec": ("concept:assign", "concept:sub"),
    "concept:postinc": ("concept:add", "concept:assign"),
    "concept:predec": ("concept:assign", "concept:sub"),
    "concept:preinc": ("concept:add", "concept:assign"),
}


@dataclass(frozen=True)
class EvidenceRecord:
    fn_name: str
    cid: str
    path: Path
    detail: str
    target: str = ""

    @property
    def rel_path(self) -> str:
        return self.path.relative_to(REPO).as_posix()


@dataclass(frozen=True)
class GapRecord:
    fn_name: str
    cid: str
    path: Path
    gap_kind: str

    @property
    def rel_path(self) -> str:
        return self.path.relative_to(REPO).as_posix()


@dataclass(frozen=True)
class ClassificationRow:
    language: str
    concept: str
    concept_class: str
    tag_kind: str
    evidence: str
    source_paths: tuple[str, ...]


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def catalog_name_from_path(path: Path) -> str:
    return path.name.split(".blake3-512:", 1)[0]


def normalize_language(language: str) -> str:
    return {"c": "c11", "jvm": "java", "ts": "typescript"}.get(language, language)


def output_path() -> Path:
    override = os.environ.get("REALIZATION_TAG_CLASSIFICATION_OUTPUT_PATH")
    return Path(override).resolve() if override else AUDIT_OUT


def payload_cid(path: Path, payload: dict[str, Any]) -> str:
    cid = payload.get("cid")
    if isinstance(cid, str) and cid:
        return cid
    return blake3_512(encode_jcs(payload).encode("utf-8"))


def file_cid(path: Path) -> str:
    return blake3_512(path.read_bytes())


def record_fn_name(path: Path, payload: dict[str, Any]) -> str:
    memento = payload.get("memento", payload)
    fn_name = memento.get("fn_name")
    if isinstance(fn_name, str) and fn_name:
        return fn_name
    return catalog_name_from_path(path)


def discover_primitive_concepts() -> tuple[list[str], dict[str, set[str]]]:
    concepts: set[str] = set()
    source_specs: dict[str, set[str]] = defaultdict(set)
    for path in sorted(SPEC_DIR.glob("morphism_*.spec.json")):
        payload = load_json(path)
        fn_name = payload.get("fn_name")
        if not isinstance(fn_name, str):
            continue
        if not fn_name.startswith("morphism:") or ":to:concept:" not in fn_name:
            continue
        left, concept_name = fn_name.rsplit(":to:concept:", 1)
        parts = left.split(":")
        if len(parts) < 3:
            continue
        language = normalize_language(parts[1])
        if language not in LANGUAGES:
            continue
        concept = f"concept:{concept_name}"
        concepts.add(concept)
        source_specs[concept].add(path.relative_to(REPO).as_posix())
    return sorted(concepts), source_specs


def concept_shape_path(concept: str) -> Path | None:
    name = concept.removeprefix("concept:")
    path = SPEC_DIR / f"{name}_shape.spec.json"
    return path if path.is_file() else None


def concept_op_def_path(concept: str) -> Path | None:
    path = SPEC_DIR / "op-definitions" / f"{concept}.op-def.ccl.json"
    return path if path.is_file() else None


def primitive_definition_sources(concept: str) -> tuple[str, ...]:
    paths: list[str] = []
    for path in (concept_shape_path(concept), concept_op_def_path(concept)):
        if path is not None:
            paths.append(path.relative_to(REPO).as_posix())
    return tuple(paths)


def morphism_detail(fn_name: str, payload: dict[str, Any]) -> str:
    left, _concept_name = fn_name.rsplit(":to:concept:", 1)
    parts = left.split(":")
    surface = ":".join(parts[2:]) if len(parts) >= 3 else ""
    memento = payload.get("memento", payload)
    post = memento.get("post", {})
    if not isinstance(post, dict):
        post = {}
    operator_map = post.get("operator_map")
    literal_map = post.get("literal_map")
    wp_note = post.get("wp_note") or memento.get("wp_note")

    pieces = [f"surface={surface}"] if surface else []
    if isinstance(operator_map, dict) and operator_map:
        mapping = ", ".join(f"{key}->{operator_map[key]}" for key in sorted(operator_map))
        pieces.append(f"operator_map={mapping}")
    if isinstance(literal_map, dict) and literal_map:
        mapping = ", ".join(f"{key}->{literal_map[key]}" for key in sorted(literal_map))
        pieces.append(f"literal_map={mapping}")
    if isinstance(wp_note, str) and wp_note:
        pieces.append("wp_note=present")
    return "; ".join(pieces) if pieces else "morphism present"


def load_morphisms() -> dict[tuple[str, str], list[EvidenceRecord]]:
    records: dict[tuple[str, str], list[EvidenceRecord]] = defaultdict(list)
    for path in sorted(ALGORITHM_DIR.glob("*.json")):
        payload = load_json(path)
        fn_name = record_fn_name(path, payload)
        if not fn_name.startswith("morphism:") or ":to:concept:" not in fn_name:
            continue
        left, concept_name = fn_name.rsplit(":to:concept:", 1)
        parts = left.split(":")
        if len(parts) < 3:
            continue
        language = normalize_language(parts[1])
        if language not in LANGUAGES:
            continue
        concept = f"concept:{concept_name}"
        records[(language, concept)].append(
            EvidenceRecord(
                fn_name=fn_name,
                cid=payload_cid(path, payload),
                path=path,
                detail=morphism_detail(fn_name, payload),
                target=":".join(parts[2:]),
            )
        )
    sort_evidence(records.values())
    return records


def load_realizations() -> dict[tuple[str, str], list[EvidenceRecord]]:
    records: dict[tuple[str, str], list[EvidenceRecord]] = defaultdict(list)
    for path in sorted(REALIZATION_DIR.glob("*.json")):
        payload = load_json(path)
        fn_name = record_fn_name(path, payload)
        if "->" not in fn_name:
            continue
        left, right = fn_name.split("->", 1)
        concept = ""
        language = ""
        target = ""
        if left.startswith("concept:") and ":" in right:
            language, target = right.split(":", 1)
            concept = left
        elif right.startswith("concept:") and ":" in left:
            language, target = left.split(":", 1)
            concept = right
        language = normalize_language(language)
        if language not in LANGUAGES or not concept:
            continue
        detail = f"realization_target={target}" if target else "realization present"
        records[(language, concept)].append(
            EvidenceRecord(
                fn_name=fn_name,
                cid=payload_cid(path, payload),
                path=path,
                detail=detail,
                target=target,
            )
        )
    sort_evidence(records.values())
    return records


def load_gaps() -> dict[tuple[str, str], list[GapRecord]]:
    records: dict[tuple[str, str], list[GapRecord]] = defaultdict(list)
    for path in sorted(GAP_DIR.glob("*.json")):
        payload = load_json(path)
        source_lang = payload.get("source_lang")
        target_concept = payload.get("target_concept_op")
        if not isinstance(source_lang, str) or not isinstance(target_concept, str):
            continue
        language = normalize_language(source_lang)
        if language not in LANGUAGES:
            continue
        fn_name = payload.get("fn_name")
        if not isinstance(fn_name, str) or not fn_name:
            fn_name = catalog_name_from_path(path)
        gap_kind = payload.get("gap_kind")
        if not isinstance(gap_kind, str):
            gap_kind = ""
        records[(language, target_concept)].append(
            GapRecord(
                fn_name=fn_name,
                cid=payload_cid(path, payload),
                path=path,
                gap_kind=gap_kind,
            )
        )
    for items in records.values():
        items.sort(key=lambda item: (item.gap_kind, item.fn_name, item.cid, item.rel_path))
    return records


def sort_evidence(groups: Iterable[list[EvidenceRecord]]) -> None:
    for records in groups:
        records.sort(key=lambda item: (item.fn_name, item.cid, item.rel_path))


def abstraction_concepts(
    realizations: dict[tuple[str, str], list[EvidenceRecord]],
    primitive_concepts: Iterable[str],
) -> tuple[list[str], dict[str, set[str]]]:
    primitive_set = set(primitive_concepts)
    concepts = sorted({concept for _language, concept in realizations if concept not in primitive_set})
    sources: dict[str, set[str]] = defaultdict(set)
    for (_language, concept), records in realizations.items():
        if concept in concepts:
            for record in records:
                sources[concept].add(record.rel_path)
    return concepts, sources


def has_positive_evidence(
    language: str,
    concept: str,
    morphisms: dict[tuple[str, str], list[EvidenceRecord]],
    realizations: dict[tuple[str, str], list[EvidenceRecord]],
) -> bool:
    return bool(morphisms.get((language, concept)) or realizations.get((language, concept)))


def composition_evidence(
    language: str,
    concept: str,
    morphisms: dict[tuple[str, str], list[EvidenceRecord]],
    realizations: dict[tuple[str, str], list[EvidenceRecord]],
    gaps: dict[tuple[str, str], list[GapRecord]],
) -> str | None:
    deps = COMPOSITION_DEPS.get(concept)
    if not deps:
        return None
    for dep in deps:
        if gaps.get((language, dep)):
            return None
        if not has_positive_evidence(language, dep, morphisms, realizations):
            return None
    return "composition_deps=" + ", ".join(deps)


def source_file_paths(
    tag_kind: str,
    concept_class: str,
    concept: str,
    evidence: Iterable[EvidenceRecord],
    gaps: Iterable[GapRecord],
    abstraction_sources: dict[str, set[str]],
) -> tuple[str, ...]:
    paths: set[str] = set()
    for record in evidence:
        paths.add(record.rel_path)
    for record in gaps:
        paths.add(record.rel_path)
    if concept_class == "primitive":
        paths.update(primitive_definition_sources(concept))
    elif tag_kind == "sugar-carrier":
        paths.update(abstraction_sources.get(concept, set()))
    return tuple(sorted(paths))


def classify_rows() -> tuple[list[ClassificationRow], list[str], list[str]]:
    primitive_concepts, _primitive_source_specs = discover_primitive_concepts()
    morphisms = load_morphisms()
    realizations = load_realizations()
    gaps = load_gaps()
    abstraction_list, abstraction_sources = abstraction_concepts(realizations, primitive_concepts)

    concepts = [(concept, "primitive") for concept in primitive_concepts]
    concepts.extend((concept, "abstraction") for concept in abstraction_list)

    rows: list[ClassificationRow] = []
    for language in LANGUAGES:
        for concept, concept_class in concepts:
            pair = (language, concept)
            morphism_records = morphisms.get(pair, [])
            realization_records = realizations.get(pair, [])
            gap_records = gaps.get(pair, [])
            if morphism_records:
                tag_kind = "first-class"
                evidence = "; ".join(record.detail for record in morphism_records)
                source_paths = source_file_paths(
                    tag_kind, concept_class, concept, morphism_records, (), abstraction_sources
                )
            elif realization_records:
                tag_kind = "boundary"
                evidence = "; ".join(record.detail for record in realization_records)
                source_paths = source_file_paths(
                    tag_kind, concept_class, concept, realization_records, (), abstraction_sources
                )
            elif gap_records:
                tag_kind = "absent"
                gap_kinds = sorted({record.gap_kind for record in gap_records if record.gap_kind})
                evidence = "gap_kinds=" + ", ".join(gap_kinds) if gap_kinds else "gap present"
                source_paths = source_file_paths(
                    tag_kind, concept_class, concept, (), gap_records, abstraction_sources
                )
            else:
                composition = composition_evidence(language, concept, morphisms, realizations, gaps)
                if composition:
                    tag_kind = "composition"
                    evidence = composition
                else:
                    tag_kind = "sugar-carrier"
                    evidence = "no morphism, realization, gap, or composition dependency"
                source_paths = source_file_paths(
                    tag_kind, concept_class, concept, (), (), abstraction_sources
                )
            rows.append(
                ClassificationRow(
                    language=language,
                    concept=concept,
                    concept_class=concept_class,
                    tag_kind=tag_kind,
                    evidence=evidence,
                    source_paths=source_paths,
                )
            )
    return rows, primitive_concepts, abstraction_list


def sources_for_payload(row: ClassificationRow) -> list[dict[str, str]]:
    out: list[dict[str, str]] = []
    for rel_path in row.source_paths:
        path = REPO / rel_path
        out.append({"path": rel_path, "cid": file_cid(path)})
    return out


def classification_payload(
    rows: list[ClassificationRow],
    primitive_concepts: list[str],
    abstraction_concepts_: list[str],
) -> dict[str, Any]:
    return {
        "audit": "realization-tag-classification",
        "schema_version": "1",
        "question_reference": QUESTION_REFERENCE_CID,
        "languages": list(LANGUAGES),
        "primitive_concepts": primitive_concepts,
        "abstraction_concepts": abstraction_concepts_,
        "rows": [
            {
                "language": row.language,
                "concept": row.concept,
                "concept_class": row.concept_class,
                "tag_kind": row.tag_kind,
                "evidence": row.evidence,
                "sources": sources_for_payload(row),
            }
            for row in rows
        ],
    }


def source_cell(paths: Iterable[str]) -> str:
    return "<br>".join(f"`{path}`" for path in paths)


def build_report(
    rows: list[ClassificationRow],
    primitive_concepts: list[str],
    abstraction_concepts_: list[str],
    classification_cid: str,
) -> str:
    lines: list[str] = [
        "# Realization Tag Classification Audit",
        "",
        "**Audit date**: 2026-05-17",
        f"**Question reference**: `{QUESTION_REFERENCE_CID}`",
        f"**Classification CID**: `{classification_cid}`",
        "**Generated by**: `tools/classify-realization-tags.py`",
        "**Replayability**: no timestamps; output is sorted by language, concept class, and concept name.",
        "",
        "This audit classifies the current concept-shape catalog entries by tag kind for the ten actively minted languages. It reads `menagerie/concept-shapes/specs/`, `menagerie/concept-shapes/catalog/algorithms/`, `menagerie/concept-shapes/catalog/realizations/`, and `menagerie/concept-shapes/gaps/`.",
        "",
        "The Classification CID is computed over a JCS payload containing every classification row and the raw BLAKE3-512 CID of each cited source file.",
        "",
        "Positive catalog evidence is applied before gap evidence. This keeps a language/concept pair first-class or boundary when a narrower gap record also exists for another surface of the same concept.",
        "",
        "## 1. Summary",
        "",
    ]

    table(
        lines,
        ["metric", "value"],
        [
            ("active languages", len(LANGUAGES)),
            ("primitive concepts", len(primitive_concepts)),
            ("abstraction concepts", len(abstraction_concepts_)),
            ("concepts per language", len(primitive_concepts) + len(abstraction_concepts_)),
            ("total rows", len(rows)),
        ],
    )

    counts = Counter(row.tag_kind for row in rows)
    lines.extend(["## 2. Tag-Kind Counts", ""])
    table(lines, ["tag-kind", "rows"], ((kind, counts[kind]) for kind in TAG_KIND_ORDER))

    lines.extend(["## 3. Per-Language Summary", ""])
    summary_rows = []
    for language in LANGUAGES:
        subset = [row for row in rows if row.language == language]
        subset_counts = Counter(row.tag_kind for row in subset)
        summary_rows.append(
            [
                language,
                *[subset_counts[kind] for kind in TAG_KIND_ORDER],
                len(subset),
            ]
        )
    table(lines, ["language", *TAG_KIND_ORDER, "total"], summary_rows)

    lines.extend(["## 4. Per-Language Classifications", ""])
    for language in LANGUAGES:
        lines.extend([f"### {language}", ""])
        subset = [row for row in rows if row.language == language]
        table(
            lines,
            ["concept", "class", "tag-kind", "evidence", "source files"],
            (
                (
                    row.concept,
                    row.concept_class,
                    row.tag_kind,
                    row.evidence,
                    source_cell(row.source_paths),
                )
                for row in subset
            ),
        )

    return "\n".join(lines).rstrip() + "\n"


def write_report(path: Path, report: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(report, encoding="utf-8")


def run_classification() -> Path:
    rows, primitive_concepts, abstraction_concepts_ = classify_rows()
    payload = classification_payload(rows, primitive_concepts, abstraction_concepts_)
    classification_cid = blake3_512(encode_jcs(payload).encode("utf-8"))
    report = build_report(rows, primitive_concepts, abstraction_concepts_, classification_cid)
    path = output_path()
    write_report(path, report)
    return path


def main() -> int:
    path = run_classification()
    print(f"Written: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
