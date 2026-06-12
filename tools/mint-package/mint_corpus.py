#!/usr/bin/env python3
"""Conjoin, then resolve, then mint -- the corpus orchestrator.

    mint_corpus.py requirements.in [--registry DIR]

Vendors rely on vendors: a bundle minted in an isolated world cites
dependency bundles at versions that world chose alone, and the citation
edges dangle. So the corpus is resolved the way the substrate proves:
the requirement sets are CONSTRAINTS, their union is the CONJUNCTION,
pip's resolver is the solver, and the resolved set is the MODEL -- one
world, one version per package, every cross-vendor edge coherent by
construction. The world's sha256 (sorted name==version lines) is its
identity and is stamped into every bundle's meta.

An unresolvable conjunction is pip telling you the corpus does not fit
one world -- that is information (partition into strata), never papered
over. Each resolved member then mints ONCE (resolve-once law) via
mint_package.py against the shared venv.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys

from tqdm import tqdm  # required: a long batch with no live report is a silence

HERE = os.path.dirname(os.path.abspath(__file__))


def _topo_order(members, venv):
    """Post-order DFS over the dependency DAG: vendor-free leaves first, app
    root last, so every package mints AFTER the deps whose bundles it cites.
    `members` is {name: version}; edges are read from the resolved venv's
    installed metadata (only deps that are themselves members count -- a dep
    outside the world is not in scope). Cycles are broken by the visiting-set
    guard (a back-edge is skipped; both nodes still appear in the order)."""
    lower = {n.lower(): n for n in members}

    def deps_of(name):
        code = (
            "import sys\n"
            "from importlib.metadata import requires, PackageNotFoundError\n"
            "try: reqs = requires(sys.argv[1]) or []\n"
            "except PackageNotFoundError: reqs = []\n"
            "out=[]\n"
            "for r in reqs:\n"
            "    n=r.replace('(',' ').split(';')[0].strip()\n"
            "    for s in ('==','>=','<=','~=','!=','>','<',' ','['):\n"
            "        if s in n: n=n.split(s)[0]\n"
            "    n=n.strip()\n"
            "    if n: out.append(n)\n"
            "print('\\n'.join(sorted(set(out))))\n"
        )
        r = subprocess.run(
            [os.path.join(venv, "bin", "python"), "-c", code, name],
            capture_output=True,
            text=True,
        )
        return [
            lower[d.lower()]
            for d in r.stdout.splitlines()
            if d.strip() and d.lower() in lower
        ]

    order, done, visiting = [], set(), set()

    def visit(n):
        if n in done or n in visiting:
            return  # already placed, or a cycle back-edge: skip
        visiting.add(n)
        for d in deps_of(n):
            visit(d)
        visiting.discard(n)
        done.add(n)
        order.append(n)

    for n in sorted(members):
        visit(n)
    return [f"{n}=={members[n]}" for n in order]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("requirements", help="requirements.in: the conjunction")
    ap.add_argument(
        "--registry", default=os.path.expanduser("~/sugar-registry")
    )
    args = ap.parse_args()

    os.makedirs(args.registry, exist_ok=True)
    venv = os.path.join(args.registry, ".world", "venv")
    if not os.path.exists(os.path.join(venv, "bin", "python")):
        os.makedirs(os.path.dirname(venv), exist_ok=True)
        subprocess.run([sys.executable, "-m", "venv", venv], check=True)

    # ── conjoin + resolve: pip is the solver, the lockfile is the model ──
    r = subprocess.run(
        [os.path.join(venv, "bin", "pip"), "install", "-q", "-r", args.requirements],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        sys.exit(
            "UNSAT: the conjoined requirements do not resolve to one world. "
            "This is information, not failure -- partition the corpus into "
            "strata and resolve each. pip's refutation:\n" + r.stderr[-2000:]
        )

    listing = json.loads(
        subprocess.run(
            [os.path.join(venv, "bin", "pip"), "list", "--format", "json"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    members = {
        p["name"]: p["version"]
        for p in listing
        if p["name"] not in ("pip", "setuptools", "wheel")
    }
    # TOPOLOGICAL (leaves-up) ORDER: a package can only cite bundles that
    # already exist, so vendor-free leaves mint first and the app root last
    # (post-order DFS over the dependency DAG). Cycles -- rare but real in
    # PyPI -- are broken deterministically (a back-edge is just dropped from
    # the order; both members still mint, the later one simply can't cite the
    # earlier, recorded as an uncited dep, never silently). The WORLD SHA is
    # over the SORTED pins (order-independent identity); the MINT order is the
    # topological one.
    pins = sorted(f"{n}=={v}" for n, v in members.items())
    order = _topo_order(members, venv)
    world_sha = hashlib.sha256("\n".join(pins).encode()).hexdigest()
    world_path = os.path.join(args.registry, ".world", f"{world_sha}.lock")
    open(world_path, "w").write("\n".join(pins) + "\n")
    print(f"WORLD: {len(pins)} packages, sha256={world_sha[:16]}... -> {world_path}")

    # ── mint each member of the model, once, in the shared world ─────────
    # Every member produces a report line (jsonl) AND a tqdm.write summary;
    # the bar's postfix carries the running tally. A long batch with no live
    # report is exactly the silence the doctrine forbids.
    report_path = os.path.join(args.registry, ".world", f"{world_sha}.report.jsonl")
    rf = open(report_path, "w")
    results = {}
    counts = {"minted": 0, "cached": 0, "no-tests": 0, "fail": 0}
    bar = tqdm(order, desc="minting world (leaves-up)", unit="pkg", dynamic_ncols=True)
    for spec in bar:
        r = subprocess.run(
            [
                sys.executable,
                os.path.join(HERE, "mint_package.py"),
                spec,
                "--registry", args.registry,
                "--venv", venv,
                "--world", world_sha,
            ],
            capture_output=True,
            text=True,
        )
        out = (r.stdout or "") + (r.stderr or "")
        name, version = spec.split("==", 1)
        rec = {"package": name, "version": version, "result": None}
        # read the worker's own meta.json -- the receipt is the ground truth,
        # the subprocess text is only the index to it.
        meta_path = os.path.join(args.registry, name, version, "meta.json")
        if os.path.exists(meta_path):
            try:
                meta = json.load(open(meta_path))
                rec["result"] = meta.get("result")
                rec["sdist_sha256"] = meta.get("sdist_sha256", "")[:16]
                rec["test_files"] = meta.get("test_files")
                rec["receipt"] = meta.get("receipt_summary")
                rec["assertion_properties"] = meta.get("assertion_properties")
            except Exception as e:
                rec["result"] = f"meta-unreadable:{e}"
        if r.returncode != 0:
            bucket = "fail"
            rec["result"] = rec.get("result") or f"exit-{r.returncode}"
            rec["stderr_tail"] = (r.stderr or "")[-400:]
        elif rec["result"] == "no-test-corpus":
            bucket = "no-tests"
        elif "already minted" in out:
            bucket = "cached"
        else:
            bucket = "minted"
        results[spec] = rec["result"]
        counts[bucket] += 1
        rf.write(json.dumps(rec) + "\n")
        rf.flush()
        bar.set_postfix(**counts)
        # the per-package report line
        rsum = rec.get("receipt") or {}
        disc = rsum.get("discharged")
        ref = rsum.get("refused")
        detail = (
            f"discharged={disc} refused={ref} props={rec.get('assertion_properties')}"
            if rsum
            else rec["result"]
        )
        tqdm.write(f"[{bucket:8}] {spec:28} {detail}")

    rf.close()
    summary_path = os.path.join(args.registry, ".world", f"{world_sha}.mints.json")
    json.dump(results, open(summary_path, "w"), indent=2)
    print(
        f"\nCORPUS COMPLETE: world={world_sha[:16]} "
        f"minted={counts['minted']} cached={counts['cached']} "
        f"no-tests={counts['no-tests']} fail={counts['fail']}"
    )
    print(f"  report:  {report_path}")
    print(f"  summary: {summary_path}")


if __name__ == "__main__":
    main()
