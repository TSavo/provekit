#!/usr/bin/env bash
# Focused source-warrant emission test for Java universe contracts.
set -euo pipefail

command -v javac >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java  >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT="$(cd "$HERE/.." && pwd)"
OUT="$KIT/out"
FIXTURES="$HERE/fixtures"

echo "== build kit =="
bash "$KIT/build.sh" "$OUT" >/dev/null 2>&1

if grep -q 'substring(0, memento.length() - 1)' "$KIT/src/JavaTestAssertionsRpc.java"; then
  echo "FAIL: source warrants must use a structured model writer, not JSON splice surgery" >&2
  exit 1
fi
if ! grep -q 'record SourceWarrant' "$KIT/src/JavaTestAssertionsRpc.java"; then
  echo "FAIL: source warrants must be represented as a model object" >&2
  exit 1
fi

JAVA_CMD="java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -cp $OUT JavaTestAssertionsRpc"

RESULT="$(
python3 - "$FIXTURES/strong-universe" "StrongUniverseLift.java" <<'PY' | eval "$JAVA_CMD" 2>/dev/null
import json, sys
workspace_root, source_path = sys.argv[1], sys.argv[2]
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "workspace_root": workspace_root,
    "source_paths": [source_path],
}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
)"

python3 - "$RESULT" <<'PY'
import json, sys

lines = [json.loads(l) for l in sys.argv[1].strip().splitlines() if l.strip()]
result = next(obj["result"] for obj in lines if obj.get("id") == 2)
ir = result["ir"]

def atom_name(contract):
    return contract["inv"]["operands"][0]["name"]

weak = [c for c in ir if atom_name(c) == "str.chars-in-set"]
strong = [c for c in ir if atom_name(c) == "str.eq-bv-blocks"]
assert len(weak) == 1, f"expected one weak universe row, got {len(weak)}"
assert len(strong) == 1, f"expected one strong universe row, got {len(strong)}"

for contract, role in [(weak[0], "java.weak-universe"), (strong[0], "java.strong-universe")]:
    warrants = contract.get("sourceWarrants")
    assert isinstance(warrants, list) and len(warrants) == 1, contract
    warrant = warrants[0]
    assert warrant["kind"] == "source-memento", warrant
    assert warrant["role"] == role, warrant
    assert warrant["source_cid"].startswith("blake3-512:"), warrant
    assert warrant["template_cid"].startswith("blake3-512:"), warrant
    assert warrant["file"].endswith(".java"), warrant
    assert warrant["span"]["start_line"] > 0, warrant
    assert "bodyText" not in warrant and "body_text" not in warrant, warrant
    assert "templateJson" not in warrant and "ast_template" not in warrant, warrant

print("PASS: Java universe contracts emit lean source-oracle warrants")
PY
