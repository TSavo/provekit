#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
baseline_json="$tmp/baseline.json"
candidate_json="$tmp/candidate.json"
stderr="$tmp/package.stderr"

section "Stale CI Fails Input Closure"
explain_then_pause "compare package input closures" <<'TEXT'
CI reuse is not keyed by job name. It is keyed by the exact input closure: source, package manifest, contracts, tarball fixture, policy, protocol catalog, and witness inputs. In this toy npm exhibit, ProvekIt computes the package-shaped closure as part of package inspection.

The accepted baseline closure belongs to safe-json@1.4.1. The poisoned 1.4.2 release has different source and tarball bytes, so it cannot reuse the baseline CI evidence.

What to look for: both outputs are ordinary provekit package inspections, and their inputClosureCid lines differ.
TEXT

print_provekit package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.1" --json --quiet
run_provekit_capture "$baseline_json" "$stderr" package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.1" --json --quiet
print_provekit package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet
run_provekit_capture "$candidate_json" "$stderr" package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet

section "Accepted Baseline Closure JSON"
show_json_file "$baseline_json"
highlight_raw_line "$baseline_json" '"inputClosureCid":' "This is the accepted baseline input closure."

section "Candidate Closure JSON"
show_json_file "$candidate_json"
highlight_raw_line "$candidate_json" '"inputClosureCid":' "This is the candidate closure; it is different, so stale CI reuse is inadmissible."

analysis_with_receipts <<'TEXT'
The two raw inputClosureCid lines are the receipts. A green job name is not enough. The closure changed, so a prior accepted result witness cannot be reused for this candidate.

This is the CI version of the same rail story: the package can be authentic and conventional-green while still tripping a content-addressed admission vector.
TEXT

next_script "08-run-whole-exhibit.sh"
