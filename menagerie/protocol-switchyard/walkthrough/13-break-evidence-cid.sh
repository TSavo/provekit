#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
mint_evolution_into "$tmp"

section "Break Evidence CID"
explain_then_pause "tamper with catalog-diff bytes after the body was minted" <<'TEXT'
What ProvekIt is doing here:
The body's evidence.catalogDiffCid pins the bytes of catalog-diff.json. The break script edits the catalog-diff file under the temp witness dir (adds a phantom unchangedSemantics entry) and asks the verifier to admit the same body against this new file.

Value ProvekIt adds:
Evidence is content-addressed. The body declared the diff bytes ahead of time; the verifier rehashes the file the verifier received; if those disagree, the file in question is not the file the body was talking about, and the admission must refuse.

Expected refusal: PEP admission refused: evidence catalogDiffCid is `<body's CID>`, supplied catalog diff hashes to `<file CID>`.
TEXT

cp "$tmp/witness/catalog-diff.json" "$tmp/witness/catalog-diff-original.json"
python3 - "$tmp/witness/catalog-diff.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['unchangedSemantics'] = ['phantom-injected-entry'] + d['unchangedSemantics']
json.dump(d, open(p, 'w'), indent=2)
PY

section "Catalog Diff Tamper"
diff -u --label "original/catalog-diff.json" --label "tampered/catalog-diff.json" \
  "$tmp/witness/catalog-diff-original.json" "$tmp/witness/catalog-diff.json" || true

print_provekit protocol check-evolution \
  --body "$tmp/witness/protocol-evolution.body.json" \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --catalog-diff "$tmp/witness/catalog-diff.json" \
  --json --quiet

stdout_file="$tmp/check.stdout"
stderr_file="$tmp/check.stderr"
status=0
set +e
run_provekit_capture "$stdout_file" "$stderr_file" \
  protocol check-evolution \
  --body "$tmp/witness/protocol-evolution.body.json" \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --catalog-diff "$tmp/witness/catalog-diff.json" \
  --json --quiet
status=$?
set -e

if [ "$status" -eq 0 ]; then
  say "Unexpected success. The verifier admitted a body against a tampered catalog-diff."
  show_json_file "$stdout_file"
  exit 1
fi

section "Refusal Evidence"
show_text_file "$stderr_file"
if grep -F "evidence catalogDiffCid is" "$stderr_file" >/dev/null; then
  say "Rail fired: evidence CID does not match the supplied catalog-diff bytes."
else
  say "Did not see the expected rail message. stderr is above."
  exit 1
fi

section "Repository State"
say "Only files inside the temp witness dir were touched. The exhibit on disk is untouched."

analysis_with_receipts <<'TEXT'
The verifier refused. The error prints the CID the body claims for the catalog diff and the CID the file actually hashes to. They are two strings that must be one. They are not, so the body and the file disagree, so the admission refuses.

This rail forecloses the simplest tamper: edit a downstream evidence file and hope nobody rehashes it. ProvekIt rehashes it.
TEXT

next_script "20-run-whole-exhibit.sh"
