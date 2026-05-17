#!/bin/sh
set -eu

bin="./target/release/provekit-realize-c"

if [ ! -x "$bin" ]; then
    printf 'missing executable: %s\n' "$bin" >&2
    exit 1
fi

responses="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"provekit.plugin.invoke","params":{"function":"wrap_identity","params":["x"],"param_types":["int"],"return_type":"int","concept_name":"identity"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"provekit.plugin.invoke","params":{"function":"free_p","params":["p"],"param_types":["int *"],"return_type":"void","concept_name":"free"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"provekit.plugin.invoke","params":{"function":"bad_sort","params":["x"],"param_types":["int"],"return_type":"MysterySort","concept_name":"identity"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":5,"method":"shutdown","params":{}}'
    } | "$bin" --rpc
)"

printf '%s\n' "$responses" | grep '"name":"provekit-realize-c"' >/dev/null
printf '%s\n' "$responses" | grep '"protocol_version":"pep/1.7.0"' >/dev/null
printf '%s\n' "$responses" | grep '"authoring_surfaces":\["c","c11"\]' >/dev/null
printf '%s\n' "$responses" | grep '"id":2' >/dev/null
printf '%s\n' "$responses" | grep '"source":"int wrap_identity(int x) {\\n    return x;\\n}\\n"' >/dev/null
printf '%s\n' "$responses" | grep '"is_stub":false' >/dev/null
printf '%s\n' "$responses" | grep '"id":3' >/dev/null
printf '%s\n' "$responses" | grep '"source":"void free_p(int \*p) {\\n    free(p);\\n}\\n"' >/dev/null
printf '%s\n' "$responses" | grep '"id":4' >/dev/null
printf '%s\n' "$responses" | grep '"error":{"code":-32602,"message":"UNSUPPORTED_SORT: no C type mapping for MysterySort"}' >/dev/null

python3 - "$bin" <<'PY'
import json
import subprocess
import sys

import blake3

BIN = sys.argv[1]
CONCEPT_CID = "blake3-512:1dcfe69eb5a7c6719d3faf2f4d073dfe81f37a4d930a37b7a535aa3de9eae7dc30965bf6d8fa451a9cb97608335a844f619adb4589d0a5e882b30fa98d60b3a9"
SITE_CID = "blake3-512:" + "11" * 64
LOSS_CID = "blake3-512:" + "22" * 64
SUGAR_CID = "blake3-512:" + "33" * 64
POLICY_CID = "blake3-512:" + "44" * 64


def cid(value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return "blake3-512:" + blake3.blake3(data).digest(length=64).hex()


def invoke(params):
    request = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "provekit.plugin.invoke",
            "params": params,
        },
        separators=(",", ":"),
    )
    proc = subprocess.run(
        [BIN, "--rpc"],
        input=request + "\n",
        text=True,
        stdout=subprocess.PIPE,
        check=True,
    )
    return json.loads(proc.stdout)


def test_concept_citation_comment_emitted_for_transported_operation():
    args_jcs = {"kind": "call-args", "values": [{"kind": "var", "name": "sql"}]}
    response = invoke(
        {
            "function": "transport_sql",
            "params": ["sql"],
            "param_types": ["const char*"],
            "return_type": "void",
            "concept_name": "concept:sql-execute",
            "transported_operation": {
                "args_jcs": args_jcs,
                "callsite_cid": "blake3-512:" + "55" * 64,
                "concept_cid": CONCEPT_CID,
                "concept_name": "concept:sql-execute",
                "concept_site_cid": SITE_CID,
                "loss_record_cid": LOSS_CID,
                "operation_kind": "sql-execute",
                "policy_cid": POLICY_CID,
                "shape_cid": CONCEPT_CID,
                "sugar_dict_cid": SUGAR_CID,
                "target_library_tag": "c-core",
                "term_position": [0],
            },
        }
    )
    source = response["result"]["source"]
    assert "/* concept: sql-execute(sql) */" in source, source
    assert "// provekit-concept: " in source, source
    assert "// provekit-concept-payload-cid: " in source, source
    assert "/* provekit-bind canonical:" not in source, source
    assert "__builtin_trap" not in source and "abort()" not in source, source
    assert "(void)0;" in source, source
    payload_line = next(
        line for line in source.splitlines() if line.strip().startswith("// provekit-concept: ")
    )
    payload = json.loads(payload_line.split(": ", 1)[1])
    assert payload["artifact_kind"] == "provekit-concept-citation-comment-sugar"
    assert payload["schema_version"] == "1"
    assert payload["emitted_by"]["target_language"] == "c"
    assert payload["emitted_by"]["target_library_tag"] == "c-core"
    assert payload["args_jcs_cid"] == cid(args_jcs)


test_concept_citation_comment_emitted_for_transported_operation()
PY

python3 tests/conformance.py "$bin"
