#!/usr/bin/env bash
# itsdangerous-token-padding: the real-name logo for the python universe rung.
#
# itsdangerous (Flask's signing dependency) encodes tokens with
#
#     def base64_encode(string):
#         string = want_bytes(string)
#         return base64.urlsafe_b64encode(string).rstrip(b"=")
#
# rstrip is TOTAL: no output of base64_encode ever ends with '=' -- for any
# input, forever, by one byte literal in the vendor's own source. The lifter
# walks that shape (the no-suffix-chars family), reports its ∀⊨sample
# evidence honestly (the wheel ships no test corpus: 0 vendor vectors,
# stated on the universe record), and conjoins ¬suffix-of("=", subject)
# into the callsite's #euf# assertion.
#
# BAD twin: the token-padding confusion -- asserting the PADDED standard
# base64url value where itsdangerous' stripped tokens live (the classic
# JWT/token interop bug). equality ∧ ¬suffixof -> UNSAT, statically.
#
# Verdicts parsed from real .verify.json rows; the verdict FLIP is the
# vacuity witness (a universe that never met the equality would let both
# twins discharge).
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/implementations/rust/target}"
BIN="$TARGET_DIR/debug/sugar"

VENV="${ITSDANGEROUS_LOGO_VENV:-/tmp/itsdangerous-logo-venv}"
export ITSDANGEROUS_LOGO_VENV="$VENV"
if [ ! -x "$VENV/bin/python" ]; then
  echo "== create venv + install the real vendor (itsdangerous) =="
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q itsdangerous
fi
"$VENV/bin/python" -c "import itsdangerous; print('vendor:', 'itsdangerous', itsdangerous.__version__ if hasattr(itsdangerous,'__version__') else '(installed)')" || {
  echo "FAIL: vendor install"; exit 1; }

audit_good_source() {
  echo
  echo "== source audit: sugar lift --report itsdangerous.encoding.base64_encode =="
  local report
  report="$(mktemp)"
  ( cd "$HERE/good" && "$BIN" lift --report --json . ) > "$report" || {
    echo "FAIL: source audit lift report"
    rm -f "$report"
    return 1
  }
  python3 - "$report" <<'PY' || {
import json, sys
from collections import Counter
result = json.load(open(sys.argv[1], encoding="utf-8"))
ledger = result.get("sourceLedger") or {}
audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.translate-universe"
    and "base64_encode" in audit.get("contract", {}).get("name", "")
]
if len(audits) != 1:
    raise SystemExit(f"FAIL: expected one base64_encode universe source audit, got {len(audits)}")
audit = audits[0]
totals = audit["totals"]
if totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: base64 source dig has unclassified source: totals={totals}")
int_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.translate-universe"
    and "int_to_bytes" in audit.get("contract", {}).get("name", "")
]
if len(int_audits) != 1:
    raise SystemExit(f"FAIL: expected one int_to_bytes universe source audit, got {len(int_audits)}")
int_audit = int_audits[0]
int_totals = int_audit["totals"]
if int_audit.get("universe_kind") != "no-prefix-chars":
    raise SystemExit(f"FAIL: expected no-prefix-chars audit, got {int_audit.get('universe_kind')}")
if int_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: int_to_bytes source dig has unclassified source: totals={int_totals}")
signature_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.constant-universe"
    and "NoneAlgorithm.get_signature" in audit.get("contract", {}).get("name", "")
]
if len(signature_audits) != 1:
    raise SystemExit(f"FAIL: expected one NoneAlgorithm.get_signature constant source audit, got {len(signature_audits)}")
signature_audit = signature_audits[0]
signature_totals = signature_audit["totals"]
if signature_audit.get("universe_kind") != "constant":
    raise SystemExit(f"FAIL: expected constant audit, got {signature_audit.get('universe_kind')}")
if signature_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: NoneAlgorithm.get_signature source dig has unclassified source: totals={signature_totals}")
if signature_audit["source_memento"].get("source_function_name") != "NoneAlgorithm.get_signature":
    raise SystemExit(f"FAIL: source oracle function should point at method body: {signature_audit['source_memento']!r}")
