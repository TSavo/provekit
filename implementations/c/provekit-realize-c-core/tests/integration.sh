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

python3 - "$bin" <<'PY'
import json
import subprocess
import sys

BIN = sys.argv[1]

def platform_semantics():
    proc = subprocess.run(
        [BIN, "--rpc"],
        input='{"jsonrpc":"2.0","id":7,"method":"provekit.plugin.platform_semantics"}\n',
        text=True,
        stdout=subprocess.PIPE,
        check=True,
    )
    return json.loads(proc.stdout)

CONCEPT_LITERAL_CID = (
    "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd"
    "7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6"
)
GOLDEN_SORT_ADMISSION_DV_CID = (
    "blake3-512:3f4313896772a69aefb7ec0367d53d763e14640e48382c76d96167815696a978"
    "18e2e3bdd2f65700f3eee66f0d34b92856d387429d246d241d2f93396a3ed131"
)
GOLDEN_CONCEPT_LITERAL_TAG_CID = (
    "blake3-512:25abe129555ae18b71e40424a074d3d0743577839683fa71a360ffca69fcb555"
    "70985314f33d3fb038ab8f34eba4a3d683f70edbc7ae2e55fad09d9015fd067c"
)

def test_platform_semantics_shape():
    r = platform_semantics()
    assert r["jsonrpc"] == "2.0"
    assert r["id"] == 7
    res = r["result"]
    assert isinstance(res["tags"], list), "tags must be list"
    assert isinstance(res["dimension_values"], list), "dimension_values must be list"
    assert res["op_aliases"] == {}, "op_aliases must be empty"
    assert len(res["tags"]) == 18, f"expected 18 tags, got {len(res['tags'])}"
    assert len(res["dimension_values"]) == 6, f"expected 6 dimension_values, got {len(res['dimension_values'])}"

def test_platform_semantics_golden_dim_cids():
    res = platform_semantics()["result"]
    dvs = {dv["dimension_name"]: dv for dv in res["dimension_values"]}
    assert dvs["ArithmeticOverflow"]["cid"] == (
        "blake3-512:00e74d3e7971790ce523ea5506858c39300f33b47287d8e4b375d6e31813c4fa"
        "be0e271805fd37c7998a14980a44608e9dd712ad6ae10de7c7abd5050b246d99"
    ), "ArithmeticOverflow CID mismatch"
    assert dvs["IntegerDivisionRounding"]["cid"] == (
        "blake3-512:9e0bab38bcfcce97b77c174bb8a729bde99ae5c84149e23be59d4715586ec361"
        "d291eb623c0fc77ee627dd76a301cde925199e3738093eb643755b3655613be5"
    ), "IntegerDivisionRounding CID mismatch"
    assert dvs["ShiftMode"]["cid"] == (
        "blake3-512:f43d802cd2046fb9bc1983618ea03560cf9432d409c70ec3329af737a7646516"
        "154b38f751f12429683e769f505aa116f73467ff0212be92aba3ac062e63ddc7"
    ), "ShiftMode CID mismatch"
    assert dvs["NullSemantics"]["cid"] == (
        "blake3-512:514cb8e34010a16cf8b2af064f7e801ec2385ba9dbe3e25b2b5c4d5dfe8fe369"
        "fe21c47f8890533e5f7efb29b93a13753028b544d2e167609d53c08a30dbb4ea"
    ), "NullSemantics CID mismatch"
    assert dvs["BitwiseSemantics"]["cid"] == (
        "blake3-512:5b77e0ae0696a1690183175edfdba3780db940c080d2c3992bbff85b4d312df8"
        "cd5b54bfff3a47f5a6a887f532143469d76e2b1131835e42716998932174094d"
    ), "BitwiseSemantics CID mismatch"
    assert dvs["SortAdmission"]["cid"] == GOLDEN_SORT_ADMISSION_DV_CID, "SortAdmission CID mismatch"

def test_platform_semantics_compare_to_structure():
    res = platform_semantics()["result"]
    dvs = {dv["dimension_name"]: dv for dv in res["dimension_values"]}
    for dim_name, dv in dvs.items():
        ct = dv["compare_to"]
        assert ct["kind"] == "atomic", f"{dim_name}: compare_to kind must be atomic"
        if dim_name == "SortAdmission":
            assert ct["name"] == "admits_sorts", f"{dim_name}: compare_to name must be admits_sorts"
            assert len(ct["args"]) > 0, f"{dim_name}: compare_to args must not be empty"
            for arg in ct["args"]:
                assert arg["kind"] == "const", f"{dim_name}: arg kind must be const"
                assert arg["sort"]["kind"] == "primitive", f"{dim_name}: arg sort kind must be primitive"
                assert arg["sort"]["name"] == "cid", f"{dim_name}: arg sort name must be cid"
                assert arg["value"].startswith("blake3-512:"), f"{dim_name}: arg value must be a CID"
        else:
            assert ct["args"] == [], f"{dim_name}: compare_to args must be empty"
            assert ct["name"].startswith("c:"), f"{dim_name}: compare_to name must start with c:"

def test_platform_semantics_op_tags_have_five_dimensions():
    res = platform_semantics()["result"]
    for tag in res["tags"]:
        if tag["op_cid"] == CONCEPT_LITERAL_CID:
            continue
        dims = tag["dimensions"]
        assert len(dims) == 5, f"tag {tag['op_cid'][:40]} must have 5 dimensions"
        for k in ("ArithmeticOverflow", "IntegerDivisionRounding", "ShiftMode", "NullSemantics", "BitwiseSemantics"):
            assert k in dims, f"tag missing dimension {k}"

def test_concept_literal_has_sort_admission_only():
    res = platform_semantics()["result"]
    literal_tags = [t for t in res["tags"] if t["op_cid"] == CONCEPT_LITERAL_CID]
    assert len(literal_tags) == 1, f"expected exactly one concept:literal tag, got {len(literal_tags)}"
    lt = literal_tags[0]
    assert list(lt["dimensions"].keys()) == ["SortAdmission"], (
        f"concept:literal must have only SortAdmission, got {list(lt['dimensions'].keys())}"
    )
    assert lt["dimensions"]["SortAdmission"] == GOLDEN_SORT_ADMISSION_DV_CID, (
        "concept:literal SortAdmission CID mismatch"
    )
    assert lt["cid"] == GOLDEN_CONCEPT_LITERAL_TAG_CID, "concept:literal tag CID mismatch"

test_platform_semantics_shape()
test_platform_semantics_golden_dim_cids()
test_platform_semantics_compare_to_structure()
test_platform_semantics_op_tags_have_five_dimensions()
test_concept_literal_has_sort_admission_only()
print("platform_semantics tests passed")
PY

python3 tests/conformance.py "$bin"
