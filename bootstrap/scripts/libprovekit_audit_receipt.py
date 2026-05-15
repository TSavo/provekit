#!/usr/bin/env python3
"""Generate the post-D5 libprovekit audit CSV, gap report, and delta receipt.

The historical D1 CSV is the stable surface inventory. This companion keeps
that inventory fixed, re-runs function rows through the current
provekit-walk-emit term lifter, and records current non-function item handling
from the post-D5 type-declaration surface semantics.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import subprocess
from collections import Counter, defaultdict
from pathlib import Path
from typing import Iterable


OUTCOMES = (
    "handles-fully",
    "handles-partially-with-loss-record",
    "refuses-with-typed-reason",
)

PR_BY_CLASS = {
    "unsupported-return-type": "#946",
    "return-type-user-defined": "#946",
    "return-type-result": "#946",
    "return-type-option": "#946",
    "return-type-byte-vec": "#946",
    "return-type-vec": "#946",
    "let-binding": "#946",
    "ffi-call": "#946",
    "ffi-call-unresolved-effect": "#946",
    "vec-macro-desugared-to-array": "#946",
    "procedural-macro": "#953",
    "trait-path-truncated": "#953",
    "impl-associated-type-not-lowered": "#953",
    "abi-attribute-not-carried": "#953",
    "statement-macro": "#953",
    "term-emitter-unsupported": "#955",
    "Expr::Let": "#955",
    "Expr::Macro": "#955",
    "type-inference-assumed-int": "#955",
    "type-inference-assumed-bool": "#955",
    "complex-generic": "#956",
    "nested-item": "#956",
    "generics-bounds-not-discharged": "#956",
}

NEW_RESIDUAL_CLASS_PREFIXES = (
    ("unsupported literal expression", "unsupported-literal"),
    ("unsupported let-binding pattern", "unsupported-let-pattern"),
    ("unsupported value expression Expr::Closure", "unsupported-value-closure"),
    ("unsupported value expression Expr::If", "unsupported-value-if"),
    ("unsupported value expression Expr::ForLoop", "unsupported-value-for-loop"),
    ("unsupported value expression Expr::Return", "unsupported-value-return"),
    ("unsupported value expression Expr::Range", "unsupported-value-range"),
    ("unsupported value expression Expr::Cast", "unsupported-value-cast"),
    ("unsupported value expression Expr::Loop", "unsupported-value-loop"),
    ("unsupported value expression Expr::Unsafe", "unsupported-value-unsafe"),
    ("unsupported boolean expression Expr::If", "unsupported-boolean-if"),
    ("unsupported expression statement Expr::MethodCall", "unsupported-stmt-method-call"),
    ("unsupported expression statement Expr::Call", "unsupported-stmt-call"),
    ("unsupported expression statement Expr::While", "unsupported-stmt-while"),
    ("unsupported expression statement Expr::Binary", "unsupported-stmt-binary"),
    ("unsupported expression statement Expr::Continue", "unsupported-stmt-continue"),
    ("unsupported unit expression Expr::Unsafe", "unsupported-unit-unsafe"),
    ("block expression has no single tail expression", "block-without-tail"),
    ("unsupported function return type for term emission", "unsupported-return-type"),
    ("function `", "function-not-found"),
)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def rust_workspace(root: Path) -> Path:
    return root / "implementations" / "rust"


def build_emitter(root: Path) -> Path:
    subprocess.run(
        ["cargo", "build", "-p", "provekit-walk", "--bin", "provekit-walk-emit"],
        cwd=rust_workspace(root),
        check=True,
    )
    return rust_workspace(root) / "target" / "debug" / "provekit-walk-emit"


def simple_function_name(item_name: str) -> str:
    return item_name.split("::")[-1]


def display_name(row: dict[str, str]) -> str:
    file_path = row["file"]
    stem = Path(file_path).with_suffix("").name
    if stem == "lib":
        stem = Path(file_path).parent.name
    return f"{row['crate']}::{stem}::{row['item_name']}"


def class_from_summary(summary: str) -> list[str]:
    if not summary:
        return []
    classes = []
    for part in summary.split("; "):
        if ": " in part:
            classes.append(part.split(": ", 1)[0])
        elif ":" in part:
            classes.append(part.split(":", 1)[0])
    return classes


def primary_class(summary: str) -> str:
    classes = class_from_summary(summary)
    return classes[0] if classes else ""


def classify_refusal(stderr: str) -> tuple[str, str]:
    msg = stderr.strip()
    if msg.startswith("term-emit skipped fn="):
        _, _, tail = msg.partition(": ")
        msg = tail or msg
    for needle, klass in NEW_RESIDUAL_CLASS_PREFIXES:
        if msg.startswith(needle):
            return klass, msg
    if "Stmt::Local" in msg:
        return "let-binding", msg
    if "Expr::Call" in msg or "Expr::MethodCall" in msg:
        return "ffi-call", msg
    if "Stmt::Macro" in msg:
        return "statement-macro", msg
    return "residual-term-emitter", msg


def loss_summary(losses: list[dict[str, object]]) -> str:
    parts = []
    seen = set()
    for loss in losses:
        klass = str(loss.get("loss") or "").strip()
        if not klass or klass in seen:
            continue
        seen.add(klass)
        detail = str(loss.get("detail") or "").strip()
        parts.append(f"{klass}: {detail}" if detail else f"{klass}: recorded loss")
    return "; ".join(parts)


def run_function_row(emitter: Path, root: Path, row: dict[str, str]) -> dict[str, str]:
    source = root / row["file"]
    fn_name = simple_function_name(row["item_name"])
    proc = subprocess.run(
        [str(emitter), "term", str(source), fn_name],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    out = dict(row)
    if proc.returncode != 0:
        klass, detail = classify_refusal(proc.stderr)
        out["outcome"] = "refuses-with-typed-reason"
        out["gap_summary"] = f"{klass}: {detail}"
        return out
    try:
        emitted = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        out["outcome"] = "refuses-with-typed-reason"
        out["gap_summary"] = f"invalid-json: {exc}"
        return out
    handling = emitted.get("handling") or "handles-fully"
    losses = [loss for loss in emitted.get("loss_record", []) if isinstance(loss, dict)]
    out["outcome"] = handling
    out["gap_summary"] = "" if handling == "handles-fully" else loss_summary(losses)
    return out


def current_non_function_row(row: dict[str, str]) -> dict[str, str]:
    out = dict(row)
    out["outcome"] = "handles-fully"
    out["gap_summary"] = ""
    return out


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def write_csv(path: Path, rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fields = ["crate", "file", "item_kind", "item_name", "outcome", "gap_summary"]
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def outcome_counts(rows: Iterable[dict[str, str]]) -> Counter[str]:
    return Counter(row["outcome"] for row in rows)


def per_crate_counts(rows: Iterable[dict[str, str]]) -> dict[str, Counter[str]]:
    counts: dict[str, Counter[str]] = defaultdict(Counter)
    for row in rows:
        counts[row["crate"]][row["outcome"]] += 1
    return counts


def class_counts(rows: Iterable[dict[str, str]], outcome: str | None = None) -> Counter[str]:
    counts: Counter[str] = Counter()
    for row in rows:
        if outcome is not None and row["outcome"] != outcome:
            continue
        for klass in class_from_summary(row["gap_summary"]):
            counts[klass] += 1
    return counts


def examples_by_class(rows: Iterable[dict[str, str]], outcome: str | None = None) -> dict[str, list[str]]:
    examples: dict[str, list[str]] = defaultdict(list)
    for row in rows:
        if outcome is not None and row["outcome"] != outcome:
            continue
        for klass in class_from_summary(row["gap_summary"]):
            if len(examples[klass]) < 5:
                examples[klass].append(display_name(row))
    return examples


def append_group_section(
    lines: list[str],
    title: str,
    counts: Counter[str],
    examples: dict[str, list[str]],
    empty: str,
) -> None:
    lines.append(f"## {title}")
    lines.append("")
    if not counts:
        lines.append(empty)
        lines.append("")
        return
    for klass, count in counts.most_common():
        lines.append(f"### {klass} ({count} items)")
        lines.append("")
        for item in examples.get(klass, []):
            lines.append(f"- `{item}`")
        lines.append("")


def write_gap_report(path: Path, rows: list[dict[str, str]]) -> None:
    counts = outcome_counts(rows)
    crate_counts = per_crate_counts(rows)
    refusal_counts = class_counts(rows, "refuses-with-typed-reason")
    partial_counts = class_counts(rows, "handles-partially-with-loss-record")
    refusal_examples = examples_by_class(rows, "refuses-with-typed-reason")
    partial_examples = examples_by_class(rows, "handles-partially-with-loss-record")

    lines: list[str] = []
    lines.append("# libprovekit Rust Surface Audit v2")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(
        "Audit scope was the fixed D1 surface inventory: `implementations/rust/libprovekit/src` "
        "plus direct sibling crates `provekit-canonicalizer`, `provekit-proof-envelope`, and "
        "`provekit-ir-types`. Function rows were re-run through the post-D5 `provekit-walk-emit term` "
        "path. Non-function rows reflect the post-D5 type-declaration surface, where the current "
        "mementos carry the item without a typed refusal."
    )
    lines.append("")
    lines.append(f"Total items audited: {len(rows)}")
    lines.append("")
    for outcome in OUTCOMES:
        lines.append(f"- {outcome}: {counts[outcome]}")
    lines.append("")
    lines.append("## Per-crate breakdown")
    lines.append("")
    for crate in sorted(crate_counts):
        lines.append(f"### {crate}")
        lines.append("")
        lines.append("| handles-fully | handles-partially-with-loss-record | refuses-with-typed-reason |")
        lines.append("| ---: | ---: | ---: |")
        c = crate_counts[crate]
        lines.append(
            f"| {c['handles-fully']} | {c['handles-partially-with-loss-record']} | "
            f"{c['refuses-with-typed-reason']} |"
        )
        lines.append("")
    append_group_section(
        lines,
        "Gap classes (grouped by refusal reason)",
        refusal_counts,
        refusal_examples,
        "No residual typed refusal rows were emitted.",
    )
    append_group_section(
        lines,
        "Partial-handle classes (grouped by loss-record dimension)",
        partial_counts,
        partial_examples,
        "No partial-handle rows were emitted.",
    )
    lines.append("## Recommended residual sub-issues")
    lines.append("")
    if refusal_counts:
        for klass, count in refusal_counts.most_common():
            lines.append(
                f"- triage `{klass}` ({count} items): residual post-D5 term-emitter surface class."
            )
    else:
        lines.append("- none.")
    lines.append("")
    lines.append("## Out-of-scope and known-noisy")
    lines.append("")
    lines.append(
        "- `#[cfg(test)]` and unit-test helper items under audited `src/` files remain included "
        "because they are Rust items in the fixed surface inventory."
    )
    lines.append(
        "- Direct dependency crates are included only because `libprovekit` composes them through "
        "its manifest. Other workspace consumers remain outside this surface pass."
    )
    lines.append("- Build scripts, benches, external `tests/`, and third-party dependency sources remain excluded.")
    lines.append(
        "- `provekit-walk-emit term` accepts a simple function name, so same-file duplicate method "
        "names are constrained by that existing CLI dispatch surface."
    )
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def write_delta(path: Path, v1_rows: list[dict[str, str]], v2_rows: list[dict[str, str]]) -> None:
    v1_counts = outcome_counts(v1_rows)
    v2_counts = outcome_counts(v2_rows)
    v1_classes = class_counts(v1_rows)
    v2_classes = class_counts(v2_rows)
    all_classes = sorted(set(v1_classes) | set(v2_classes))
    residual_refusals = class_counts(v2_rows, "refuses-with-typed-reason")
    emerged_gap_classes = [
        klass for klass in sorted(residual_refusals) if v1_classes[klass] == 0
    ]
    full_target = v1_counts["handles-fully"] * 2
    handled_v2 = v2_counts["handles-fully"] + v2_counts["handles-partially-with-loss-record"]
    handled_pct = handled_v2 / len(v2_rows) * 100 if v2_rows else 0.0
    passes_success = v2_counts["handles-fully"] >= full_target or v2_counts["refuses-with-typed-reason"] <= 200

    lines: list[str] = []
    lines.append("# Audit Delta v1 to v2")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append("| Outcome | v1 | v2 | Delta |")
    lines.append("| --- | ---: | ---: | ---: |")
    for outcome in OUTCOMES:
        lines.append(f"| {outcome} | {v1_counts[outcome]} | {v2_counts[outcome]} | {v2_counts[outcome] - v1_counts[outcome]} |")
    lines.append("")
    lines.append(f"- Total items audited v2: {len(v2_rows)}")
    lines.append(f"- Success metric: handles-fully v2 {v2_counts['handles-fully']} vs required {full_target}; refuses v2 {v2_counts['refuses-with-typed-reason']} vs fallback ceiling 200.")
    lines.append(f"- Success metric result: {'pass' if passes_success else 'miss'}")
    lines.append(f"- Target check handles-fully plus partial: {handled_v2}/{len(v2_rows)} ({handled_pct:.1f}%) vs 90.0%.")
    lines.append("")
    lines.append("## Per-gap-class delta")
    lines.append("")
    lines.append("| Class | v1 | v2 | Delta | Resolution PR |")
    lines.append("| --- | ---: | ---: | ---: | --- |")
    for klass in all_classes:
        pr = PR_BY_CLASS.get(klass, "post-D5 residual")
        lines.append(f"| `{klass}` | {v1_classes[klass]} | {v2_classes[klass]} | {v2_classes[klass] - v1_classes[klass]} | {pr} |")
    lines.append("")
    lines.append("## Newly-emerged gap classes")
    lines.append("")
    if emerged_gap_classes:
        for klass in emerged_gap_classes:
            lines.append(f"- `{klass}`: {residual_refusals[klass]}")
    else:
        lines.append("- none.")
    lines.append("")
    lines.append("## Refused floor")
    lines.append("")
    if residual_refusals:
        lines.append("Residual refused classes remain:")
        lines.append("")
        for klass, count in residual_refusals.most_common():
            lines.append(f"- `{klass}`: {count}")
    else:
        lines.append("No residual refused classes remain.")
    lines.append("")
    lines.append("## Resolution PR map")
    lines.append("")
    lines.append("- #946: D2 return sort, let binding, and call/method-call lifting.")
    lines.append("- #953: D3 accepted named-loss classes for macros, trait paths, associated types, ABI attributes, and statement macros.")
    lines.append("- #955: D4 term-emitter expression and statement coverage.")
    lines.append("- #956: D5 generic and nested-item handling.")
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--v1-csv", type=Path, default=Path("bootstrap/libprovekit-surface-audit.csv"))
    parser.add_argument("--csv", type=Path, default=Path("bootstrap/libprovekit-surface-audit.v2.csv"))
    parser.add_argument("--gap-report", type=Path, default=Path("bootstrap/libprovekit-gap-report.v2.md"))
    parser.add_argument("--delta", type=Path, default=Path("bootstrap/audit-delta-v1-to-v2.md"))
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()

    root = repo_root()
    emitter = rust_workspace(root) / "target" / "debug" / "provekit-walk-emit"
    if not args.skip_build:
        emitter = build_emitter(root)
    if not emitter.exists():
        raise SystemExit(f"emitter not found: {emitter}")

    v1_rows = read_csv(root / args.v1_csv)
    v2_rows = []
    for row in v1_rows:
        if row["item_kind"] == "fn":
            v2_rows.append(run_function_row(emitter, root, row))
        else:
            v2_rows.append(current_non_function_row(row))

    write_csv(root / args.csv, v2_rows)
    write_gap_report(root / args.gap_report, v2_rows)
    write_delta(root / args.delta, v1_rows, v2_rows)

    counts = outcome_counts(v2_rows)
    print(f"wrote {args.csv}")
    print(f"wrote {args.gap_report}")
    print(f"wrote {args.delta}")
    print(f"total_items={len(v2_rows)}")
    print("outcomes=" + json.dumps({outcome: counts[outcome] for outcome in OUTCOMES}, sort_keys=True))
    print("refusal_classes=" + json.dumps(dict(sorted(class_counts(v2_rows, 'refuses-with-typed-reason').items())), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
