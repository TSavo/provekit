#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-collectors-defensive"
FIXTURE="$SCRIPT_DIR/fixtures/trivial.c"
DEFENSIVE_FIXTURE="$SCRIPT_DIR/fixtures/defensive.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

if [ ! -f "$FIXTURE" ]; then
    echo "FAIL: fixture not found: $FIXTURE" >&2
    exit 1
fi

if [ ! -f "$DEFENSIVE_FIXTURE" ]; then
    echo "FAIL: fixture not found: $DEFENSIVE_FIXTURE" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["trivial.c","defensive.c"],"surface":"c-collectors-defensive"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"id":1' || {
    echo "FAIL: initialize did not echo id 1" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-collectors-defensive"' || {
    echo "FAIL: initialize missing c-collectors-defensive name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"provekit-lift/1"' || {
    echo "FAIL: initialize missing protocol version" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":2' || {
    echo "FAIL: lift did not echo id 2" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# declarations[] must be non-empty (at least one function-contract per function with body)
printf '%s\n' "$RESPONSES" | grep -q '"declarations":\[{' || {
    echo "FAIL: declarations must be non-empty" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# Each synthesized contract must have kind: function-contract
printf '%s\n' "$RESPONSES" | grep -q '"kind":"function-contract"' || {
    echo "FAIL: declarations must contain function-contract entries" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# fn_name field must be present
printf '%s\n' "$RESPONSES" | grep -q '"fn_name"' || {
    echo "FAIL: function-contract entries must have fn_name field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# pre must be present (trivial true predicate)
printf '%s\n' "$RESPONSES" | grep -q '"pre"' || {
    echo "FAIL: function-contract entries must have pre field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# post must be present
printf '%s\n' "$RESPONSES" | grep -q '"post"' || {
    echo "FAIL: function-contract entries must have post field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# Verify known function names are lifted (regex backend always finds them)
printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"add"' || {
    echo "FAIL: add function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"identity"' || {
    echo "FAIL: identity function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"negate"' || {
    echo "FAIL: negate function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"caller"' || {
    echo "FAIL: caller function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"effects":{"effects":\[\]}' || {
    echo "FAIL: declarations must include explicit effects:{effects:[]} field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

RESPONSES_JSON="$RESPONSES" python3 - <<'PY'
import json
import os
import sys

responses = [json.loads(line) for line in os.environ["RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 2), None)
if lift is None:
    raise SystemExit("FAIL: lift response missing")
decls = lift["result"]["declarations"]
by_name = {
    decl["fn_name"]: decl
    for decl in decls
    if decl.get("kind") == "function-contract"
}

def fail(message):
    raise SystemExit(f"FAIL: {message}\n{json.dumps(by_name, sort_keys=True)}")

def var(name):
    return {"kind": "var", "name": name}

def const_int(value):
    return {
        "kind": "const",
        "value": value,
        "sort": {"kind": "primitive", "name": "Int"},
    }

def ctor(name, args):
    return {"kind": "ctor", "name": name, "args": args}

def atom(name, args):
    return {"kind": "atomic", "name": name, "args": args}

def contains_formula(formula, expected):
    if formula == expected:
        return True
    if isinstance(formula, dict):
        for key in ("operands", "args"):
            if any(contains_formula(part, expected) for part in formula.get(key, [])):
                return True
    return False

checks = [
    (
        "bug_on_nonnegative",
        "pre",
        atom("\u2265", [var("x"), const_int(0)]),
        "BUG_ON(x < 0) did not lift x >= 0",
    ),
    (
        "errno_guard",
        "pre",
        atom("\u2260", [var("ptr"), var("NULL")]),
        "if (!ptr) return -ENOMEM did not lift ptr != NULL",
    ),
    (
        "user_buffer",
        "pre",
        atom("is_user_ptr", [var("buf")]),
        "__user char *buf did not lift is_user_ptr(buf)",
    ),
    (
        "held_lock",
        "pre",
        atom("lock_held", [var("lock")]),
        "__must_hold(lock) did not lift lock_held(lock)",
    ),
    (
        "trailing_return",
        "post",
        atom("=", [var("result"), ctor("+", [var("x"), const_int(1)])]),
        "trailing return x + 1 did not lift result = x + 1",
    ),
    (
        "ret_guard",
        "pre",
        atom("\u2265", [var("ret"), const_int(0)]),
        "if (ret < 0) return ret did not lift ret >= 0",
    ),
    (
        "goto_error",
        "pre",
        atom("\u2260", [var("x"), const_int(0)]),
        "if (x == 0) goto error did not lift x != 0",
    ),
    (
        "assert_positive",
        "pre",
        atom(">", [var("x"), const_int(0)]),
        "assert(x > 0) did not lift x > 0",
    ),
    (
        "rcu_pointer",
        "pre",
        atom("is_rcu_protected", [var("p")]),
        "__rcu int *p did not lift is_rcu_protected(p)",
    ),
    (
        "sized_count",
        "pre",
        atom("\u2265", [var("n"), const_int(0)]),
        "size_t n did not lift n >= 0",
    ),
    (
        "gfp_flags",
        "pre",
        atom("valid_gfp_flags", [var("gfp")]),
        "gfp_t gfp did not lift valid_gfp_flags(gfp)",
    ),
]

for fn_name, field, expected, message in checks:
    decl = by_name.get(fn_name)
    if decl is None:
        fail(f"{fn_name} function not found in declarations")
    if not contains_formula(decl.get(field), expected):
        fail(message)

effect_checks = [
    ("acquire_lock", {"kind": "lock_acquire", "target": "lock"}),
    ("release_lock", {"kind": "lock_release", "target": "lock"}),
]

for fn_name, expected in effect_checks:
    decl = by_name.get(fn_name)
    if decl is None:
        fail(f"{fn_name} function not found in declarations")
    effects = decl.get("effects", {}).get("effects", [])
    if expected not in effects:
        fail(f"{fn_name} missing effect {expected}")
PY

echo "provekit-lift-c-collectors-defensive integration passed"
