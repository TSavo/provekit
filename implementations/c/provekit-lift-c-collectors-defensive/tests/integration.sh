#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-collectors-defensive"
FIXTURE="$SCRIPT_DIR/fixtures/trivial.c"
DEFENSIVE_FIXTURE="$SCRIPT_DIR/fixtures/defensive.c"
CHECKED_FIXTURE="$SCRIPT_DIR/fixtures/checked_demo.c"
BRANCH_FIXTURE="$SCRIPT_DIR/fixtures/branch_returns.c"
TERM_UNSUPPORTED_FIXTURE="$SCRIPT_DIR/fixtures/term_unsupported.c"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
FOO_EXPECTED_TERM="$REPO_ROOT/menagerie/c11-language-signature/example/foo.term.json"

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

if [ ! -f "$CHECKED_FIXTURE" ]; then
    echo "FAIL: fixture not found: $CHECKED_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$BRANCH_FIXTURE" ]; then
    echo "FAIL: fixture not found: $BRANCH_FIXTURE" >&2
    exit 1
fi

if [ ! -f "$TERM_UNSUPPORTED_FIXTURE" ]; then
    echo "FAIL: fixture not found: $TERM_UNSUPPORTED_FIXTURE" >&2
    exit 1
fi

if [ "${PK_C_ENABLE_CLANG_AST:-}" = "1" ] && [ ! -f "$FOO_EXPECTED_TERM" ]; then
    echo "FAIL: expected term fixture not found: $FOO_EXPECTED_TERM" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["trivial.c","defensive.c","checked_demo.c","branch_returns.c"],"surface":"c-collectors-defensive"}}\n'
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

def post_term(name):
    post = by_name[name].get("post", {})
    if post.get("kind") != "atomic" or post.get("name") != "=":
        fail(f"{name} post is not an equality: {post}")
    args = post.get("args", [])
    if len(args) != 2 or args[0] != var("result"):
        fail(f"{name} post does not equate result: {post}")
    return args[1]

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
    (
        "checked",
        "post",
        atom("=", [var("result"), var("x")]),
        "checked trailing return did not lift result = x",
    ),
]

for fn_name, field, expected, message in checks:
    decl = by_name.get(fn_name)
    if decl is None:
        fail(f"{fn_name} function not found in declarations")
    if not contains_formula(decl.get(field), expected):
        fail(message)

def is_true_formula(formula):
    return formula == atom("true", [])

for fn_name in [
    "bug_on_nonnegative",
    "errno_guard",
    "ret_guard",
    "goto_error",
    "assert_positive",
    "checked",
    "handled_null_store",
    "sg_nents_for_len",
    "sg_miter_next",
    "crypto_skcipher_encrypt",
    "crypto_skcipher_decrypt",
    "skb_to_sgvec",
    "__skb_to_sgvec",
]:
    decl = by_name.get(fn_name)
    if decl is None:
        fail(f"{fn_name} function not found in declarations")
    if not is_true_formula(decl.get("pre")):
        fail(f"{fn_name} internal defensive guard leaked into pre: {decl.get('pre')}")

checked_decl = by_name.get("checked")
if checked_decl is None:
    fail("checked function not found in declarations")
if contains_formula(checked_decl.get("post"), atom("=", [var("result"), const_int(-1)])):
    fail("checked postcondition used BUG_ON macro return -1 instead of trailing return x")

branch_checks = {
    "branch_foo": ctor("ite", [
        ctor("=", [var("x"), const_int(0)]),
        const_int(-22),
        var("x"),
    ]),
    "branch_else": ctor("ite", [
        ctor("=", [var("x"), const_int(0)]),
        const_int(-22),
        var("x"),
    ]),
    "branch_block": ctor("ite", [
        ctor("<", [var("x"), const_int(0)]),
        const_int(-1),
        ctor("+", [var("x"), const_int(1)]),
    ]),
    "branch_nested": ctor("ite", [
        ctor("<", [var("x"), const_int(0)]),
        ctor("ite", [
            ctor("<", [var("x"), const_int(-10)]),
            const_int(-10),
            const_int(-1),
        ]),
        var("x"),
    ]),
    "early_return": ctor("ite", [
        ctor("truthy", [var("err")]),
        const_int(-5),
        var("ok"),
    ]),
}

