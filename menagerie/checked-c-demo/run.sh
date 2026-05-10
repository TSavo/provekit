#!/bin/bash
# C analog of the Python checked() / unittest gold-pipeline demo.
# Lifts via kunit (callsite-attached observations on the same callee), then
# extracts the lifted IR formulas and conjoins them via the substrate's
# canonical IrFormula AND constructor. The result is dispatched to provekit
# prove (which runs the solver portfolio).
#
# No hand-written SMT. The only file with formulas in it is the lifter's
# JSON output. The conjunction is built mechanically from those.
set -e

DEMO_DIR=/workspace/provekit/menagerie/checked-c-demo
OUT=${OUT:-/workspace/out}
mkdir -p "$OUT"

run_lift() {
    local lifter=$1
    local surface=$2
    local source=$3
    local output=$4
    {
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$DEMO_DIR\",\"surface\":\"$surface\",\"source_paths\":[\"$source\"]}}"
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$lifter" --rpc > "$OUT/$output" 2>"$OUT/$output.stderr"
}

echo "=== lift checked_kunit.c via kunit ==="
run_lift provekit-lift-c-kunit c-kunit tests/checked_kunit.c kunit.json
python3 -c "import json
for line in open('$OUT/kunit.json'):
    obj = json.loads(line)
    if obj.get('id') != 2: continue
    for d in obj['result']['declarations']:
        print(f\"  {d['name']:50s} fn={d['fn_name']}  post={d['post']['name']}\")"

echo
echo "=== mechanically conjoin the two kunit posts via IrFormula AND ==="
python3 <<PYEOF > "$OUT/conjoined.json"
import json
posts = []
for line in open("$OUT/kunit.json"):
    obj = json.loads(line)
    if obj.get("id") != 2: continue
    for d in obj["result"]["declarations"]:
        if d["fn_name"] == "checked":
            posts.append(d["post"])
conjoined = {"kind": "and", "operands": posts}
print(json.dumps(conjoined, indent=2))
PYEOF
sed 's/^/  /' "$OUT/conjoined.json"

echo
echo "=== provekit prove --formula <conjoined> ==="
provekit prove --formula "$(cat $OUT/conjoined.json)" 2>&1 | sed 's/^/  /' | head -20

echo
echo "=== interpretation ==="
echo "  The conjoined formula is the substrate's joint claim from BOTH kunit"
echo "  observations on checked(42). One says ==42, the other says !=42. The"
echo "  substrate cannot satisfy both for a deterministic function — provekit's"
echo "  solver portfolio refutes the conjunction, surfacing the broken test"
echo "  before it ever runs."