if not any(
    m.get("role") == "python.constant-universe"
    and m.get("source_function_name") == "NoneAlgorithm.get_signature"
    for m in result.get("sourceMementos") or []
):
    raise SystemExit("FAIL: lift report missing class-method constant source memento")
hmac_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.instance-field-universe"
    and "HMACAlgorithm" in audit.get("contract", {}).get("name", "")
]
if len(hmac_audits) != 1:
    raise SystemExit(
        f"FAIL: expected one HMACAlgorithm digest_method audit, got {len(hmac_audits)}"
    )
hmac_audit = hmac_audits[0]
hmac_totals = hmac_audit["totals"]
if hmac_audit.get("universe_kind") != "constructor-field-getter":
    raise SystemExit(
        f"FAIL: expected constructor-field-getter audit, got {hmac_audit.get('universe_kind')}"
    )
hmac_memento = hmac_audit["source_memento"]
if hmac_memento.get("source_function_name") != "HMACAlgorithm.__init__":
    raise SystemExit(
        "FAIL: HMACAlgorithm source oracle should point at constructor: "
        f"{hmac_memento!r}"
    )
if hmac_memento.get("constructor_default_param_names") != ["digest_method"]:
    raise SystemExit(
        "FAIL: HMACAlgorithm source memento did not record the defaulted param: "
        f"{hmac_memento!r}"
    )
if hmac_memento.get("constructor_default_attr_name") != "default_digest_method":
    raise SystemExit(
        "FAIL: HMACAlgorithm source memento did not record the default attr: "
        f"{hmac_memento!r}"
    )
if "body_text" in hmac_memento or "ast_template" in hmac_memento:
    raise SystemExit("FAIL: HMACAlgorithm source memento embeds source/template body")
if hmac_totals.get("unclassified_source") != 0:
    raise SystemExit(
        "FAIL: HMACAlgorithm.digest_method source dig has unclassified source: "
        f"totals={hmac_totals}"
    )
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "If"
    and locus.get("ast_path") == "$.body[0]"
    for locus in hmac_audit["loci"]
):
    raise SystemExit("FAIL: HMACAlgorithm digest-method default branch was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Assign"
    and locus.get("ast_path") == "$.body[0].body[0]"
    for locus in hmac_audit["loci"]
):
    raise SystemExit("FAIL: HMACAlgorithm digest-method default assignment was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "AnnAssign"
    and locus.get("ast_path") == "$.body[1]"
    for locus in hmac_audit["loci"]
):
    raise SystemExit("FAIL: HMACAlgorithm digest-method field assignment was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Constant"
    and locus.get("ast_path") == "$.args.defaults[0]"
    and "default constructor argument emitted" in locus.get("reason", "")
    for locus in hmac_audit["loci"]
):
    raise SystemExit("FAIL: HMACAlgorithm digest-method default argument was not warranted")
abstract_signature_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.raise-locus-universe"
    and "test_signing_algorithm_get_signature_is_abstract"
    in audit.get("contract", {}).get("name", "")
]
if len(abstract_signature_audits) != 1:
    raise SystemExit(
        "FAIL: expected one SigningAlgorithm.get_signature raise-locus "
        f"audit, got {len(abstract_signature_audits)}"
    )
abstract_signature_audit = abstract_signature_audits[0]
abstract_signature_totals = abstract_signature_audit["totals"]
if abstract_signature_audit.get("universe_kind") != "raise-locus":
    raise SystemExit(
        f"FAIL: expected raise-locus audit, got "
        f"{abstract_signature_audit.get('universe_kind')}"
    )
if (
    abstract_signature_audit["source_memento"].get("source_function_name")
    != "SigningAlgorithm.get_signature"
):
    raise SystemExit(
        "FAIL: abstract signature source oracle should point at method body: "
        f"{abstract_signature_audit['source_memento']!r}"
    )
if abstract_signature_totals.get("unclassified_source") != 0:
    raise SystemExit(
        "FAIL: SigningAlgorithm.get_signature raise-locus source dig has "
        f"unclassified source: totals={abstract_signature_totals}"
    )
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Raise"
    for locus in abstract_signature_audit["loci"]
):
    raise SystemExit("FAIL: SigningAlgorithm.get_signature raise statement was not warranted")
