#!/usr/bin/env bash
# Run the BZ-DETERMINISM-001 seam obligation through the full six-solver
# portfolio (maude, z3, cvc5, vampire, coq, lean) and print each seat's
# verdict. The obligation is the species' real composition check, lifted
# from the Go production call site:
#
#   fixed   : (result = 0)     -> ((result < 0) = false)   [VALID    -> discharged]
#   exhibit : (result = value) -> ((result < 0) = false)   [INVALID  -> counterexample]
#
# The portfolio + binaries come from the repo-root .provekit/config.toml
# (mode = first-wins). Missing seats degrade to Undecidable.
#
# Usage: from repo root, with z3/cvc5/vampire/coqc/lean/maude on PATH:
#   ./menagerie/bug-zoo/species/BZ-DETERMINISM-001-canonical-byte-order/six-solver-portfolio.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
PK="${PROVEKIT_CLI:-$ROOT/implementations/rust/target/release/provekit}"
SP="$ROOT/menagerie/bug-zoo/species/BZ-DETERMINISM-001-canonical-byte-order"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cat > "$WORK/fixed.json" <<'JSON'
{ "kind": "implies", "operands": [
  { "kind": "atomic", "name": "=", "args": [
    { "kind": "var", "name": "result" },
    { "kind": "const", "sort": { "kind": "primitive", "name": "Int" }, "value": 0 } ] },
  { "kind": "atomic", "name": "=", "args": [
    { "kind": "ctor", "name": "<", "args": [
      { "kind": "var", "name": "result" },
      { "kind": "const", "sort": { "kind": "primitive", "name": "Int" }, "value": 0 } ] },
    { "kind": "const", "sort": { "kind": "primitive", "name": "Bool" }, "value": false } ] } ] }
JSON

cat > "$WORK/exhibit.json" <<'JSON'
{ "kind": "implies", "operands": [
  { "kind": "atomic", "name": "=", "args": [
    { "kind": "var", "name": "result" },
    { "kind": "var", "name": "value" } ] },
  { "kind": "atomic", "name": "=", "args": [
    { "kind": "ctor", "name": "<", "args": [
      { "kind": "var", "name": "result" },
      { "kind": "const", "sort": { "kind": "primitive", "name": "Int" }, "value": 0 } ] },
    { "kind": "const", "sort": { "kind": "primitive", "name": "Bool" }, "value": false } ] } ] }
JSON

run_one() {
  local label="$1" file="$2"
  echo "==== $label obligation ===="
  ( cd "$ROOT" && "$PK" prove --formula "$file" --json 2>/dev/null ) | python3 -c "
import sys, json
d = json.load(sys.stdin)
print('  overall:', d['status'])
for i in sorted(d['solverInvocations'], key=lambda x: x['solver']):
    err = (i['error'] or '')[:50]
    print(f\"    {i['solver']:8} {i['status']:13} {i['wallClockMs']:>6}ms  {err}\")
"
}

run_one "FIXED (expect discharged)" "$WORK/fixed.json"
run_one "EXHIBIT (expect counterexample)" "$WORK/exhibit.json"
