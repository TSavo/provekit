#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
mint_evolution_into "$tmp"

section "Break Policy Mode"
explain_then_pause "rewrite body.changeClass to migration-required and recheck" <<'TEXT'
What ProvekIt is doing here:
The body is minted under change-class extension-only against a policy that declares acceptedChangeClass extension-only. The break script rewrites the body's changeClass to migration-required without changing the policy. The verifier then refuses on policy/body class mismatch.

Value ProvekIt adds:
A producer cannot upgrade the change class after the fact by editing one field in the body. The policy is hashed into the body via policyCid. The change class is a body field too. The verifier reads both and refuses any mismatch with the policy's acceptedChangeClass.

Expected refusal: PEP admission refused: policy accepts `extension-only`, body declares `migration-required`.
TEXT

cp "$tmp/witness/protocol-evolution.body.json" "$tmp/witness/body-original.json"
python3 - "$tmp/witness/protocol-evolution.body.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d['changeClass'] = 'migration-required'
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
  say "Unexpected success. The mismatched body was admitted."
  show_json_file "$stdout_file"
  exit 1
fi

section "Refusal Evidence"
show_text_file "$stderr_file"
if grep -F "policy accepts \`extension-only\`, body declares \`migration-required\`" "$stderr_file" >/dev/null; then
  say "Rail fired: policy/body changeClass mismatch."
else
  say "Did not see the expected rail message. stderr is above."
  exit 1
fi

section "Repository State"
say "Only the body inside the temp witness dir was touched. The exhibit on disk is untouched."

analysis_with_receipts <<'TEXT'
The verifier refused. The error names both the policy's accepted class and the body's declared class. The mismatch is the rail.

Why this matters: an extension-only edge says "no producer-side migration required for consumers." A migration-required edge says the opposite. They are different physics and the policy is the rule that picks. The body cannot lie about which physics it lives under.
TEXT

next_script "12-break-input-closure.sh"