message_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.instance-field-universe"
    and "BadData.__str__" in audit.get("contract", {}).get("name", "")
]
message_by_function = {
    audit.get("source_memento", {}).get("source_function_name"): audit
    for audit in message_audits
}
if set(message_by_function) != {"BadData.__init__", "BadData.__str__"}:
    raise SystemExit(
        "FAIL: expected BadData constructor/getter instance-field audits, "
        f"got {sorted(str(k) for k in message_by_function)}"
    )
for function_name, message_audit in message_by_function.items():
    message_totals = message_audit["totals"]
    if message_totals.get("unclassified_source") != 0:
        raise SystemExit(
            f"FAIL: {function_name} instance-field source dig has "
            f"unclassified source: totals={message_totals}"
        )
init_audit = message_by_function["BadData.__init__"]
str_audit = message_by_function["BadData.__str__"]
if not any(
    locus.get("status") == "support"
    and locus.get("ast_kind") == "Expr"
    for locus in init_audit["loci"]
):
    raise SystemExit("FAIL: BadData.__init__ super call was not explicit support")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Assign"
    for locus in init_audit["loci"]
):
    raise SystemExit("FAIL: BadData.__init__ field assignment was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Return"
    for locus in str_audit["loci"]
):
    raise SystemExit("FAIL: BadData.__str__ return was not warranted")
payload_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.instance-field-universe"
    and "BadSignature" in audit.get("contract", {}).get("name", "")
]
if len(payload_audits) != 1:
    raise SystemExit(f"FAIL: expected one BadSignature payload audit, got {len(payload_audits)}")
payload_audit = payload_audits[0]
payload_totals = payload_audit["totals"]
if payload_audit.get("universe_kind") != "constructor-field-getter":
    raise SystemExit(f"FAIL: expected constructor-field-getter audit, got {payload_audit.get('universe_kind')}")
if payload_audit["source_memento"].get("source_function_name") != "BadSignature.__init__":
    raise SystemExit(f"FAIL: payload source oracle should point at constructor: {payload_audit['source_memento']!r}")
if payload_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: BadSignature.payload source dig has unclassified source: totals={payload_totals}")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "AnnAssign"
    for locus in payload_audit["loci"]
):
    raise SystemExit("FAIL: BadSignature.payload field assignment was not warranted")
bad_payload_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.instance-field-universe"
    and "BadPayload" in audit.get("contract", {}).get("name", "")
]
if len(bad_payload_audits) != 1:
    raise SystemExit(f"FAIL: expected one BadPayload default-field audit, got {len(bad_payload_audits)}")
bad_payload_audit = bad_payload_audits[0]
bad_payload_totals = bad_payload_audit["totals"]
if bad_payload_audit.get("universe_kind") != "constructor-field-getter":
    raise SystemExit(f"FAIL: expected constructor-field-getter audit, got {bad_payload_audit.get('universe_kind')}")
bad_payload_memento = bad_payload_audit["source_memento"]
if bad_payload_memento.get("source_function_name") != "BadPayload.__init__":
    raise SystemExit(f"FAIL: BadPayload source oracle should point at constructor: {bad_payload_memento!r}")
if bad_payload_memento.get("constructor_default_param_names") != ["original_error"]:
    raise SystemExit(f"FAIL: BadPayload source memento did not record the defaulted field: {bad_payload_memento!r}")
if "body_text" in bad_payload_memento or "ast_template" in bad_payload_memento:
    raise SystemExit("FAIL: BadPayload source memento embeds source/template body")
if bad_payload_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: BadPayload.original_error source dig has unclassified source: totals={bad_payload_totals}")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Constant"
    and locus.get("ast_path") == "$.args.defaults[0]"
    and "default constructor argument emitted" in locus.get("reason", "")
    for locus in bad_payload_audit["loci"]
):
    raise SystemExit("FAIL: BadPayload.original_error default argument was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "AnnAssign"
    for locus in bad_payload_audit["loci"]
):
    raise SystemExit("FAIL: BadPayload.original_error field assignment was not warranted")
header_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.instance-field-universe"
    and "BadHeader" in audit.get("contract", {}).get("name", "")
]
if len(header_audits) != 1:
    raise SystemExit(f"FAIL: expected one BadHeader header audit, got {len(header_audits)}")
