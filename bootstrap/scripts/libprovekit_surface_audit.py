#!/usr/bin/env python3
"""Audit libprovekit Rust functions through provekit-walk term emission.

The original D1 audit artifacts are not present in this worktree. This driver
walks the libprovekit Rust surface, invokes the real provekit-walk term emitter
for each discovered function name, and tallies the refusal classes used by the
D2 issue.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path


FN_RE = re.compile(
    r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]+\"\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<|\()"
)

ACCEPTED_LOSS_CLASSES = (
    "procedural-macro",
    "trait-path-truncated",
    "impl-associated-type-not-lowered",
    "abi-attribute-not-carried",
    "statement-macro",
)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def rust_workspace(root: Path) -> Path:
    return root / "implementations" / "rust"


def default_surface(root: Path) -> list[Path]:
    base = root / "implementations" / "rust" / "libprovekit"
    paths = list((base / "src").rglob("*.rs")) + list((base / "tests").rglob("*.rs"))
    return sorted(paths)


def discover_functions(paths: list[Path]) -> list[tuple[Path, str]]:
    out: list[tuple[Path, str]] = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for match in FN_RE.finditer(text):
            out.append((path, match.group(1)))
    return out


def build_emitter(root: Path) -> Path:
    subprocess.run(
        ["cargo", "build", "-p", "provekit-walk", "--bin", "provekit-walk-emit"],
        cwd=rust_workspace(root),
        check=True,
    )
    return rust_workspace(root) / "target" / "debug" / "provekit-walk-emit"


def classify_failure(stderr: str) -> str:
    if "Stmt::Local" in stderr or "let-binding" in stderr:
        return "let-binding"
    if "Stmt::Macro" in stderr or "statement-macro" in stderr:
        return "statement-macro"
    if "unsupported function return type" in stderr:
        return "unsupported-return-type"
    if "procedural-macro" in stderr:
        return "procedural-macro"
    if "trait-path-truncated" in stderr:
        return "trait-path-truncated"
    if "impl-associated-type-not-lowered" in stderr:
        return "impl-associated-type-not-lowered"
    if "abi-attribute-not-carried" in stderr:
        return "abi-attribute-not-carried"
    if "Expr::Call" in stderr or "Expr::MethodCall" in stderr:
        return "ffi-call"
    return "other"


def run_one(emitter: Path, path: Path, function: str) -> dict:
    proc = subprocess.run(
        [str(emitter), "term", str(path), function],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    row = {
        "path": str(path),
        "function": function,
        "status": "ok" if proc.returncode == 0 else "refused",
        "refusal_class": None,
        "handling": None,
        "losses": [],
        "stderr": proc.stderr.strip(),
    }
    if proc.returncode != 0:
        row["refusal_class"] = classify_failure(proc.stderr)
        return row
    try:
        emitted = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        row["status"] = "refused"
        row["refusal_class"] = "invalid-json"
        row["stderr"] = str(exc)
        return row
    row["handling"] = emitted.get("handling")
    row["losses"] = [
        loss.get("loss")
        for loss in emitted.get("loss_record", [])
        if isinstance(loss, dict) and loss.get("loss")
    ]
    return row


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()

    root = repo_root()
    emitter = rust_workspace(root) / "target" / "debug" / "provekit-walk-emit"
    if not args.skip_build:
        emitter = build_emitter(root)
    if not emitter.exists():
        parser.error(f"emitter not found: {emitter}")

    functions = discover_functions(default_surface(root))
    rows = [run_one(emitter, path, function) for path, function in functions]
    refusals = Counter(row["refusal_class"] for row in rows if row["status"] == "refused")
    handling = Counter(row["handling"] for row in rows if row["status"] == "ok")
    losses = Counter(loss for row in rows for loss in row["losses"])
    target_total = sum(refusals[name] for name in ("let-binding", "unsupported-return-type", "ffi-call"))

    report = {
        "surface_items": len(rows),
        "status": Counter(row["status"] for row in rows),
        "handling": handling,
        "refusals": refusals,
        "losses": losses,
        "target_refusal_total": target_total,
        "rows": rows,
    }
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    print(f"surface_items={len(rows)}")
    print(f"ok={report['status']['ok']} refused={report['status']['refused']}")
    print(
        "target_refusals "
        f"let-binding={refusals['let-binding']} "
        f"unsupported-return-type={refusals['unsupported-return-type']} "
        f"ffi-call={refusals['ffi-call']} "
        f"combined={target_total}"
    )
    print(
        "handling "
        f"handles-fully={handling['handles-fully']} "
        f"handles-partially-with-loss-record={handling['handles-partially-with-loss-record']}"
    )
    print(
        "accepted_loss_refusals "
        + " ".join(f"{name}={refusals[name]}" for name in ACCEPTED_LOSS_CLASSES)
    )
    print(
        "accepted_loss_dimensions "
        + " ".join(f"{name}={losses[name]}" for name in ACCEPTED_LOSS_CLASSES)
    )
    if refusals:
        print("refusals_by_class=" + json.dumps(dict(sorted(refusals.items())), sort_keys=True))
    if losses:
        print("losses=" + json.dumps(dict(sorted(losses.items())), sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
