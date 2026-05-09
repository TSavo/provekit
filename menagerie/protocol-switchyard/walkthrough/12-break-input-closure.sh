#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
mint_evolution_into "$tmp"

section "Break Input Closure"
explain_then_pause "drop catalogDiffCid from inputCids and recheck" <<'TEXT'
What ProvekIt is doing here:
The body's inputCids array must include every CID the body's closure references: fromCatalogCid, toCatalogCid, policyCid, verifierCid, changed-spec CIDs, and every CID inside the changeSet and evidence subtrees. The break script removes evidence.catalogDiffCid from inputCids and runs the verifier.

Value ProvekIt adds:
Input closure is the rail that prevents an attacker from minting a body whose evidence references CIDs that were never declared as inputs. Every CID under evidence and changeSet must also appear in inputCids. The verifier walks the body's CID graph and checks closure.

Expected refusal: PEP admission refused: inputCids missing required CID `<the catalogDiffCid>`.
TEXT

cp "$tmp/witness/protocol-evolution.body.json" "$tmp/witness/body-original.json"
python3 - "$tmp/witness/protocol-evolution.body.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
required = d['evidence']['catalogDiffCid']
d['inputCids'] = [c for c in d['inputCids'] if c != required]
json.dump(d, open(p, 'w'), indent=2)
PY

section "Body Diff"
diff -u --label "original/body.json" --label "tampered/body.json" \
  "$tmp/witness/body-original.json" "$tmp/witness/protocol-evolution.body.json" || true

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
  say "Unexpected success. A non-closed body was admitted."
  show_json_file "$stdout_file"
  exit 1
fi

section "Refusal Evidence"
show_text_file "$stderr_file"
if grep -F "inputCids missing required CID" "$stderr_file" >/dev/null; then
  say "Rail fired: input-closure check refused the body."
else
  say "Did not see the expected rail message. stderr is above."
  exit 1
fi

section "Repository State"
say "Only the body inside the temp witness dir was touched. The exhibit on disk is untouched."

analysis_with_receipts <<'TEXT'
The verifier refused. The error names exactly which CID was required by the body's closure but missing from inputCids.

This is the rail that turns a body claim into a recomputable artifact. Anyone who can read the body knows up front the full set of bytes the claim depends on, and a verifier can refuse the moment the claim's references run outside that set.
TEXT

next_script "13-break-evidence-cid.sh"