header_audit = header_audits[0]
header_totals = header_audit["totals"]
if header_audit.get("universe_kind") != "constructor-field-getter":
    raise SystemExit(f"FAIL: expected constructor-field-getter audit, got {header_audit.get('universe_kind')}")
if header_audit["source_memento"].get("source_function_name") != "BadHeader.__init__":
    raise SystemExit(f"FAIL: header source oracle should point at constructor: {header_audit['source_memento']!r}")
if header_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: BadHeader.header source dig has unclassified source: totals={header_totals}")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "AnnAssign"
    and locus.get("line") == 85
    for locus in header_audit["loci"]
):
    raise SystemExit("FAIL: BadHeader.header field assignment was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "AnnAssign"
    and locus.get("line") == 89
    for locus in header_audit["loci"]
):
    raise SystemExit("FAIL: BadHeader.original_error field assignment was not accounted")
stdlib_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.delegation-universe"
    and audit.get("universe_kind") == "delegation-stdlib"
    and "_CompactJSON.loads" in audit.get("contract", {}).get("name", "")
]
if len(stdlib_audits) != 1:
    raise SystemExit(f"FAIL: expected one _CompactJSON.loads stdlib delegation audit, got {len(stdlib_audits)}")
stdlib_audit = stdlib_audits[0]
stdlib_totals = stdlib_audit["totals"]
if stdlib_audit["source_memento"].get("source_function_name") != "_CompactJSON.loads":
    raise SystemExit(f"FAIL: stdlib delegation source oracle should point at staticmethod: {stdlib_audit['source_memento']!r}")
if stdlib_totals.get("unclassified_source") != 0:
    raise SystemExit(f"FAIL: _CompactJSON.loads source dig has unclassified source: totals={stdlib_totals}")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Call"
    for locus in stdlib_audit["loci"]
):
    raise SystemExit("FAIL: _CompactJSON.loads stdlib call was not warranted")
package_audits = [
    audit for audit in result.get("sourceAudits", [])
    if audit.get("role") == "python.package-source"
    and audit.get("package") == "itsdangerous"
]
if len(package_audits) != 1:
    raise SystemExit(f"FAIL: expected one itsdangerous package accounting audit, got {len(package_audits)}")
package_totals = package_audits[0]["totals"]
if package_totals.get("unclassified_source", 0) <= 0:
    raise SystemExit(f"FAIL: itsdangerous package audit did not expose unclassified source: {package_totals}")
if ledger.get("unclassified_source") != package_totals.get("unclassified_source"):
    raise SystemExit(f"FAIL: package unclassified source not reflected in ledger: ledger={ledger} package={package_totals}")
serializer_overload_loci = [
    locus for locus in package_audits[0]["loci"]
    if str(locus.get("file", "")).endswith("/serializer.py")
    and any(
        str(locus.get("ast_path", "")).startswith(f"$.module.body[12].body[{index}]")
        for index in range(4, 9)
    )
]
if not serializer_overload_loci:
    raise SystemExit("FAIL: serializer overload metadata loci missing from package audit")
serializer_overload_totals = Counter(locus.get("status") for locus in serializer_overload_loci)
if serializer_overload_totals.get("unclassified", 0):
    raise SystemExit(
        "FAIL: serializer overload metadata still has unclassified source: "
        f"{serializer_overload_totals}"
    )
if not any(
    locus.get("status") == "support"
    and "overload" in locus.get("reason", "")
    for locus in serializer_overload_loci
):
    raise SystemExit("FAIL: serializer overload declaration metadata was not support")
if not any(
    locus.get("status") == "inactive"
    and "overload" in locus.get("reason", "")
    for locus in serializer_overload_loci
):
    raise SystemExit("FAIL: serializer overload ellipsis body was not inactive")
if audit.get("universe_kind") != "no-suffix-chars":
    raise SystemExit(f"FAIL: expected no-suffix-chars audit, got {audit.get('universe_kind')}")
if "body_text" in audit["source_memento"] or "ast_template" in audit["source_memento"]:
    raise SystemExit("FAIL: source audit memento embeds source/template body")
mementos = result.get("sourceMementos") or []
roles = {m.get("role") for m in mementos}
if "python.translate-universe" not in roles:
    raise SystemExit(f"FAIL: lift report missing python source mementos: roles={sorted(str(r) for r in roles)}")
