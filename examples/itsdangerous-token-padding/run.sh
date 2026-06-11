#!/usr/bin/env bash
# itsdangerous-token-padding: the real-name logo for the python universe rung.
#
# itsdangerous (Flask's signing dependency) encodes tokens with
#
#     def base64_encode(string):
#         string = want_bytes(string)
#         return base64.urlsafe_b64encode(string).rstrip(b"=")
#
# rstrip is TOTAL: no output of base64_encode ever ends with '=' -- for any
# input, forever, by one byte literal in the vendor's own source. The lifter
# walks that shape (the no-suffix-chars family), reports its ∀⊨sample
# evidence honestly (the wheel ships no test corpus: 0 vendor vectors,
# stated on the universe record), and conjoins ¬suffix-of("=", subject)
# into the callsite's #euf# assertion.
#
# BAD twin: the token-padding confusion -- asserting the PADDED standard
# base64url value where itsdangerous' stripped tokens live (the classic
# JWT/token interop bug). equality ∧ ¬suffixof -> UNSAT, statically.
#
# Verdicts parsed from real .verify.json rows; the verdict FLIP is the
# vacuity witness (a universe that never met the equality would let both
# twins discharge).
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/implementations/rust/target}"
BIN="$TARGET_DIR/debug/sugar"

VENV="${ITSDANGEROUS_LOGO_VENV:-/tmp/itsdangerous-logo-venv}"
export ITSDANGEROUS_LOGO_VENV="$VENV"
if [ ! -x "$VENV/bin/python" ]; then
  echo "== create venv + install the real vendor (itsdangerous) =="
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q itsdangerous
fi
"$VENV/bin/python" -c "import itsdangerous; print('vendor:', 'itsdangerous', itsdangerous.__version__ if hasattr(itsdangerous,'__version__') else '(installed)')" || {
  echo "FAIL: vendor install"; exit 1; }

echo "== build the CLI =="
cargo build --manifest-path "$REPO/implementations/rust/Cargo.toml" -p sugar-cli --bin sugar >/dev/null || {
  echo "FAIL: sugar build"; exit 1; }
[ -x "$BIN" ] || { echo "FAIL: sugar binary missing at $BIN"; exit 1; }

run_twin() {
  local twin="$1" expect="$2"
  local dir="$HERE/$twin"
  echo
  echo "==================== twin: $twin (expect: $expect) ===================="
  rm -f "$dir"/blake3-512:*.proof 2>/dev/null
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/__pycache__" 2>/dev/null
  rm -f "$dir"/.prove*.json "$dir"/.verify*.json 2>/dev/null

  ( cd "$dir" && "$BIN" mint --out . ) >/dev/null || { echo "FAIL: mint ($twin)"; return 1; }
  ( cd "$dir" && "$BIN" verify --project . --json > .verify.json ) || true
  [ -s "$dir/.verify.json" ] || { echo "FAIL: no verify receipt ($twin)"; return 1; }

  EXPECT="$expect" TWIN="$twin" python3 - "$dir/.verify.json" <<'PY' || return 1
import json, os, sys
expect, twin = os.environ["EXPECT"], os.environ["TWIN"]
doc = json.load(open(sys.argv[1]))
found = [
    (r.get("property", ""), r.get("status", ""))
    for r in doc.get("rows", [])
    if "base64_encode" in str(r.get("property", ""))
]
if not found:
    print(f"FAIL({twin}): no base64_encode property rows in receipt"); sys.exit(1)
statuses = {s for _, s in found}
print(f"rows({twin}):")
for n, s in found:
    print(f"  {s:14s} {n[:110]}")
ok_words = {"discharged", "proven", "consistent", "sat"}
bad_words = {"unsatisfied", "refused", "unsat", "contradictory", "inconsistent", "violation", "violated"}
if expect == "discharged":
    verdict_ok = statuses & ok_words and not (statuses & bad_words)
else:
    verdict_ok = bool(statuses & bad_words)
if not verdict_ok:
    print(f"FAIL({twin}): expected {expect}, statuses={sorted(statuses)}"); sys.exit(1)
print(f"OK({twin}): {expect}")
PY
}

fail=0
run_twin good discharged || fail=1
run_twin bad refused || fail=1

echo
if [ "$fail" -ne 0 ]; then
  echo "==== itsdangerous-token-padding: FAIL ===="
  exit 1
fi
echo "==== itsdangerous-token-padding: PASS ===="
echo "the padded-token confusion refuted statically by one byte literal"
echo "(rstrip(b'=')) from itsdangerous' own source -- the real-name logo."
