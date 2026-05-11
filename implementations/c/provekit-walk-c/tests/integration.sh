#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-walk-c"
FIXTURE="$SCRIPT_DIR/fixtures/wp_basic.c"
CHECKED_FIXTURE="$SCRIPT_DIR/fixtures/checked_demo.c"
OPAQUE_BUG_ON_FIXTURE="$SCRIPT_DIR/fixtures/checked_demo_opaque_bug_on.c"
TWO_ARMED_FIXTURE="$SCRIPT_DIR/fixtures/two_armed_if.c"
CALL_STATEMENT_FIXTURE="$SCRIPT_DIR/fixtures/call_statement.c"
HANDLED_GUARD_FIXTURE="$SCRIPT_DIR/fixtures/handled_guard.c"
ACTUALS_FIXTURE="$SCRIPT_DIR/fixtures/callsite_actuals.c"

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

if [ ! -f "$TWO_ARMED_FIXTURE" ]; then
    echo "FAIL: fixture not found: $TWO_ARMED_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$CALL_STATEMENT_FIXTURE" ]; then
    echo "FAIL: fixture not found: $CALL_STATEMENT_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$HANDLED_GUARD_FIXTURE" ]; then
    echo "FAIL: fixture not found: $HANDLED_GUARD_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$ACTUALS_FIXTURE" ]; then
    echo "FAIL: fixture not found: $ACTUALS_FIXTURE" >&2
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
if entry_wp.get("kind") != "atomic" or entry_wp.get("name") != "true":
    raise SystemExit(f"FAIL: checked_demo handled BUG_ON should not become caller pre, got {entry_wp}")

callsite_wp = arrivals[0].get("wp", {})
if callsite_wp.get("kind") != "atomic" or callsite_wp.get("name") != "true":
    raise SystemExit(f"FAIL: checked_demo callsite WP should be true, got {callsite_wp}")

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
if entry_wp.get("kind") != "atomic" or entry_wp.get("name") != "true":
    raise SystemExit(f"FAIL: {label} handled BUG_ON should not become caller pre, got {entry_wp}")

callsite_wp = arrivals[0].get("wp", {})
if callsite_wp.get("kind") != "atomic" or callsite_wp.get("name") != "true":
    raise SystemExit(f"FAIL: {label} callsite WP should be true, got {callsite_wp}")

