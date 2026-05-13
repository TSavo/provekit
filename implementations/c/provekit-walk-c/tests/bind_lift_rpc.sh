#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/provekit-bind-lift-c"
FIXTURE="/tmp/cbindkit-fixture"
OUT="/tmp/cbindkit-rpc.out"

mkdir -p "$FIXTURE"
cat > "$FIXTURE/foo.c" <<'EOF'
// concept: identity
int wrap_identity(int x) {
    return x;
}

// concept: bool-cell
int toggle(int flag) {
    return !flag;
}

// concept: option
int maybe_first(int *items, int len) {
    if (len == 0) return -1;
    else return items[0];
}
EOF

"$BIN" <<EOF > "$OUT"
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":"$FIXTURE","source_paths":["foo.c"]}}
{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}
EOF

python3 - "$OUT" <<'PY'
import json
import sys

path = sys.argv[1]
lines = [json.loads(line) for line in open(path, encoding="utf-8") if line.strip()]
assert len(lines) == 3, lines

init = lines[0]["result"]
assert init["protocol_version"] == "pep/1.7.0"
assert init["name"] == "provekit-bind-lift-c"
assert init["capabilities"]["authoring_surfaces"] == ["c", "c11", "c-bind"]
assert init["capabilities"]["ir_version"] == "bind-ir/1.0.0"
assert init["capabilities"]["emits_signed_mementos"] is False

doc = lines[1]["result"]
assert doc["kind"] == "ir-document"
entries = doc["ir"]
assert [entry["concept_annotation"] for entry in entries] == ["identity", "bool-cell", "option"]
assert [entry["fn_name"] for entry in entries] == ["wrap_identity", "toggle", "maybe_first"]
assert entries[0]["param_names"] == ["x"]
assert entries[0]["param_types"] == ["int"]
assert entries[0]["return_type"] == "int"
assert entries[2]["param_names"] == ["items", "len"]
assert entries[2]["param_types"] == ["int *", "int"]
assert all(entry["kind"] == "bind-lift-entry" for entry in entries)
assert all(entry["term_shape_cid"].startswith("blake3-512:") for entry in entries)
assert all(len(entry["term_shape_cid"]) == len("blake3-512:") + 128 for entry in entries)

wire = json.dumps(doc, sort_keys=True)
assert "c:" not in wire
assert "c11:" not in wire

print(json.dumps(doc, sort_keys=True, separators=(",", ":")))
PY
