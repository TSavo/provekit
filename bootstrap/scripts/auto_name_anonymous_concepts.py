#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
OUT_DIR = REPO_ROOT / "bootstrap/auto-named-concepts"
DEFAULT_RECEIPT_PATH = OUT_DIR / "receipt.json"
README_PATH = OUT_DIR / "README.md"

ANONYMOUS_RE = re.compile(r"\bUNNAMED-CONCEPT-[0-9A-Fa-f]+\b")
CONCEPT_COMMENT_RE = re.compile(r"//\s*concept:\s*(UNNAMED-CONCEPT-[0-9A-Fa-f]+)\b")
SUGAR_CONCEPT_RE = re.compile(r"//\s*sugar:concept\[blake3-512:[0-9A-Fa-f]+\]\([^)]*\)")
KEBAB_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")

SKIP_DIRS = {
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "vendor",
}

FUNCTION_PATTERNS = [
    re.compile(r"\b(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
    re.compile(r"\bdef\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
    re.compile(r"\bfunction\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
    re.compile(r"\bfunc\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\("),
    re.compile(r"\bfun\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
]

C_LIKE_CONTROL_WORDS = {
    "catch",
    "else",
    "for",
    "if",
    "return",
    "switch",
    "while",
}


@dataclass(frozen=True)
class Anchor:
    function_name: str
    function_line: int | None
    signature: str
    context: str


@dataclass(frozen=True)
class TagOccurrence:
    source_file: Path
    line_index: int
    tag_kind: str
    anonymous_name: str
    original_line: str
    anchor: Anchor
    prompt: str


@dataclass(frozen=True)
class NamingTask:
    key: tuple[str, str, int | None, str]
    anonymous_name: str
    source_file: Path
    anchor: Anchor
    prompt: str


class StubNamer:
    def __init__(self, mode: str) -> None:
        self.mode = mode

    def propose(self, task: NamingTask) -> str:
        base = task.anchor.function_name
        if not base or base.startswith("unknown-at-line-"):
            base = task.anonymous_name
        proposed = to_kebab_case(base)
        if proposed.startswith("unnamed-concept-"):
            proposed = f"anonymous-{task.anonymous_name.rsplit('-', 1)[-1].lower()}"
        proposed = sanitize_name(proposed, task.anonymous_name)
        print(
            f"would name {task.source_file}:{task.anchor.function_line or '?'} "
            f"{task.anonymous_name} as {proposed}"
        )
        return proposed


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    source_root = args.annotated_source_root.resolve()
    if not source_root.exists() or not source_root.is_dir():
        print(f"annotated source root is not a directory: {source_root}", file=sys.stderr)
        return 2

    receipt_path = args.receipt.resolve()
    occurrences_by_file = discover_occurrences(source_root, args.context_lines)
    tasks = build_tasks(occurrences_by_file)
    namer = StubNamer(args.llm_mode)
    proposals = propose_names(tasks, namer)

    entries: list[dict[str, object]] = []
    touched_files: list[Path] = []
    for source_file, occurrences in sorted(occurrences_by_file.items(), key=lambda item: str(item[0])):
        file_entries, changed = apply_file_edits(
            source_root=source_root,
            source_file=source_file,
            occurrences=occurrences,
            proposals=proposals,
            dry_run=args.dry_run,
        )
        entries.extend(file_entries)
        if changed:
            touched_files.append(source_file)

    sweep_paths = list(dict.fromkeys([*touched_files, Path(__file__), README_PATH]))
    em_dash_sweep = run_em_dash_sweep(sweep_paths)

    receipt = {
        "tool": "auto-name-anonymous-concepts",
        "annotated_source_root": str(source_root),
        "llm_mode": args.llm_mode,
        "dry_run": args.dry_run,
        "files_scanned": len(list(iter_source_files(source_root))),
        "anonymous_tags_found": len(entries),
        "files_changed": [str(path.relative_to(source_root)) for path in touched_files],
        "entries": entries,
        "em_dash_sweep": em_dash_sweep,
    }
    if not args.dry_run:
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(
            json.dumps(receipt, indent=2, sort_keys=True, ensure_ascii=True) + "\n",
            encoding="utf-8",
        )

    print(
        f"auto_name_anonymous_concepts: tags={len(entries)} "
        f"files_changed={len(touched_files)} receipt={receipt_path}"
    )
    print(f"em_dash_sweep: clean={em_dash_sweep['clean']} matches={len(em_dash_sweep['matches'])}")
    return 0


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Name anonymous ProveKit concept annotations in annotated source files."
    )
    parser.add_argument("annotated_source_root", type=Path)
    parser.add_argument(
        "--llm-mode",
        choices=("stub", "deterministic"),
        default="stub",
        help="Use a placeholder namer. deterministic is the test-facing spelling.",
    )
    parser.add_argument(
        "--receipt",
        type=Path,
        default=DEFAULT_RECEIPT_PATH,
        help="Receipt path. Defaults to bootstrap/auto-named-concepts/receipt.json.",
    )
    parser.add_argument(
        "--context-lines",
        type=int,
        default=80,
        help="Maximum lines of function body and surrounding context sent to the namer.",
    )
    parser.add_argument("--dry-run", action="store_true", help="Build receipt data without writing edits.")
    return parser.parse_args(argv)


def discover_occurrences(source_root: Path, context_lines: int) -> dict[Path, list[TagOccurrence]]:
    grouped: dict[Path, list[TagOccurrence]] = {}
    for source_file in iter_source_files(source_root):
        try:
            contents = source_file.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        lines = contents.splitlines(keepends=True)
        occurrences: list[TagOccurrence] = []
        for line_index, line in enumerate(lines):
            for anonymous_name, tag_kind in anonymous_tags_in_line(line):
                anchor = find_anchor(lines, line_index, context_lines)
                prompt = build_prompt(source_file, line, anonymous_name, anchor)
                occurrences.append(
                    TagOccurrence(
                        source_file=source_file,
                        line_index=line_index,
                        tag_kind=tag_kind,
                        anonymous_name=anonymous_name,
                        original_line=line.rstrip("\n"),
                        anchor=anchor,
                        prompt=prompt,
                    )
                )
        if occurrences:
            grouped[source_file] = occurrences
    return grouped


def iter_source_files(source_root: Path) -> Iterable[Path]:
    for path in sorted(source_root.rglob("*")):
        if not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.relative_to(source_root).parts):
            continue
        yield path


def anonymous_tags_in_line(line: str) -> list[tuple[str, str]]:
    tags: list[tuple[str, str]] = []
    for match in CONCEPT_COMMENT_RE.finditer(line):
        tags.append((match.group(1), "concept_comment"))
    if SUGAR_CONCEPT_RE.search(line):
        for match in ANONYMOUS_RE.finditer(line):
            tags.append((match.group(0), "sugar_concept_tag"))
    return tags


def find_anchor(lines: list[str], tag_line_index: int, context_lines: int) -> Anchor:
    search_end = min(len(lines), tag_line_index + context_lines + 1)
    for idx in range(tag_line_index + 1, search_end):
        function_name = function_name_from_line(lines[idx])
        if function_name is None:
            continue
        context = context_for_function(lines, tag_line_index, idx, context_lines)
        return Anchor(
            function_name=function_name,
            function_line=idx + 1,
            signature=lines[idx].strip(),
            context=context,
        )
    start = max(0, tag_line_index - 8)
    end = min(len(lines), tag_line_index + context_lines + 1)
    return Anchor(
        function_name=f"unknown-at-line-{tag_line_index + 1}",
        function_line=None,
        signature="",
        context="".join(lines[start:end]).rstrip(),
    )


def function_name_from_line(line: str) -> str | None:
    stripped = line.strip()
    if not stripped or stripped.startswith("//"):
        return None
    if stripped.startswith(("@", "#", "[")):
        return None
    for pattern in FUNCTION_PATTERNS:
        match = pattern.search(stripped)
        if match:
            return match.group(1)
    c_like = re.search(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\([^;{}]*\)\s*(?:\{|$)", stripped)
    if c_like and c_like.group(1) not in C_LIKE_CONTROL_WORDS:
        return c_like.group(1)
    return None


def context_for_function(
    lines: list[str],
    tag_line_index: int,
    function_line_index: int,
    context_lines: int,
) -> str:
    start = max(0, tag_line_index - 8)
    end = function_body_end(lines, function_line_index, context_lines)
    return "".join(lines[start:end]).rstrip()


def function_body_end(lines: list[str], function_line_index: int, context_lines: int) -> int:
    max_end = min(len(lines), function_line_index + context_lines)
    brace_depth = 0
    saw_open = False
    for idx in range(function_line_index, max_end):
        line = strip_line_comment(lines[idx])
        brace_depth += line.count("{")
        if "{" in line:
            saw_open = True
        brace_depth -= line.count("}")
        if saw_open and brace_depth <= 0:
            return idx + 1
    return max_end


def strip_line_comment(line: str) -> str:
    return line.split("//", 1)[0]


def build_prompt(source_file: Path, tag_line: str, anonymous_name: str, anchor: Anchor) -> str:
    return "\n".join(
        [
            "You name anonymous ProveKit concept annotations.",
            "Return exactly one semantic name.",
            "Constraints:",
            "- Name must be kebab-case.",
            "- Names must be prefix-free within this run.",
            "- Name must not start with concept:.",
            "- Do not rename catalog-matched concepts.",
            "- The LLM agent edits source comments only.",
            "- Do not touch substrate, specs, memento types, or pipeline code.",
            f"Source file: {source_file}",
            f"Anonymous tag: {anonymous_name}",
            f"Tag line: {tag_line.strip()}",
            f"Anchor function: {anchor.function_name}",
            f"Signature: {anchor.signature}",
            "Context:",
            anchor.context,
        ]
    )


def build_tasks(occurrences_by_file: dict[Path, list[TagOccurrence]]) -> list[NamingTask]:
    seen: set[tuple[str, str, int | None, str]] = set()
    tasks: list[NamingTask] = []
    for source_file, occurrences in occurrences_by_file.items():
        for occurrence in occurrences:
            key = (
                str(source_file),
                occurrence.anchor.function_name,
                occurrence.anchor.function_line,
                occurrence.anonymous_name,
            )
            if key in seen:
                continue
            seen.add(key)
            tasks.append(
                NamingTask(
                    key=key,
                    anonymous_name=occurrence.anonymous_name,
                    source_file=source_file,
                    anchor=occurrence.anchor,
                    prompt=occurrence.prompt,
                )
            )
    return tasks


def propose_names(tasks: list[NamingTask], namer: StubNamer) -> dict[tuple[str, str, int | None, str], str]:
    raw: list[tuple[NamingTask, str]] = []
    for task in tasks:
        raw.append((task, sanitize_name(namer.propose(task), task.anonymous_name)))

    proposals: dict[tuple[str, str, int | None, str], str] = {}
    assigned: list[str] = []
    for task, proposed in raw:
        final_name = make_prefix_free(proposed, task.anonymous_name, assigned)
        proposals[task.key] = final_name
        assigned.append(final_name)
    return proposals


def sanitize_name(proposed: str, anonymous_name: str) -> str:
    name = to_kebab_case(proposed)
    if name.startswith("concept-"):
        name = name.removeprefix("concept-")
    if not name or name.startswith("unnamed-concept-") or not KEBAB_RE.fullmatch(name):
        suffix = anonymous_name.rsplit("-", 1)[-1].lower()
        name = f"anonymous-{suffix}"
    return name


def make_prefix_free(proposed: str, anonymous_name: str, assigned: list[str]) -> str:
    candidate = proposed
    if not conflicts_with_assigned(candidate, assigned):
        return candidate
    prefix = f"c{anonymous_name.rsplit('-', 1)[-1].lower()}"
    candidate = f"{prefix}-{proposed}"
    counter = 2
    while conflicts_with_assigned(candidate, assigned):
        candidate = f"{prefix}-{counter}-{proposed}"
        counter += 1
    return candidate


def conflicts_with_assigned(candidate: str, assigned: list[str]) -> bool:
    for existing in assigned:
        if candidate == existing or candidate.startswith(f"{existing}-") or existing.startswith(f"{candidate}-"):
            return True
    return False


def to_kebab_case(value: str) -> str:
    value = value.strip().removeprefix("concept:")
    value = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", value)
    value = re.sub(r"[^A-Za-z0-9]+", "-", value)
    value = value.strip("-").lower()
    value = re.sub(r"-+", "-", value)
    return value


def apply_file_edits(
    source_root: Path,
    source_file: Path,
    occurrences: list[TagOccurrence],
    proposals: dict[tuple[str, str, int | None, str], str],
    dry_run: bool,
) -> tuple[list[dict[str, object]], bool]:
    original = source_file.read_text(encoding="utf-8")
    lines = original.splitlines(keepends=True)
    replacements_by_line: dict[int, dict[str, str]] = {}
    proposal_by_occurrence: dict[TagOccurrence, str] = {}

    for occurrence in occurrences:
        key = (
            str(source_file),
            occurrence.anchor.function_name,
            occurrence.anchor.function_line,
            occurrence.anonymous_name,
        )
        proposed = proposals[key]
        proposal_by_occurrence[occurrence] = proposed
        replacements_by_line.setdefault(occurrence.line_index, {})[occurrence.anonymous_name] = proposed

    edited_lines = list(lines)
    for line_index, replacements in replacements_by_line.items():
        edited_line = edited_lines[line_index]
        for anonymous_name, proposed in sorted(replacements.items()):
            edited_line = edited_line.replace(anonymous_name, proposed)
        edited_lines[line_index] = edited_line

    edited = "".join(edited_lines)
    changed = edited != original
    if changed and not dry_run:
        source_file.write_text(edited, encoding="utf-8")

    entries: list[dict[str, object]] = []
    for occurrence in occurrences:
        proposed = proposal_by_occurrence[occurrence]
        new_line = edited_lines[occurrence.line_index].rstrip("\n")
        entries.append(
            {
                "source_file": str(source_file.relative_to(source_root)),
                "line": occurrence.line_index + 1,
                "tag_kind": occurrence.tag_kind,
                "anonymous_name": occurrence.anonymous_name,
                "anchor_function": occurrence.anchor.function_name,
                "anchor_line": occurrence.anchor.function_line,
                "original_tag": occurrence.original_line,
                "new_tag": new_line,
                "proposed_name": proposed,
                "edit_succeeded": occurrence.anonymous_name not in new_line and proposed in new_line,
            }
        )
    return entries, changed


def run_em_dash_sweep(paths: list[Path]) -> dict[str, object]:
    matches: list[dict[str, object]] = []
    for path in paths:
        if not path.exists() or not path.is_file():
            continue
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(lines, start=1):
            if "\u2014" in line:
                matches.append({"path": str(path), "line": line_number})
    return {"clean": not matches, "matches": matches}


if __name__ == "__main__":
    raise SystemExit(main())