print("opaque-bug-on-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY

TWO_ARMED_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["two_armed_if.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

TWO_ARMED_RESPONSES_JSON="$TWO_ARMED_RESPONSES" python3 - <<'PY'
import json
import os

responses = [json.loads(line) for line in os.environ["TWO_ARMED_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: two_armed_if lift response missing")

decls = lift["result"]["declarations"]
chains = [
    d for d in decls
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
    and d.get("evidence", {}).get("caller") == "two_armed"
]

def has_atom(formula, name, lhs, rhs):
    if formula.get("kind") == "atomic" and formula.get("name") == name:
        args = formula.get("args", [])
        return (
            len(args) == 2
            and args[0].get("kind") == lhs[0]
            and args[0].get(lhs[1]) == lhs[2]
            and args[1].get("kind") == rhs[0]
            and args[1].get(rhs[1]) == rhs[2]
        )
    return any(has_atom(op, name, lhs, rhs) for op in formula.get("operands", []))

def find_chain(callee, branch, guard_name):
    matches = [c for c in chains if c.get("evidence", {}).get("callee") == callee]
    if not matches:
        raise SystemExit(f"FAIL: no two_armed -> {callee} wp-walk-chain emitted")
    chain = matches[0]
    arrivals = chain["evidence"].get("arrivals", [])
    arm = next((a for a in arrivals if a.get("kind") == "ConditionalArm"), None)
    if arm is None:
        raise SystemExit(f"FAIL: {callee} chain missing ConditionalArm arrival")
    if arm.get("branch") != branch:
        raise SystemExit(f"FAIL: {callee} ConditionalArm branch should be {branch}, got {arm.get('branch')}")
    if not has_atom(arm.get("cond", {}), guard_name, ("var", "name", "x"), ("const", "value", 50)):
        raise SystemExit(f"FAIL: {callee} ConditionalArm cond missing x {guard_name} 50: {arm.get('cond')}")
    entry = next((a for a in arrivals if a.get("kind") == "FunctionEntry"), None)
    if entry is None:
        raise SystemExit(f"FAIL: {callee} FunctionEntry arrival missing")
    entry_wp = entry.get("wp", {})
    if entry_wp.get("kind") == "atomic" and entry_wp.get("name") == "true":
        raise SystemExit(f"FAIL: {callee} FunctionEntry WP is trivial true")
    if not has_atom(entry_wp, guard_name, ("var", "name", "x"), ("const", "value", 50)):
        raise SystemExit(f"FAIL: {callee} FunctionEntry WP missing guard x {guard_name} 50: {entry_wp}")
    if chain.get("post") != arrivals[0].get("wp"):
        raise SystemExit(f"FAIL: {callee} chain post must match callsite WP")
    if chain.get("pre") != entry.get("wp"):
        raise SystemExit(f"FAIL: {callee} chain pre must match function entry WP")
    print(f"two-armed-{callee}-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))

find_chain("helper_a", "then", ">")
find_chain("helper_b", "else", "≤")
PY

CALL_STATEMENT_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["call_statement.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

CALL_STATEMENT_RESPONSES_JSON="$CALL_STATEMENT_RESPONSES" python3 - <<'PY'
import json
import os

def is_true(formula):
    return formula.get("kind") == "atomic" and formula.get("name") == "true" and not formula.get("args")

responses = [json.loads(line) for line in os.environ["CALL_STATEMENT_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: call_statement lift response missing")

chains = [
    d for d in lift["result"]["declarations"]
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
    and d.get("evidence", {}).get("caller") == "call_statement"
    and d.get("evidence", {}).get("callee") == "external_call"
]
if not chains:
    raise SystemExit("FAIL: external call statement did not emit wp-walk-chain")

chain = chains[0]
arrivals = chain.get("evidence", {}).get("arrivals", [])
if len(arrivals) < 2:
    raise SystemExit(f"FAIL: external call statement expected callsite and entry arrivals, got {arrivals}")
callsite = arrivals[0]
if callsite.get("kind") != "Callsite" or callsite.get("stmt_index") != 0:
    raise SystemExit(f"FAIL: external call statement arrival should be Callsite at stmt 0, got {callsite}")
if not is_true(callsite.get("wp", {})):
    raise SystemExit(f"FAIL: external call statement WP should be true, got {callsite.get('wp')}")
if chain.get("post") != callsite.get("wp"):
    raise SystemExit("FAIL: external call statement post must match callsite WP")
print("external-call-statement-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY

HANDLED_GUARD_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["handled_guard.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

HANDLED_GUARD_RESPONSES_JSON="$HANDLED_GUARD_RESPONSES" python3 - <<'PY'
import json
import os

def is_true(formula):
    return formula.get("kind") == "atomic" and formula.get("name") == "true" and not formula.get("args")

responses = [json.loads(line) for line in os.environ["HANDLED_GUARD_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: handled_guard lift response missing")

chains = [
    d for d in lift["result"]["declarations"]
    if d.get("kind") == "function-contract"
    and d.get("evidence", {}).get("kind") == "wp-walk-chain"
    and d.get("evidence", {}).get("caller") == "call_guarded"
    and d.get("evidence", {}).get("callee") == "guarded_store"
]
if not chains:
    raise SystemExit("FAIL: call_guarded -> guarded_store wp-walk-chain missing")

chain = chains[0]
arrivals = chain.get("evidence", {}).get("arrivals", [])
if len(arrivals) < 2:
    raise SystemExit(f"FAIL: handled guard expected callsite and entry arrivals, got {arrivals}")
if not is_true(arrivals[0].get("wp", {})):
    raise SystemExit(f"FAIL: handled null guard leaked into callsite WP: {arrivals[0].get('wp')}")
entry = next((a for a in arrivals if a.get("kind") == "FunctionEntry"), None)
if entry is None:
    raise SystemExit("FAIL: handled guard FunctionEntry arrival missing")
if not is_true(entry.get("wp", {})):
    raise SystemExit(f"FAIL: handled null guard leaked into entry WP: {entry.get('wp')}")
if not is_true(chain.get("pre", {})):
    raise SystemExit(f"FAIL: handled null guard chain pre should be true, got {chain.get('pre')}")
print("handled-guard-chain", " -> ".join(f"{a['kind']}@{a.get('line', 0)}:{a.get('column', 0)}" for a in arrivals))
PY

ACTUALS_RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["callsite_actuals.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

ACTUALS_RESPONSES_AGAIN="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["callsite_actuals.c"],"surface":"c-walk","parse_backend":"clang_ast"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

ACTUALS_RESPONSES_JSON="$ACTUALS_RESPONSES" ACTUALS_RESPONSES_AGAIN_JSON="$ACTUALS_RESPONSES_AGAIN" python3 - <<'PY'
import json
import os

def callsite_arrival(blob):
    responses = [json.loads(line) for line in blob.splitlines() if line.strip()]
    lift = next((r for r in responses if r.get("id") == 2), None)
    if lift is None:
        raise SystemExit("FAIL: callsite_actuals lift response missing")

    chains = [
        d for d in lift["result"]["declarations"]
        if d.get("kind") == "function-contract"
        and d.get("evidence", {}).get("kind") == "wp-walk-chain"
        and d.get("evidence", {}).get("caller") == "actuals_caller"
        and d.get("evidence", {}).get("callee") == "actuals_callee"
    ]
    if not chains:
        raise SystemExit("FAIL: no actuals_caller -> actuals_callee wp-walk-chain emitted")

    arrivals = chains[0]["evidence"].get("arrivals", [])
    callsites = [a for a in arrivals if a.get("kind") == "Callsite"]
    if not callsites:
        raise SystemExit("FAIL: callsite_actuals chain missing Callsite arrival")
    for arrival in callsites:
        if "args" not in arrival:
            raise SystemExit(f"FAIL: Callsite arrival missing args field: {arrival}")
        if not isinstance(arrival["args"], list):
            raise SystemExit(f"FAIL: Callsite arrival args must be an array: {arrival['args']!r}")
    return callsites[0]

arrival = callsite_arrival(os.environ["ACTUALS_RESPONSES_JSON"])
again = callsite_arrival(os.environ["ACTUALS_RESPONSES_AGAIN_JSON"])
args = arrival["args"]

positions = [arg.get("position") for arg in args]
if positions != list(range(len(args))):
    raise SystemExit(f"FAIL: callsite args positions must be contiguous from 0, got {positions}")
if len(args) != 3:
    raise SystemExit(f"FAIL: expected three callsite args, got {len(args)}: {args}")

texts = [arg.get("text") for arg in args]
if texts != ["7", "input", "input+1"]:
    raise SystemExit(f"FAIL: unexpected callsite arg texts: {texts}")

kinds = [arg.get("kind") for arg in args]
if kinds[0] != "IntegerLiteral" or kinds[1] != "DeclRefExpr" or kinds[2] != "BinaryOperator":
    raise SystemExit(f"FAIL: expected literal/ref/binop cursor kinds, got {kinds}")

types = [arg.get("type") for arg in args]
if types != ["int", "int", "int"]:
    raise SystemExit(f"FAIL: expected int arg types, got {types}")

for arg in args:
    term = arg.get("term")
    if not isinstance(term, dict):
        raise SystemExit(f"FAIL: callsite arg missing IR term object: {arg}")
    if term.get("kind") not in {"const", "var", "ctor"}:
        raise SystemExit(f"FAIL: callsite arg term has unexpected shape: {term}")

if arrival != again:
    raise SystemExit(f"FAIL: callsite arrival changed across identical runs: {arrival} != {again}")

print("callsite-actuals", ",".join(f"{a['position']}:{a['kind']}:{a['text']}" for a in args))
PY
