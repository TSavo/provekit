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

python3 - "$BIN" "$ROOT" <<'PY'
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import blake3

BIN = Path(sys.argv[1])
ROOT = Path(sys.argv[2])
REALIZER_ROOT = ROOT.parent / "provekit-realize-c-core"
REALIZER_BIN = REALIZER_ROOT / "target/release/provekit-realize-c"

CONCEPT_CID = "blake3-512:1dcfe69eb5a7c6719d3faf2f4d073dfe81f37a4d930a37b7a535aa3de9eae7dc30965bf6d8fa451a9cb97608335a844f619adb4589d0a5e882b30fa98d60b3a9"
SITE_CID = "blake3-512:" + "11" * 64
LOSS_CID = "blake3-512:" + "22" * 64
SUGAR_CID = "blake3-512:" + "33" * 64
POLICY_CID = "blake3-512:" + "44" * 64
KIT_CID = "blake3-512:" + "aa" * 64


def cid(value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return "blake3-512:" + blake3.blake3(data).digest(length=64).hex()


def base_payload():
    args_jcs = [{"kind": "var", "name": "sql"}]
    return {
        "args_jcs": args_jcs,
        "args_jcs_cid": cid(args_jcs),
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": CONCEPT_CID,
        "concept_name": "concept:sql-execute",
        "concept_site_cid": SITE_CID,
        "emitted_by": {
            "kit_cid": KIT_CID,
            "kit_id": "provekit-realize-c-core@0.1.0",
            "kit_kind": "realize",
            "target_language": "c",
            "target_library_tag": "c-core",
        },
        "loss_record_cid": LOSS_CID,
        "operation_kind": "sql-execute",
        "policy_cid": POLICY_CID,
        "schema_version": "1",
        "shape_cid": CONCEPT_CID,
        "sugar_dict_cid": SUGAR_CID,
        "term_position": [0],
    }


def source_for(payload, payload_cid=None):
    payload_json = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    payload_cid = payload_cid or cid(payload)
    return (
        "void concept_carrier(void) {\n"
        f"    // provekit-concept: {payload_json}\n"
        f"    // provekit-concept-payload-cid: {payload_cid}\n"
        "    (void)0;\n"
        "}\n"
    )


def lift_source(source):
    with tempfile.TemporaryDirectory(prefix="provekit-c-concept-") as tmp:
        tmp_path = Path(tmp)
        (tmp_path / "carrier.c").write_text(source, encoding="utf-8")
        request = "\n".join(
            [
                json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
                json.dumps(
                    {
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "lift",
                        "params": {
                            "workspace_root": str(tmp_path),
                            "source_paths": ["carrier.c"],
                        },
                    }
                ),
                json.dumps({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}}),
            ]
        )
        proc = subprocess.run(
            [str(BIN)],
            input=request + "\n",
            text=True,
            stdout=subprocess.PIPE,
            check=True,
        )
    lines = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
    return lines[1]["result"]


def diagnostic_kinds(doc):
    return {diag.get("kind") for diag in doc.get("diagnostics", [])}


def test_concept_citation_relift_recovers_identity():
    payload = base_payload()
    doc = lift_source(source_for(payload))
    citations = doc.get("concept_citations", [])
    assert len(citations) == 1, json.dumps(doc, sort_keys=True)
    recovered = citations[0]
    for key in [
        "concept_cid",
        "operation_kind",
        "shape_cid",
        "term_position",
        "args_jcs_cid",
    ]:
        assert recovered[key] == payload[key], (key, recovered, payload)


def test_concept_citation_payload_cid_mismatch_refuses():
    payload = base_payload()
    bad_cid = "blake3-512:" + "00" * 64
    doc = lift_source(source_for(payload, payload_cid=bad_cid))
    assert doc.get("concept_citations", []) == [], doc
    assert "concept-citation:payload-cid-mismatch" in diagnostic_kinds(doc), doc


def test_concept_citation_args_cid_mismatch_refuses():
    payload = base_payload()
    payload["args_jcs_cid"] = "blake3-512:" + "ff" * 64
    doc = lift_source(source_for(payload))
    assert doc.get("concept_citations", []) == [], doc
    assert "concept-citation:args-cid-mismatch" in diagnostic_kinds(doc), doc


def test_concept_citation_unknown_schema_version_refuses():
    payload = base_payload()
    payload["schema_version"] = "2"
    doc = lift_source(source_for(payload))
    assert doc.get("concept_citations", []) == [], doc
    assert "concept-citation:unknown-schema-version" in diagnostic_kinds(doc), doc


def realize_source():
    subprocess.run(["make", "-C", str(REALIZER_ROOT)], stdout=subprocess.DEVNULL, check=True)
    params = {
        "function": "transport_sql",
        "params": ["sql"],
        "param_types": ["const char*"],
        "return_type": "void",
        "concept_name": "concept:sql-execute",
        "transported_operation": {
            "args_jcs": [{"kind": "var", "name": "sql"}],
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
    request = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": "provekit.plugin.invoke", "params": params},
        separators=(",", ":"),
    )
    proc = subprocess.run(
        [str(REALIZER_BIN), "--rpc"],
        input=request + "\n",
        text=True,
        stdout=subprocess.PIPE,
        check=True,
    )
    return json.loads(proc.stdout)["result"]["source"]


def test_concept_citation_lower_to_c_relift_round_trip():
    source = realize_source()
    doc = lift_source(source)
    citations = doc.get("concept_citations", [])
    assert len(citations) == 1, json.dumps(doc, sort_keys=True)
    assert citations[0]["concept_cid"] == CONCEPT_CID
    assert citations[0]["operation_kind"] == "sql-execute"
    assert citations[0]["shape_cid"] == CONCEPT_CID


test_concept_citation_relift_recovers_identity()
test_concept_citation_payload_cid_mismatch_refuses()
test_concept_citation_args_cid_mismatch_refuses()
test_concept_citation_unknown_schema_version_refuses()
test_concept_citation_lower_to_c_relift_round_trip()
PY
