#!/usr/bin/env bash
# Canonical sugar-cli self-application runner.
#
# Mints the dependency proofs (libsugar + rust-std shim), places them in the
# cli's verify pool, mints sugar-cli (all four surfaces) with the Tier 2b
# rust-analyzer oracle and the loud pipeline logging, then proves it. Prints the
# three gates and the discharge scoreboard. Read docs/self-application/
# KIT-SETUP-AND-SELF-APPLICATION.md for the why.
#
# Usage:  scripts/self-apply.sh [--no-oracle]
#   --no-oracle  skip the ~minutes rust-analyzer cold index (method-call
#                receivers stay unresolved; the body-discharge + cross-crate
#                free-function bridges still run, in ~30s).
#
# Idempotent. Writes scratch to /tmp/self-apply/*. Does NOT commit anything.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || { echo "not in the sugar repo"; exit 1; }

BIN=implementations/rust/target/debug/sugar
CLI=implementations/rust/sugar-cli
IMPORTS="$CLI/.sugar/imports"
SCRATCH=/tmp/self-apply
LOG="$SCRATCH/run.log"
ORACLE_ENV=()
[ "${1:-}" != "--no-oracle" ] && ORACLE_ENV=(SUGAR_RESOLVE_ORACLE=rust-analyzer)

[ -x "$BIN" ] || { echo "build first: (cd implementations/rust && cargo build -p sugar-cli -p sugar-walk)"; exit 1; }
rm -rf "$SCRATCH"; mkdir -p "$SCRATCH" "$IMPORTS"; rm -f "$IMPORTS"/*.proof; : > "$LOG"

mint_dep () {  # <project-dir> <short-name>
  local dir="$1" name="$2" out="$SCRATCH/dep-$2"
  rm -rf "$out"; mkdir -p "$out"
  echo "==== mint dep: $name ====" | tee -a "$LOG"
  RUST_LOG=info "$BIN" mint --project "$dir" --out "$out" 2>>"$LOG" | tee -a "$LOG" >/dev/null
  local p; p=$(ls -t "$out"/*.proof 2>/dev/null | head -1)
  [ -n "$p" ] && [ -f "$p" ] || { echo "!! no .proof for $name in $out (check the manifest command paths, see the runbook)"; exit 1; }
  cp "$p" "$IMPORTS/$(basename "$p")"
  echo ">> placed $(basename "$p") ($(wc -c <"$p") bytes) into imports" | tee -a "$LOG"
}

mint_dep implementations/rust/libsugar          libsugar
mint_dep examples/sugar-shim-rust-std           shim-std

echo "==== mint sugar-cli (oracle: ${ORACLE_ENV:+on}${ORACLE_ENV:-off}) ====" | tee -a "$LOG"
env "${ORACLE_ENV[@]}" RUST_LOG=info,sugar_walk_rpc=info \
  "$BIN" mint --project "$CLI" --out "$SCRATCH/cli" 2>"$SCRATCH/mint.err" | tee -a "$LOG" >/dev/null
CLIPROOF=$(ls -t "$SCRATCH/cli"/*.proof 2>/dev/null | head -1)

echo; echo "==== GATE 1: dependency contracts forwarded (>0) ===="
grep -iE "dependency contract.*forwarded|dep_forwarded" "$SCRATCH/mint.err" | tail -1
echo "==== GATE 2: oracle resolution (16+/N is good; 0/N means cold-index timeout or oracle off) ===="
grep -iE "batch complete: resolved|oracle resolved [0-9]" "$SCRATCH/mint.err" | tail -2
echo "==== eligibility + drops (want: high eligible, 0 body-discharge-ineligible drops) ===="
sed 's/\x1b\[[0-9;]*m//g' "$SCRATCH/mint.err" | grep -E "function_contract_lift:|lift_implications: complete|call sites had a MATCHING" | tail -3

echo; echo "==== prove (scoreboard) ===="
"$BIN" prove "$CLI" --with "$SCRATCH/cli" --json > "$SCRATCH/prove.json" 2>"$SCRATCH/prove.err"
echo "totals:"; grep -E '"totalCallsites"|"discharged"|"violations"' "$SCRATCH/prove.json"
echo "discharge methods:"; grep -oE '"method": "[a-z-]+"' "$SCRATCH/prove.json" | sort | uniq -c
echo "reasons:"; grep -oE '"reason": "[a-z -]+' "$SCRATCH/prove.json" | sed 's/.*: "//' | sort | uniq -c | sort -rn | head
echo
echo "artifacts in $SCRATCH/ (run.log, mint.err, prove.json, prove.err). cli proof: $CLIPROOF"
