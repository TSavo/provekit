#!/usr/bin/env bash
# Focused source-warrant emission test for Java universe contracts.
set -euo pipefail

command -v javac >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java  >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT="$(cd "$HERE/.." && pwd)"
OUT="$KIT/out"
FIXTURES="$HERE/fixtures"

echo "== build kit =="
bash "$KIT/build.sh" "$OUT" >/dev/null 2>&1

if grep -q 'substring(0, memento.length() - 1)' "$KIT/src/JavaTestAssertionsRpc.java"; then
  echo "FAIL: source warrants must use a structured model writer, not JSON splice surgery" >&2
  exit 1
fi
if ! grep -q 'record SourceWarrant' "$KIT/src/JavaTestAssertionsRpc.java"; then
  echo "FAIL: source warrants must be represented as a model object" >&2
  exit 1
fi

JAVA_CMD="java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -cp $OUT JavaTestAssertionsRpc"

lift_result_from() {
python3 - "$1" "$2" <<'PY' | eval "$JAVA_CMD" 2>/dev/null
import json, sys
workspace_root, source_path = sys.argv[1], sys.argv[2]
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "workspace_root": workspace_root,
    "source_paths": [source_path],
}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
}

lift_result() {
  lift_result_from "$FIXTURES/strong-universe" "$1"
}

RESULT="$(lift_result "StrongUniverseLift.java")"
TAIL_RESULT="$(lift_result "StrongTailLift.java")"
COMMONS_CRC_RESULT="$(lift_result_from "$FIXTURES/commons-codec-crc32" "CommonsCodecCrc32Test.java")"

python3 - "$RESULT" "$TAIL_RESULT" "$COMMONS_CRC_RESULT" <<'PY'
import json, sys

def rpc_result(raw):
    lines = [json.loads(l) for l in raw.strip().splitlines() if l.strip()]
    return next(obj["result"] for obj in lines if obj.get("id") == 2)

result = rpc_result(sys.argv[1])
tail_result = rpc_result(sys.argv[2])
commons_crc_result = rpc_result(sys.argv[3])
ir = result["ir"]
source_ledger = result.get("sourceLedger")
assert isinstance(source_ledger, dict), result
assert source_ledger["source_loci"] > 0, source_ledger
assert source_ledger["source_warranted"] > 0, source_ledger
assert source_ledger["source_refused"] > 0, source_ledger
assert source_ledger["source_inactive"] > 0, source_ledger
assert source_ledger["source_refuted"] == 0, source_ledger
assert source_ledger["source_work"] == 0, source_ledger
assert source_ledger["unclassified_source"] == 0, source_ledger

source_mementos = result.get("sourceMementos")
assert isinstance(source_mementos, list) and source_mementos, result
roles = {m.get("role") for m in source_mementos}
assert "java.weak-universe" in roles, source_mementos
assert "java.strong-universe" in roles, source_mementos
assert "java.test-fact" in roles, source_mementos
fact = next(m for m in source_mementos if m.get("role") == "java.test-fact")
assert fact["kind"] == "source-memento", fact
assert fact["claimName"].endswith("::assertion"), fact
assert fact["contractName"].endswith("::assertion"), fact
assert fact["source_function_name"] == "testFullBlockStrong", fact
assert fact["file"].endswith("StrongUniverseLift.java"), fact
assert fact["source_cid"].startswith("blake3-512:"), fact
assert fact["template_cid"].startswith("blake3-512:"), fact
assert "bodyText" not in fact and "body_text" not in fact, fact
assert "templateJson" not in fact and "ast_template" not in fact, fact

source_audits = result.get("sourceAudits")
assert isinstance(source_audits, list) and source_audits, result
for audit in source_audits:
    assert audit["kind"] == "source-audit", audit
    assert audit["language"] == "java", audit
    assert audit["source_memento"]["kind"] == "source-memento", audit
    assert "bodyText" not in audit["source_memento"], audit
    assert "body_text" not in audit["source_memento"], audit
    assert audit["totals"]["source_loci"] == len(audit["loci"]), audit
    assert any(locus["status"] == "warranted" for locus in audit["loci"]), audit
    for locus in audit["loci"]:
        assert locus.get("ast_path", "").startswith("$."), locus
        assert locus.get("span", {}).get("start_line", 0) > 0, locus
        assert locus.get("line_range") == [
            locus["span"]["start_line"],
            locus["span"]["end_line"],
        ], locus

strong_audit = next(
    audit
    for audit in source_audits
    if audit["role"] == "java.strong-universe"
)
strong_statuses = {locus["status"] for locus in strong_audit["loci"]}
assert {"warranted", "refused", "inactive"} <= strong_statuses, strong_audit
assert "unclassified" not in strong_statuses, strong_audit
by_line = {locus["line"]: locus["status"] for locus in strong_audit["loci"]}
assert by_line[780] == "warranted", strong_audit
assert by_line[737] == "inactive", strong_audit
assert next(locus for locus in strong_audit["loci"] if locus["line"] == 737)["ast_kind"] == "SWITCH", strong_audit
assert next(locus for locus in strong_audit["loci"] if locus["line"] == 740)["ast_kind"] == "CASE", strong_audit
assert ".FOR_LOOP@" in next(locus for locus in strong_audit["loci"] if locus["line"] == 780)["ast_path"], strong_audit
assert strong_audit["totals"]["source_refused"] > 0, strong_audit
assert strong_audit["totals"]["source_inactive"] > 0, strong_audit
assert strong_audit["totals"]["unclassified_source"] == 0, strong_audit

