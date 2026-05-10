#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-walk-c"
FIXTURE="$SCRIPT_DIR/fixtures/wp_basic.c"
CHECKED_FIXTURE="$SCRIPT_DIR/fixtures/checked_demo.c"
OPAQUE_BUG_ON_FIXTURE="$SCRIPT_DIR/fixtures/checked_demo_opaque_bug_on.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

if [ ! -f "$FIXTURE" ]; then
    echo "FAIL: fixture not found: $FIXTURE" >&2
    exit 1
fi

if [ ! -f "$CHECKED_FIXTURE" ]; then
    echo "FAIL: fixture not found: $CHECKED_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$OPAQUE_BUG_ON_FIXTURE" ]; then
    echo "FAIL: fixture not found: $OPAQUE_BUG_ON_FIXTURE" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["wp_basic.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-walk"' || {
    echo "FAIL: initialize missing c-walk name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

RESPONSES_JSON="$RESPONSES" python3 - <<'PY'
import json
import os

responses = [json.loads(line) for line in os.environ["RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: lift response missing")

decls = lift["result"]["declarations"]
chains = [
    d for d in decls
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
]
if not chains:
    raise SystemExit("FAIL: no wp-walk-chain function-contract emitted")

chain = chains[0]
arrivals = chain["evidence"].get("arrivals", [])
if len(arrivals) < 3:
    raise SystemExit(f"FAIL: expected >=3 arrivals, got {len(arrivals)}")

kinds = [a.get("kind") for a in arrivals[:3]]
if kinds != ["Callsite", "LetBinding", "FunctionEntry"]:
    raise SystemExit(f"FAIL: first three arrivals wrong: {kinds}")

entry = next((a for a in arrivals if a.get("kind") == "FunctionEntry"), None)
if entry is None:
    raise SystemExit("FAIL: FunctionEntry arrival missing")
if entry.get("wp", {}).get("kind") == "atomic" and entry["wp"].get("name") == "true":
    raise SystemExit("FAIL: FunctionEntry WP is trivial true")

post = chain.get("post")
if post != arrivals[0].get("wp"):
    raise SystemExit("FAIL: chain post must match callsite WP")
if chain.get("pre") != entry.get("wp"):
    raise SystemExit("FAIL: chain pre must match function entry WP")

print("sample-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY

CHECKED_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["checked_demo.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

CHECKED_RESPONSES_JSON="$CHECKED_RESPONSES" python3 - <<'PY'
import json
import os

responses = [json.loads(line) for line in os.environ["CHECKED_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: checked_demo lift response missing")

decls = lift["result"]["declarations"]
chains = [
    d for d in decls
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
    and d.get("evidence", {}).get("caller") == "composed_ok"
    and d.get("evidence", {}).get("callee") == "checked"
]
if not chains:
    raise SystemExit("FAIL: no checked_demo composed_ok -> checked wp-walk-chain emitted")

chain = chains[0]
arrivals = chain["evidence"].get("arrivals", [])
if len(arrivals) < 3:
    raise SystemExit(f"FAIL: checked_demo expected >=3 arrivals, got {len(arrivals)}")

kinds = [a.get("kind") for a in arrivals[:3]]
if kinds != ["Callsite", "LetBinding", "FunctionEntry"]:
    raise SystemExit(f"FAIL: checked_demo first three arrivals wrong: {kinds}")

binding = arrivals[1]
if binding.get("name") != "y":
    raise SystemExit(f"FAIL: checked_demo LetBinding should be y, got {binding.get('name')}")

entry = next((a for a in arrivals if a.get("kind") == "FunctionEntry"), None)
if entry is None:
    raise SystemExit("FAIL: checked_demo FunctionEntry arrival missing")

entry_wp = entry.get("wp", {})
if entry_wp.get("kind") == "atomic" and entry_wp.get("name") == "true":
    raise SystemExit("FAIL: checked_demo FunctionEntry WP is trivial true")

if entry_wp.get("kind") != "atomic" or entry_wp.get("name") != "≥":
    raise SystemExit(f"FAIL: checked_demo FunctionEntry WP should be ≥ atom, got {entry_wp}")
args = entry_wp.get("args", [])
if len(args) != 2 or args[0].get("kind") != "const" or args[0].get("value") != 42 or args[1].get("kind") != "const" or args[1].get("value") != 10:
    raise SystemExit(f"FAIL: checked_demo FunctionEntry WP should be 42 ≥ 10, got {entry_wp}")

if chain.get("post") != arrivals[0].get("wp"):
    raise SystemExit("FAIL: checked_demo chain post must match callsite WP")
if chain.get("pre") != entry.get("wp"):
    raise SystemExit("FAIL: checked_demo chain pre must match function entry WP")

print("checked-demo-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY

OPAQUE_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["checked_demo_opaque_bug_on.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

CHECKED_RESPONSES_JSON="$OPAQUE_RESPONSES" CHECKED_LABEL="opaque BUG_ON" python3 - <<'PY'
import json
import os

label = os.environ["CHECKED_LABEL"]
responses = [json.loads(line) for line in os.environ["CHECKED_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit(f"FAIL: {label} lift response missing")

decls = lift["result"]["declarations"]
chains = [
    d for d in decls
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
    and d.get("evidence", {}).get("caller") == "composed_ok"
    and d.get("evidence", {}).get("callee") == "checked"
]
if not chains:
    raise SystemExit(f"FAIL: no {label} composed_ok -> checked wp-walk-chain emitted")

arrivals = chains[0]["evidence"].get("arrivals", [])
if len(arrivals) < 3:
    raise SystemExit(f"FAIL: {label} expected >=3 arrivals, got {len(arrivals)}")

entry = next((a for a in arrivals if a.get("kind") == "FunctionEntry"), None)
if entry is None:
    raise SystemExit(f"FAIL: {label} FunctionEntry arrival missing")
entry_wp = entry.get("wp", {})
if entry_wp.get("kind") != "atomic" or entry_wp.get("name") != "≥":
    raise SystemExit(f"FAIL: {label} FunctionEntry WP should be ≥ atom, got {entry_wp}")
args = entry_wp.get("args", [])
if len(args) != 2 or args[0].get("kind") != "const" or args[0].get("value") != 42 or args[1].get("kind") != "const" or args[1].get("value") != 10:
    raise SystemExit(f"FAIL: {label} FunctionEntry WP should be 42 ≥ 10, got {entry_wp}")

print("opaque-bug-on-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY
