#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
write_v1_catalog "$tmp/from-catalog.json"
write_v2_catalog "$tmp/to-catalog.json"
write_policy "$tmp/policy.json"
write_verifier "$tmp/verifier.json"
mkdir -p "$tmp/witness"

tampered_spec="$tmp/v2-request-smuggling-refusal-tampered.md"
cp "$V2_SMUGGLING_SPEC" "$tampered_spec"
printf '\nTAMPERED LINE\n' >> "$tampered_spec"

section "Break Spec Bytes"
explain_then_pause "feed a tampered v2 spec into provekit protocol evolve" <<'TEXT'
What ProvekIt is doing here:
The v2 catalog says request-smuggling-refusal binds to a specific blake3-512 of the v2 spec bytes. The break script keeps the catalog unchanged and points the --changed-spec flag at a tampered copy of the spec instead. The CLI rehashes the file the flag points at, compares it to the property CID the catalog declares, and refuses the mint if they disagree.

Value ProvekIt adds:
The bridge between the catalog property layer and the spec bytes layer is the load-bearing rail. If a producer can swap spec bytes without updating the catalog, the catalog is decorative. ProvekIt refuses to mint when those layers disagree.

Expected refusal: changed spec `request-smuggling-refusal` CID mismatch.
TEXT

section "Tamper Diff"
diff -u --label "original/v2/request-smuggling-refusal.md" --label "tampered-copy/request-smuggling-refusal.md" \
  "$V2_SMUGGLING_SPEC" "$tampered_spec" || true

print_provekit protocol evolve \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --out-dir "$tmp/witness" \
  --change-class extension-only \
  --producer protocol-switchyard-walkthrough \
  --changed-spec "request-smuggling-refusal=$tampered_spec" \
  --changed-spec "content-length-transfer-encoding=$V2_FRAMING_SPEC" \
  --json --quiet

stdout_file="$tmp/evolve.stdout"
stderr_file="$tmp/evolve.stderr"
status=0
set +e
run_provekit_capture "$stdout_file" "$stderr_file" \
  protocol evolve \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --out-dir "$tmp/witness" \
  --change-class extension-only \
  --producer protocol-switchyard-walkthrough \
  --changed-spec "request-smuggling-refusal=$tampered_spec" \
  --changed-spec "content-length-transfer-encoding=$V2_FRAMING_SPEC" \
  --json --quiet
status=$?
set -e

if [ "$status" -eq 0 ]; then
  say "Unexpected success. The tampered spec was admitted."
  show_json_file "$stdout_file"
  exit 1
fi

section "Refusal Evidence"
show_text_file "$stderr_file"
if grep -F "changed spec \`request-smuggling-refusal\` CID mismatch" "$stderr_file" >/dev/null; then
  say "Rail fired: changed-spec CID mismatch (catalog property layer to spec bytes)."
else
  say "Did not see the expected rail message. stderr is above."
  exit 1
fi

section "Repository State"
say "The exhibit profiles/http directory was never modified."
say "The tampered spec was a copy under: $tampered_spec"
diff -q "$V2_SMUGGLING_SPEC" "$EXHIBIT_ROOT/profiles/http/v2/specs/request-smuggling-refusal.md" >/dev/null && say "v2 spec on disk is unchanged."

analysis_with_receipts <<'TEXT'
The mint refused. The error names the obligation key (`request-smuggling-refusal`), the CID the catalog declares, and the CID the file actually hashes to. That is the bridge edge between the catalog property layer and the spec bytes layer doing its job.

Repository state is clean because the break only ever touched a copy under $TMPDIR.
TEXT

next_script "11-break-policy-mode.sh"