memento = next(m for m in mementos if m.get("role") == "python.translate-universe")
if not memento.get("claimName", "").endswith("::assertion") or not memento.get("contractName", "").endswith("::assertion"):
    raise SystemExit(f"FAIL: source memento does not link to assertion contract: {memento!r}")
if "body_text" in memento or "ast_template" in memento:
    raise SystemExit(f"FAIL: source memento embeds source/template body: {memento!r}")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Attribute"
    and locus.get("ast_path") == "$.body[2].value.func"
    for locus in audit["loci"]
):
    raise SystemExit("FAIL: rstrip AST path was not warranted")
if not any(
    locus.get("status") == "warranted"
    and locus.get("ast_kind") == "Attribute"
    and locus.get("ast_path") == "$.body[0].value.func"
    for locus in int_audit["loci"]
):
    raise SystemExit("FAIL: lstrip AST path was not warranted")
print(
    "source audit base64:",
    f"loci={totals['source_loci']}",
    f"warranted={totals['source_warranted']}",
    f"inactive={totals['source_inactive']}",
    f"support={totals.get('source_support', 0)}",
    f"refused={totals['source_refused']}",
    f"unclassified={totals['unclassified_source']}",
)
print(
    "source audit int_to_bytes:",
    f"loci={int_totals['source_loci']}",
    f"warranted={int_totals['source_warranted']}",
    f"inactive={int_totals['source_inactive']}",
    f"support={int_totals.get('source_support', 0)}",
    f"refused={int_totals['source_refused']}",
    f"unclassified={int_totals['unclassified_source']}",
)
print(
    "source audit NoneAlgorithm.get_signature:",
    f"loci={signature_totals['source_loci']}",
    f"warranted={signature_totals['source_warranted']}",
    f"inactive={signature_totals['source_inactive']}",
    f"support={signature_totals.get('source_support', 0)}",
    f"refused={signature_totals['source_refused']}",
    f"unclassified={signature_totals['unclassified_source']}",
)
print(
    "source audit HMACAlgorithm.digest_method:",
    f"loci={hmac_totals['source_loci']}",
    f"warranted={hmac_totals['source_warranted']}",
    f"inactive={hmac_totals['source_inactive']}",
    f"support={hmac_totals.get('source_support', 0)}",
    f"refused={hmac_totals['source_refused']}",
    f"unclassified={hmac_totals['unclassified_source']}",
)
print(
    "source audit SigningAlgorithm.get_signature:",
    f"loci={abstract_signature_totals['source_loci']}",
    f"warranted={abstract_signature_totals['source_warranted']}",
    f"inactive={abstract_signature_totals['source_inactive']}",
    f"support={abstract_signature_totals.get('source_support', 0)}",
    f"refused={abstract_signature_totals['source_refused']}",
    f"unclassified={abstract_signature_totals['unclassified_source']}",
)
message_totals = {
    key: sum(audit["totals"][key] for audit in message_by_function.values())
    for key in (
        "source_loci",
        "source_warranted",
        "source_inactive",
        "source_support",
        "source_refused",
        "unclassified_source",
    )
}
print(
    "source audit BadData.__str__:",
    f"loci={message_totals['source_loci']}",
    f"warranted={message_totals['source_warranted']}",
    f"inactive={message_totals['source_inactive']}",
    f"support={message_totals.get('source_support', 0)}",
    f"refused={message_totals['source_refused']}",
    f"unclassified={message_totals['unclassified_source']}",
)
print(
    "source audit BadSignature.payload:",
    f"loci={payload_totals['source_loci']}",
    f"warranted={payload_totals['source_warranted']}",
    f"inactive={payload_totals['source_inactive']}",
    f"support={payload_totals.get('source_support', 0)}",
    f"refused={payload_totals['source_refused']}",
    f"unclassified={payload_totals['unclassified_source']}",
)
print(
    "source audit BadPayload.original_error:",
    f"loci={bad_payload_totals['source_loci']}",
    f"warranted={bad_payload_totals['source_warranted']}",
    f"inactive={bad_payload_totals['source_inactive']}",
    f"support={bad_payload_totals.get('source_support', 0)}",
    f"refused={bad_payload_totals['source_refused']}",
    f"unclassified={bad_payload_totals['unclassified_source']}",
)
print(
    "source audit BadHeader.header:",
    f"loci={header_totals['source_loci']}",
    f"warranted={header_totals['source_warranted']}",
    f"inactive={header_totals['source_inactive']}",
    f"support={header_totals.get('source_support', 0)}",
    f"refused={header_totals['source_refused']}",
    f"unclassified={header_totals['unclassified_source']}",
)
print(
    "source audit _CompactJSON.loads:",
    f"loci={stdlib_totals['source_loci']}",
    f"warranted={stdlib_totals['source_warranted']}",
    f"inactive={stdlib_totals['source_inactive']}",
    f"support={stdlib_totals.get('source_support', 0)}",
    f"refused={stdlib_totals['source_refused']}",
    f"unclassified={stdlib_totals['unclassified_source']}",
)
print(
    "source audit package:",
    f"loci={package_totals['source_loci']}",
    f"unclassified={package_totals['unclassified_source']}",
)
print(
    "source audit serializer overload metadata:",
    f"loci={len(serializer_overload_loci)}",
    f"support={serializer_overload_totals.get('support', 0)}",
    f"inactive={serializer_overload_totals.get('inactive', 0)}",
    f"unclassified={serializer_overload_totals.get('unclassified', 0)}",
)
PY
    rm -f "$report"
    return 1
  }
  rm -f "$report"
}

