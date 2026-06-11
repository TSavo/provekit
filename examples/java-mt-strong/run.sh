#!/usr/bin/env bash
# java-mt-strong showcase: STRONG rung — Mersenne Twister reference vectors DERIVED.
#
# THESIS: the FLOOR rung (java-mt-reference) lifts the vendor's per-draw
# assertEquals(refValue, nextInt()) as point contracts and catches a WITHIN-TEST
# contradiction. The STRONG rung WALKS the vendor's entire seed→state→twist→temper
# pipeline INTER-PROCEDURALLY for the literal Nishimura seed and pins each draw to
# the WALKED recurrence:  mt32.eq-seeded(refValue, <walked seed→state→draw FOL>).
#
#   GOOD: the 8 vendor-sworn reference values are each DERIVED — z3 reports
#         `unsat` on `(not (= refValue <walked>))` → DISCHARGED by computation.
#   BAD:  a SINGLE wrong-but-plausible value (0x3fa23624, NO second contradictory
#         claim) → z3 `sat` → UNSATISFIED by the walked recurrence. The refutation
#         is the real Mersenne Twister algorithm over the real seed, NOT a
#         within-test contradiction. This is the leap over the FLOOR rung.
#
# Real lift → real ir-compiler → real z3. Verdicts read from check-sat, not exit codes.
#
# LOGO: "Commons RNG's own Mersenne Twister reference vectors, DERIVED — the whole
#        seed→state→twist→temper pipeline walked and checked, no extraction."
set -euo pipefail

command -v javac   >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java    >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }
command -v z3      >/dev/null 2>&1 || { echo "SKIP: no z3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
KIT_DIR="$REPO/implementations/java/sugar-lift-java-tests"
RUST="$REPO/implementations/rust"
KIT_OUT="$(mktemp -d)"
SMT_BIN="$RUST/target/debug/sugar-ir-smt-lib"

[ -x "$SMT_BIN" ] || { echo "SKIP: ir-compiler binary not built ($SMT_BIN)"; exit 0; }

echo "== build kit =="
bash "$KIT_DIR/build.sh" "$KIT_OUT" >/dev/null 2>&1

JAVA_CMD="java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -cp $KIT_OUT JavaTestAssertionsRpc"

lift() {  # <workspace> <src>
  python3 - "$1" "$2" <<'PY' | eval "$JAVA_CMD" 2>/dev/null
import sys, json
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift",
    "params":{"workspace_root":sys.argv[1],"source_paths":[sys.argv[2]]}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
}

GOOD_LIFT="$(mktemp)"; BAD_LIFT="$(mktemp)"
lift "$HERE/good" "src/test/java/demo/MersenneTwisterStrongTest.java"   > "$GOOD_LIFT"
lift "$HERE/bad"  "src/test/java/demo/MersenneTwisterWrongValueTest.java" > "$BAD_LIFT"

echo
echo "== STRONG-tier discharge (real lift → real ir-compiler → real z3) =="
python3 - "$GOOD_LIFT" "$BAD_LIFT" "$SMT_BIN" <<'PY'
import sys, json, subprocess

def load(path):
    for ln in open(path):
        ln = ln.strip()
        if ln and json.loads(ln).get("id") == 2:
            return json.loads(ln)["result"]
    raise SystemExit(f"no lift result in {path}")

def checksat(inv, smt_bin):
    reqs = [
        {"jsonrpc":"2.0","id":1,"method":"sugar.ir.handshake","params":{}},
        {"jsonrpc":"2.0","id":2,"method":"sugar.ir.compile",
         "params":{"ir_json":inv,"target_dialect":"smt-lib-v2.6"}},
    ]
    p = subprocess.run([smt_bin], input="\n".join(json.dumps(r) for r in reqs),
                       capture_output=True, text=True)
    smt = None
    for ln in p.stdout.splitlines():
        if not ln.strip(): continue
        o = json.loads(ln)
        if o.get("id") == 2 and "result" in o:
            r = o["result"]; smt = r.get("preamble","") + r.get("body","")
    assert smt, f"no SMT: {p.stdout[:200]} {p.stderr[:200]}"
    z = subprocess.run(["z3","-smt2","-in"], input=smt + "\n(check-sat)\n",
                       capture_output=True, text=True)
    return z.stdout.strip().splitlines()[0]

good_path, bad_path, smt_bin = sys.argv[1], sys.argv[2], sys.argv[3]
good = load(good_path); bad = load(bad_path)

gpins = sorted((c for c in good["ir"] if c["name"].endswith("::mt-seed-value-pin")),
               key=lambda c: c["name"])
assert len(gpins) == 8, f"GOOD: expected 8 derived draw pins, got {len(gpins)}"
print(f"GOOD — {len(gpins)} reference draws, each DERIVED from the walked recurrence:")
for c in gpins:
    draw = c["name"].split("testDraw")[1].split(":")[0]
    v = checksat(c["inv"], smt_bin)
    assert v == "unsat", f"  draw[{draw}] must DISCHARGE (unsat), got {v}"
    print(f"  draw[{draw}]  →  {v}   (DISCHARGED by derivation)")

bpins = [c for c in bad["ir"] if c["name"].endswith("::mt-seed-value-pin")]
assert len(bpins) == 1, f"BAD: expected 1 wrong-value pin, got {len(bpins)}"
v = checksat(bpins[0]["inv"], smt_bin)
assert v == "sat", f"BAD wrong value must be REFUTED (sat), got {v}"
print(f"BAD  — a single wrong-by-one-bit value (0x3fa23624), NO contradiction:")
print(f"  draw[0]  →  {v}   (UNSATISFIED by the walked seed→state→twist→temper recurrence)")

print()
print("PASS: java-mt-strong — the Mersenne Twister reference-vector oath DERIVED.")
print("      GOOD: 8 vendor-sworn draws discharged by the walked recurrence (unsat).")
print("      BAD:  one wrong value refuted by DERIVATION, not contradiction (sat).")
print("      Real callsite (mt.nextInt()), real vendor value, real recurrence — the oath.")
PY

rm -f "$GOOD_LIFT" "$BAD_LIFT"; rm -rf "$KIT_OUT"
