#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
say "Workspace: $tmp"

section "Mint Migration Witness"
explain_then_pause "invoke provekit protocol evolve" <<'TEXT'
What ProvekIt is doing here:
The walkthrough writes the v1 catalog, the v2 catalog, the policy, and the verifier into a temp dir, then invokes:

  provekit protocol evolve --from from-catalog.json --to to-catalog.json \
                           --policy policy.json --verifier verifier.json \
                           --out-dir witness/ --change-class extension-only \
                           --changed-spec request-smuggling-refusal=<path> \
                           --changed-spec content-length-transfer-encoding=<path>

The CLI canonicalizes each catalog (JCS), hashes it (blake3-512), checks each --changed-spec file's bytes against the property CID the catalog declares, builds a ProtocolCatalogDiff, fills out a ProtocolEvolutionBodyClaim with all the input CIDs in their closure, mints a TruthDischargeWitness over the body, and writes everything to disk.

Value ProvekIt adds:
The witnessed edge is the substrate primitive paper 10 talks about. fromCatalogCid and toCatalogCid are the two roots. bodyCid is the migration body. witnessCid is the truth-discharge over the body. None of those CIDs are claimed; all are recomputable from the inputs in this temp dir.
TEXT

stdout_file="$tmp/evolve.stdout"
stderr_file="$tmp/evolve.stderr"

print_provekit protocol evolve \
  --from "$tmp/from-catalog.json" \
  --to "$tmp/to-catalog.json" \
  --policy "$tmp/policy.json" \
  --verifier "$tmp/verifier.json" \
  --out-dir "$tmp/witness" \
  --change-class extension-only \
  --producer protocol-switchyard-walkthrough \
  --changed-spec "request-smuggling-refusal=$V2_SMUGGLING_SPEC" \
  --changed-spec "content-length-transfer-encoding=$V2_FRAMING_SPEC" \
  --json --quiet

mint_evolution_into "$tmp"

section "Evolve Summary JSON"
show_json_file "$stdout_file"
highlight_raw_line "$stdout_file" '"fromCatalogCid":' "Source root for the migration edge."
highlight_raw_line "$stdout_file" '"toCatalogCid":' "Destination root for the migration edge."
highlight_raw_line "$stdout_file" '"bodyCid":' "ProtocolEvolutionBodyClaim CID over both roots, the catalog diff, the policy, and the verifier."
highlight_raw_line "$stdout_file" '"witnessCid":' "TruthDischargeWitness CID over the body."
highlight_raw_line "$stdout_file" '"changeClass": "extension-only"' "Declared change class matches the policy's acceptedChangeClass."

section "Witness Artifacts on Disk"
ls -la "$tmp/witness"

section "Witness Body Excerpt"
show_json_file "$tmp/witness/protocol-evolution.body.json"

analysis_with_receipts <<'TEXT'
Four CIDs proved. fromCatalogCid is the JCS-canonical hash of the v1 catalog. toCatalogCid is the same for v2. bodyCid is the canonical hash of the body claim that names both roots, the catalog diff, the policy CID, the verifier CID, and the closure of input CIDs. witnessCid is the truth-discharge over the body. The migration edge is now an addressable artifact a third party can recompute.

Save this temp dir path; script 06 reads from it.
TEXT

cp -R "$tmp" "$tmp.preserved" 2>/dev/null || true
say ""
say "PROVEKIT_SWITCHYARD_LAST_TMP=$tmp"
printf '%s\n' "$tmp" > "${TMPDIR:-/tmp}/provekit-switchyard-last-tmp.txt"

next_script "06-verify-witness.sh"
