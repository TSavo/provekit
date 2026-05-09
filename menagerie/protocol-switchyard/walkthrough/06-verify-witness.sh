#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
mint_evolution_into "$tmp"

section "Verify Witness"
explain_then_pause "invoke provekit protocol check-evolution" <<'TEXT'
What ProvekIt is doing here:
Verification is the same admission code path as mint, run against the on-disk artifacts:

  provekit protocol check-evolution --body protocol-evolution.body.json \
                                    --from from-catalog.json --to to-catalog.json \
                                    --policy policy.json --verifier verifier.json \
                                    --catalog-diff catalog-diff.json

The verifier rehashes both catalogs and confirms body.fromCatalogCid and body.toCatalogCid agree. It rehashes the policy and verifier and confirms body.policyCid and body.verifierCid agree. It rehashes the catalog diff and confirms body.evidence.catalogDiffCid agrees. It then checks that the changeSet in the body matches the catalog diff arrays, that the version label rule is consistent with the change class, that the change class is the one the policy accepts, and that body.inputCids contains every CID required by the body's closure.

Value ProvekIt adds:
None of those rails depend on the producer being trustworthy. They are all locally recomputable. Anyone who can read the artifacts can run the same admission code with no shared secret.
TEXT

stdout_file="$tmp/check.stdout"
stderr_file="$tmp/check.stderr"

print_provekit protocol check-evolution \
  --body "$tmp/witness/protocol-evolution.body.json" \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --catalog-diff "$tmp/witness/catalog-diff.json" \
  --json --quiet

run_provekit_capture "$stdout_file" "$stderr_file" \
  protocol check-evolution \
  --body "$tmp/witness/protocol-evolution.body.json" \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --catalog-diff "$tmp/witness/catalog-diff.json" \
  --json --quiet

section "Check JSON"
show_json_file "$stdout_file"
highlight_raw_line "$stdout_file" '"ok": true,' "Every admission rail returned ok."
highlight_raw_line "$stdout_file" '"bodyCid":' "Body CID matches script 05's bodyCid receipt."
highlight_raw_line "$stdout_file" '"changeClass": "extension-only"' "Body's declared class still matches the policy."
highlight_raw_line "$stdout_file" '"catalogDiffCid":' "Catalog diff bytes still hash to the CID the body claims."

analysis_with_receipts <<'TEXT'
The migration edge admits. Now we need to prove the rails are not theatre. The break scripts (10, 11, 12, 13) perturb one rail at a time on a copy of this temp dir and show the verifier refuses. If even one of those breaks slipped through, the green receipt above would not be evidence; it would be polite text. They do not slip through.
TEXT

next_script "10-break-spec-bytes.sh"