for fn_name, expected in branch_checks.items():
    if fn_name not in by_name:
        fail(f"{fn_name} function not found in declarations")
    actual = post_term(fn_name)
    if actual != expected:
        fail(f"{fn_name} post term mismatch: {actual}")

refusals = lift["result"].get("refusals", [])
if not any(
    r.get("kind") == "loop-requires-invariant"
    and r.get("surface") == "c-collectors-defensive"
    and r.get("reason") == "loop body requires an invariant-backed contract"
    for r in refusals
):
    fail(f"loop_refusal did not emit loop refusal: {refusals}")

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

if [ "${PK_C_ENABLE_CLANG_AST:-}" = "1" ]; then
    TERM_RESPONSES="$(
        {
            printf '%s\n' '{"jsonrpc":"2.0","id":10,"method":"initialize","params":{}}'
            printf '{"jsonrpc":"2.0","id":11,"method":"lift","params":{"workspace_root":'
            printf '"%s"' "$REPO_ROOT"
            printf ',"source_paths":["menagerie/c11-language-signature/example/foo.c","implementations/c/provekit-lift-c-collectors-defensive/tests/fixtures/term_unsupported.c"],"surface":"c-collectors-defensive"}}\n'
            printf '%s\n' '{"jsonrpc":"2.0","id":12,"method":"shutdown"}'
        } | "$BIN" --rpc
    )"

    TERM_RESPONSES_JSON="$TERM_RESPONSES" FOO_EXPECTED_TERM="$FOO_EXPECTED_TERM" python3 - <<'PY'
import json
import os

responses = [json.loads(line) for line in os.environ["TERM_RESPONSES_JSON"].splitlines() if line.strip()]
lift = next((r for r in responses if r.get("id") == 11), None)
if lift is None:
    raise SystemExit("FAIL: term lift response missing")

decls = lift["result"]["declarations"]
terms = [decl for decl in decls if decl.get("kind") == "c11-algebra-term"]
contracts = [decl for decl in decls if decl.get("kind") == "function-contract"]

def fail(message):
    raise SystemExit(f"FAIL: {message}\n{json.dumps(lift, sort_keys=True)}")

def surface_key(text):
    return "".join((text or "").split())

def normalize_term(value):
    if isinstance(value, dict):
        return {
            key: normalize_term(child)
            for key, child in value.items()
            if key != "op_cid"
        }
    if isinstance(value, list):
        return [normalize_term(child) for child in value]
    return value

foo_terms = [
    term for term in terms
    if term.get("fn_name") == "foo" or term.get("source") == "foo.c"
]
if len(foo_terms) != 1:
    fail(f"expected exactly one c11-algebra-term for foo, found {len(foo_terms)}")
foo = foo_terms[0]

expected = json.load(open(os.environ["FOO_EXPECTED_TERM"], encoding="utf-8"))
expected_surface = "seq(if(eq(x, 0), return(neg(22)), skip), return(x))"
if surface_key(foo.get("term_surface")) != surface_key(expected_surface):
    fail(f"foo term_surface mismatch: {foo.get('term_surface')!r}")

if normalize_term(foo.get("term")) != normalize_term(expected["term"]):
    fail(f"foo term AST mismatch: {foo.get('term')}")

unsupported_names = {"unsupported_goto", "unsupported_ternary"}
contract_names = {decl.get("fn_name") for decl in contracts}
if not unsupported_names <= contract_names:
    fail(f"unsupported fixture did not still emit function-contracts: {contract_names}")

term_names = {term.get("fn_name") for term in terms}
if unsupported_names & term_names:
    fail(f"unsupported functions emitted c11-algebra-term declarations: {term_names}")

messages = [
    diag.get("message", "")
    for diag in lift["result"].get("diagnostics", [])
    if isinstance(diag, dict)
]
for name in unsupported_names:
    needle = f"term-emit skipped fn={name}: unsupported AST node"
    if not any(needle in message for message in messages):
        fail(f"missing fail-closed diagnostic for {name}: {messages}")
PY
else
    echo "SKIP: c11-algebra-term checks (PK_C_ENABLE_CLANG_AST not set)"
fi

echo "provekit-lift-c-collectors-defensive integration passed"