echo "== build the CLI =="
cargo build --manifest-path "$REPO/implementations/rust/Cargo.toml" -p sugar-cli --bin sugar >/dev/null || {
  echo "FAIL: sugar build"; exit 1; }
[ -x "$BIN" ] || { echo "FAIL: sugar binary missing at $BIN"; exit 1; }

run_twin() {
  local twin="$1" expect="$2"
  local dir="$HERE/$twin"
  echo
  echo "==================== twin: $twin (expect: $expect) ===================="
  rm -f "$dir"/blake3-512:*.proof 2>/dev/null
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/__pycache__" 2>/dev/null
  rm -f "$dir"/.prove*.json "$dir"/.verify*.json 2>/dev/null

  ( cd "$dir" && "$BIN" mint --out . ) >/dev/null || { echo "FAIL: mint ($twin)"; return 1; }
  ( cd "$dir" && "$BIN" verify --project . --json > .verify.json ) || true
  [ -s "$dir/.verify.json" ] || { echo "FAIL: no verify receipt ($twin)"; return 1; }

  EXPECT="$expect" TWIN="$twin" python3 - "$dir/.verify.json" <<'PY' || return 1
import json, os, sys
expect, twin = os.environ["EXPECT"], os.environ["TWIN"]
doc = json.load(open(sys.argv[1]))
found = [
    (r.get("property", ""), r.get("status", ""))
    for r in doc.get("rows", [])
    if "base64_encode" in str(r.get("property", ""))
]
if not found:
    print(f"FAIL({twin}): no base64_encode property rows in receipt"); sys.exit(1)
statuses = {s for _, s in found}
print(f"rows({twin}):")
for n, s in found:
    print(f"  {s:14s} {n[:110]}")
ok_words = {"discharged", "proven", "consistent", "sat"}
bad_words = {"unsatisfied", "refused", "unsat", "contradictory", "inconsistent", "violation", "violated"}
if expect == "discharged":
    verdict_ok = statuses & ok_words and not (statuses & bad_words)
else:
    verdict_ok = bool(statuses & bad_words)
if not verdict_ok:
    print(f"FAIL({twin}): expected {expect}, statuses={sorted(statuses)}"); sys.exit(1)
print(f"OK({twin}): {expect}")
PY
}

fail=0
audit_good_source || fail=1
run_twin good discharged || fail=1
run_twin bad refused || fail=1

echo
if [ "$fail" -ne 0 ]; then
  echo "==== itsdangerous-token-padding: FAIL ===="
  exit 1
fi
echo "==== itsdangerous-token-padding: PASS ===="
echo "the padded-token confusion refuted statically by one byte literal"
echo "(rstrip(b'=')) from itsdangerous' own source -- the real-name logo."
