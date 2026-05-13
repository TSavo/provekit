#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
verify_json="$tmp/binary-red.json"
verify_stderr="$tmp/binary-red.stderr"
status=0

section "Substituted Bytes Fail Binary Rail"
explain_then_pause "verify observed tarball bytes against admitted binaryCid" <<'TEXT'
This failure mode is simpler. The attacker reuses accepted release metadata but serves different bytes. Package metadata can be replayed; bytes cannot fake a content-addressed binary rail.

ProvekIt computes the observed binary CID from the tarball it was actually given and compares it to the binary CID in the release receipt. No behavioral witness is needed if the bytes are already wrong.

What to look for: attestedBinaryCid and observedBinaryCid are different, and the verdict is rejected.
TEXT

print_provekit verify --artifact "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-substituted/package.tgz" --proof "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.2.json" --json --quiet
set +e
run_provekit_capture "$verify_json" "$verify_stderr" verify --artifact "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-substituted/package.tgz" --proof "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.2.json" --json --quiet
status=$?
set -e
if [ "$status" -eq 0 ]; then
  say "Unexpectedly accepted substituted bytes."
  show_json_file "$verify_json"
  exit 1
fi

section "Binary Rail Rejection JSON"
show_json_file "$verify_json"
highlight_raw_line "$verify_json" '"reason": "binaryCid mismatch"' "The binary rail rejected before behavior was considered."
highlight_raw_line "$verify_json" '"attestedBinaryCid":' "This is the binary CID named by the release receipt."
highlight_raw_line "$verify_json" '"observedBinaryCid":' "This is the CID ProvekIt computed from the observed tarball."

analysis_with_receipts <<'TEXT'
The binary rail is a content-addressed tripwire. Even if the metadata is familiar, ProvekIt admits the bytes it sees, not the story around the bytes.
TEXT

next_script "07-reuse-stale-ci-fail-closure.sh"