tail_audits = [
    audit for audit in tail_result["sourceAudits"]
    if audit["role"] == "java.strong-universe"
]
assert len(tail_audits) == 2, tail_audits
by_contract = {audit["contract"]["name"]: audit for audit in tail_audits}
ba_audit = next(audit for name, audit in by_contract.items() if "s:ba" in name)
f_audit = next(audit for name, audit in by_contract.items() if "s:f" in name)
ba_by_line = {locus["line"]: locus for locus in ba_audit["loci"]}
f_by_line = {locus["line"]: locus for locus in f_audit["loci"]}
assert ba_by_line[737]["status"] == "warranted", ba_audit
assert ba_by_line[752]["status"] == "warranted", ba_audit
assert ba_by_line[753]["status"] == "warranted", ba_audit
assert ba_by_line[757]["status"] == "warranted", ba_audit
assert ba_by_line[758]["status"] == "warranted", ba_audit
assert ba_by_line[740]["status"] == "inactive", ba_audit
assert ba_by_line[742]["status"] == "inactive", ba_audit
assert ba_by_line[780]["status"] == "inactive", ba_audit
assert f_by_line[737]["status"] == "warranted", f_audit
assert f_by_line[740]["status"] == "warranted", f_audit
assert f_by_line[742]["status"] == "warranted", f_audit
assert f_by_line[746]["status"] == "warranted", f_audit
assert f_by_line[747]["status"] == "warranted", f_audit
assert f_by_line[752]["status"] == "inactive", f_audit
assert f_by_line[753]["status"] == "inactive", f_audit
assert f_by_line[780]["status"] == "inactive", f_audit
for audit in tail_audits:
    assert audit["totals"]["unclassified_source"] == 0, audit

crc_contracts = [
    contract for contract in commons_crc_result["ir"]
    if contract["name"].endswith("::crc-value-pin")
]
assert len(crc_contracts) == 1, commons_crc_result
crc_mementos = commons_crc_result.get("sourceMementos")
assert isinstance(crc_mementos, list) and crc_mementos, commons_crc_result
assert any(m.get("role") == "java.crc-value-pin" for m in crc_mementos), crc_mementos
assert any(m.get("role") == "java.test-fact" for m in crc_mementos), crc_mementos
crc_warrants = crc_contracts[0].get("sourceWarrants")
assert isinstance(crc_warrants, list) and len(crc_warrants) == 1, crc_contracts[0]
crc_warrant = crc_warrants[0]
assert crc_warrant["role"] == "java.crc-value-pin", crc_warrant
assert crc_warrant["universe_kind"] == "crc32.eq-walked", crc_warrant
assert crc_warrant["file"].endswith("vendor/commons-codec/org/apache/commons/codec/digest/PureJavaCrc32.java"), crc_warrant
assert crc_warrant["source_function_name"] == "update", crc_warrant
assert crc_warrant["span"]["start_line"] == 598, crc_warrant
assert crc_warrant["span"]["end_line"] == 637, crc_warrant
assert "bodyText" not in crc_warrant and "body_text" not in crc_warrant, crc_warrant
assert "templateJson" not in crc_warrant and "ast_template" not in crc_warrant, crc_warrant

crc_audits = [
    audit for audit in commons_crc_result.get("sourceAudits", [])
    if audit["role"] == "java.crc-value-pin"
]
assert len(crc_audits) == 1, commons_crc_result
crc_audit = crc_audits[0]
assert crc_audit["totals"]["source_loci"] > 0, crc_audit
assert crc_audit["totals"]["source_warranted"] > 0, crc_audit
assert crc_audit["totals"]["source_refused"] > 0, crc_audit
assert crc_audit["totals"]["source_inactive"] > 0, crc_audit
assert crc_audit["totals"]["unclassified_source"] == 0, crc_audit
assert any(
    locus["status"] == "warranted"
    and locus["ast_kind"] == "EXPRESSION_STATEMENT"
    and "crc32.eq-walked" in locus["reason"]
    for locus in crc_audit["loci"]
), crc_audit
crc_by_line = {locus["line"]: locus for locus in crc_audit["loci"]}
for line in [605, 606, 607, 609, 610, 611, 612]:
    assert crc_by_line[line]["status"] == "warranted", (line, crc_audit)
    assert "slicing-by-8" in crc_by_line[line]["reason"], (line, crc_by_line[line])
for line in (616, 617, 618, 629, 630, 636):
    assert line in crc_by_line, (line, crc_audit)

def atom_name(contract):
    return contract["inv"]["operands"][0]["name"]

weak = [c for c in ir if atom_name(c) == "str.chars-in-set"]
strong = [c for c in ir if atom_name(c) == "str.eq-bv-blocks"]
assert len(weak) == 1, f"expected one weak universe row, got {len(weak)}"
assert len(strong) == 1, f"expected one strong universe row, got {len(strong)}"

for contract, role in [(weak[0], "java.weak-universe"), (strong[0], "java.strong-universe")]:
    warrants = contract.get("sourceWarrants")
    assert isinstance(warrants, list) and len(warrants) == 1, contract
    warrant = warrants[0]
    assert warrant["kind"] == "source-memento", warrant
    assert warrant["role"] == role, warrant
    assert warrant["source_cid"].startswith("blake3-512:"), warrant
    assert warrant["template_cid"].startswith("blake3-512:"), warrant
    assert warrant["file"].endswith(".java"), warrant
    assert warrant["span"]["start_line"] > 0, warrant
    assert "bodyText" not in warrant and "body_text" not in warrant, warrant
    assert "templateJson" not in warrant and "ast_template" not in warrant, warrant

print("PASS: Java universe contracts emit lean source-oracle warrants, including Commons Codec CRC32")
PY
