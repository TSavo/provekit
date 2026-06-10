#!/usr/bin/env bash
#
# run.sh — end-to-end driver for the Base64 generalization prototype.
#
#   1. builds the AST walker (Base64Walker.java, com.sun.source, --release 21)
#   2. runs it against the VENDORED commons-codec source -> walker.json (facts)
#   3. emits SMT-LIB2 from those facts (emit_smt.js; reads ONLY the JSON)
#   4. runs z3 on all four checks and ASSERTS on the results:
#        A. strong_derive   -> sat, derived Y == vendor "Zm9v" byte-for-byte
#        B. strong_unique   -> unsat  (Y pinned uniquely)
#        C. refute_alphabet -> unsat  (no standard-table output is '-'/'_')
#        D. weak_alphabet   -> unsat  (out-of-alphabet membership claim)
#
# Skip-guards: missing JDK (javac) or z3 -> exit 0 with a SKIP notice (honest:
# the experiment did not run, it was not falsely reported green). Any FAILED
# assertion -> exit 1.

set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

VENDOR_B64="vendor/Base64.java"
VENDOR_NCODEC="vendor/BaseNCodec.java"

# ---- skip-guards ---------------------------------------------------------
if ! command -v javac >/dev/null 2>&1; then
  echo "SKIP: javac (JDK) not found; cannot build the AST walker."
  exit 0
fi
if ! command -v java >/dev/null 2>&1; then
  echo "SKIP: java not found."
  exit 0
fi
if ! command -v z3 >/dev/null 2>&1; then
  echo "SKIP: z3 not found; cannot discharge the constraints."
  exit 0
fi
if ! command -v node >/dev/null 2>&1; then
  echo "SKIP: node not found; cannot run the SMT emitter."
  exit 0
fi
for f in "$VENDOR_B64" "$VENDOR_NCODEC"; do
  if [ ! -f "$f" ]; then
    echo "SKIP: vendored source missing: $f (run the fetch in PROVENANCE.md)."
    exit 0
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ---- 1. build walker -----------------------------------------------------
echo "== building AST walker =="
javac --release 21 -d "$WORK" walker/Base64Walker.java || { echo "FAIL: walker compile"; exit 1; }

# ---- 2. walk vendored source --------------------------------------------
echo "== walking vendored commons-codec source =="
java -cp "$WORK" Base64Walker "$VENDOR_B64" "$VENDOR_NCODEC" > "$WORK/walker.json" \
  || { echo "FAIL: walker run"; cat "$WORK/walker.json"; exit 1; }
echo "-- walker.json --"
cat "$WORK/walker.json"

# ---- 3. emit SMT ---------------------------------------------------------
echo "== emitting SMT-LIB2 from walked facts =="
EXPECTED="$(node emit_smt.js "$WORK/walker.json" "$WORK")" || { echo "FAIL: emit_smt"; exit 1; }
echo "$EXPECTED"
EXPECTED_Y="$(echo "$EXPECTED" | sed -n 's/^EXPECTED_Y=//p')"   # e.g. 90,109,57,118

# ---- 4. run z3 + assert --------------------------------------------------
rc=0

echo "== A. strong_derive (expect sat; derived Y must equal vendor vector) =="
A_OUT="$(z3 "$WORK/strong_derive.smt2")"
echo "$A_OUT"
# parse derived bytes from the model (#xHH) in y0..y3 order
DERIVED="$(echo "$A_OUT" | grep -oE '#x[0-9a-fA-F]{2}' | while read -r h; do
  printf '%d,' "$((16#${h#\#x}))"; done | sed 's/,$//')"
if [ "$(echo "$A_OUT" | head -1)" = "sat" ] && [ "$DERIVED" = "$EXPECTED_Y" ]; then
  echo "PASS A: z3 derived Y = $DERIVED == vendor $EXPECTED_Y (\"Zm9v\")"
else
  echo "FAIL A: derived=[$DERIVED] expected=[$EXPECTED_Y] first-line=[$(echo "$A_OUT" | head -1)]"
  rc=1
fi

echo "== B. strong_unique (expect unsat) =="
B_OUT="$(z3 "$WORK/strong_unique.smt2")"
echo "$B_OUT"
if [ "$(echo "$B_OUT" | head -1)" = "unsat" ]; then echo "PASS B: Y pinned uniquely"; else echo "FAIL B"; rc=1; fi

echo "== C. refute_alphabet (expect unsat) =="
C_OUT="$(z3 "$WORK/refute_alphabet.smt2")"
echo "$C_OUT"
if [ "$(echo "$C_OUT" | head -1)" = "unsat" ]; then echo "PASS C: no standard-table output is '-'/'_'"; else echo "FAIL C"; rc=1; fi

echo "== D. weak_alphabet (expect unsat) =="
D_OUT="$(z3 "$WORK/weak_alphabet.smt2")"
echo "$D_OUT"
if [ "$(echo "$D_OUT" | head -1)" = "unsat" ]; then echo "PASS D: out-of-alphabet claim refuted"; else echo "FAIL D"; rc=1; fi

echo
if [ "$rc" -eq 0 ]; then echo "ALL CHECKS PASSED"; else echo "SOME CHECKS FAILED"; fi
exit "$rc"
