#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUILD="$ROOT/target/test-classes"
WORK="$ROOT/target/test-workspace"

rm -rf "$BUILD" "$WORK"
mkdir -p "$BUILD" "$WORK/.sugar/imports"
cp "$HERE/fixtures/PanamaConsumer.java" "$WORK/PanamaConsumer.java"

javac -d "$BUILD" "$ROOT/src/PanamaFfmLiftRpc.java"

request='{"jsonrpc":"2.0","id":1,"method":"lift","params":{"workspace_root":"'"$WORK"'","source_paths":["."],"contract_bindings":[{"name":"decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion","contract_cid":"blake3-512:java-source-cid"},{"name":"decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion","contract_cid":"blake3-512:rust-target-cid","target_proof_cid":"blake3-512:rust-proof-cid"}]}}'
printf '%s\n' "$request" | java -cp "$BUILD" PanamaFfmLiftRpc > "$WORK/response.json"

python3 - "$WORK/response.json" "$WORK/java-panama-ffm.call-edges.json" <<'PY'
import json
import sys

response_path, sidecar_path = sys.argv[1:3]
response = json.load(open(response_path, encoding="utf-8"))
result = response["result"]
edges = result["callEdges"]
assert len(edges) == 1, edges
edge = edges[0]
expected = "rust-kit:decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
assert edge["targetSymbol"] == expected, edge
assert edge["sourceContractCid"] == "blake3-512:java-source-cid", edge
assert edge["targetContractCid"] == "blake3-512:rust-target-cid", edge
sidecar = json.load(open(sidecar_path, encoding="utf-8"))
assert sidecar["edges"] == edges, sidecar
print("panama lifter test ok:", edge["targetSymbol"])
PY
