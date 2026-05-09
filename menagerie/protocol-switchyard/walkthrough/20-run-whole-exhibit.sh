#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

section "Run Whole Exhibit"
explain_then_pause "invoke the protocol-switchyard runner end-to-end" <<'TEXT'
What ProvekIt is doing here:
The runner is the integrated form of scripts 02 through 06. It writes catalogs, policy, and verifier into its own temp dir, shells through `provekit protocol evolve`, and prints the four load-bearing CIDs.

Value ProvekIt adds:
The runner is the answer to "what does an end-to-end integrator look like for paper 10?" The numbered tour scripts show the same primitives one piece at a time; the runner shows the pieces composing.
TEXT

stdout_file="$(tmp_dir)/runner.stdout"
stderr_file="$stdout_file.stderr"

print_switchyard --all --json
run_switchyard_capture "$stdout_file" "$stderr_file" --all --json

section "Runner JSON"
show_json_file "$stdout_file"
highlight_raw_line "$stdout_file" '"from_catalog_cid":' "Same root paper 10 names as the source root."
highlight_raw_line "$stdout_file" '"to_catalog_cid":' "Same root paper 10 names as the destination root."
highlight_raw_line "$stdout_file" '"body_cid":' "Migration body CID (paper 10 section 6)."
highlight_raw_line "$stdout_file" '"witness_cid":' "TruthDischargeWitness CID over the body."

analysis_with_receipts <<'TEXT'
End-to-end pass. The runner's CIDs come from the same canonicalization the tour scripts used, so the visitor can rerun script 05 and get the same fromCatalogCid and toCatalogCid the runner just emitted. The break scripts already proved each rail refuses tampering one input at a time. The runner shows the green case integrated.
TEXT

say ""
say "Walkthrough complete."
