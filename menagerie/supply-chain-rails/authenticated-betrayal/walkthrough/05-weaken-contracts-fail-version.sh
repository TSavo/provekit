#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
inspect_json="$tmp/weakened-inspect.json"
inspect_stderr="$tmp/weakened-inspect.stderr"
version_json="$tmp/version-red.json"
version_stderr="$tmp/version-red.stderr"
status=0

section "Weakened Contract Fails Version Rail"
explain_then_pause "check contract-set extension" <<'TEXT'
The attacker has another move: stop lying. The weakened 1.4.2 package removes runtime.no-env-secret-read from its declared contract set. That makes the behavior easier to witness honestly, but it changes the contract set for a compatible-looking patch update.

ProvekIt value here is versioned contract continuity. A compatible update must satisfy oldSet subset newSet. If the old contract disappears, the release can still be authentic, but it is not an admissible compatible continuation.

What to look for: conventional receipts stay green, then version check rejects and names runtime.no-env-secret-read as missing.
TEXT

print_provekit package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-weakened" --json --quiet
run_provekit_capture "$inspect_json" "$inspect_stderr" package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-weakened" --json --quiet

section "Weakened Package Inspection JSON"
show_json_file "$inspect_json"
highlight_raw_line "$inspect_json" '"tool": "slsa-verifier"' "The SLSA verifier still accepts the weakened release's package receipt."
highlight_raw_line "$inspect_json" '"contractSetCid": "blake3-512:a4d0ee37' "The weakened release declares a different contract set."
highlight_raw_line "$inspect_json" '"lifter": "provekit-lift-ts"' "The different contract set is still derived from the TypeScript lifter."

print_provekit version check-extension --previous "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.1.json" --candidate "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.2-weakened.json" --json --quiet
set +e
run_provekit_capture "$version_json" "$version_stderr" version check-extension --previous "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.1.json" --candidate "menagerie/supply-chain-rails/authenticated-betrayal/expected/release-1.4.2-weakened.json" --json --quiet
status=$?
set -e
if [ "$status" -eq 0 ]; then
  say "Unexpectedly accepted weakened contract set."
  show_json_file "$version_json"
  exit 1
fi

section "Version Rail Rejection JSON"
show_json_file "$version_json"
highlight_raw_line "$version_json" '"verdict": "rejected"' "The compatible-update rail rejects the candidate."
highlight_raw_line "$version_json" '"rule": "oldSet subset newSet"' "The rule is contract-set extension, not package identity."
highlight_raw_line "$version_json" '"runtime.no-env-secret-read"' "The removed preserved contract is named in the receipt."

analysis_with_receipts <<'TEXT'
This proves the maintainer's forced choice. If they preserve the old contract, witness lower goes red. If they weaken the contract to match behavior, version compatibility goes red. Ordinary provenance remains useful context, but it is not the admission predicate.
TEXT

next_script "06-substitute-bytes-fail-binary.sh"
