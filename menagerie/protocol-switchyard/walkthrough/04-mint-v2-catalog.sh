#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
v2_catalog="$tmp/to-catalog.json"
v2_smug_cid="$(hash_spec "$V2_SMUGGLING_SPEC")"
v2_fram_cid="$(hash_spec "$V2_FRAMING_SPEC")"
v1_smug_cid="$(hash_spec "$V1_SMUGGLING_SPEC")"
v1_fram_cid="$(hash_spec "$V1_FRAMING_SPEC")"
write_v2_catalog "$v2_catalog"

section "Mint v2 Catalog"
explain_then_pause "hash v2 specs and assemble the v2 catalog" <<'TEXT'
What ProvekIt is doing here:
Same shape as the v1 catalog, but the property CIDs come from the v2 spec bytes. The catalog also carries the v1.0.1 version label. That label is policy-checked: under `extension-only` with no cross-kit semantic obligation, the policy says a patch bump is allowed.

Value ProvekIt adds:
The change is locally observable. The v1 property CIDs and the v2 property CIDs are right here, side by side. A consumer can compare them without reading prose.
TEXT

section "Property CIDs Side by Side"
printf '  %-35s %s\n' "key" "v1                              v2"
printf '  %-35s %s -> %s\n' "request-smuggling-refusal" "$v1_smug_cid" "$v2_smug_cid"
printf '  %-35s %s -> %s\n' "content-length-transfer-encoding" "$v1_fram_cid" "$v2_fram_cid"

section "v2 Catalog JSON"
show_json_file "$v2_catalog"
highlight_raw_line "$v2_catalog" '"version": "v1.0.1"' "v2 carries the v1.0.1 patch label."
highlight_raw_line "$v2_catalog" "$v2_smug_cid" "v2 request-smuggling-refusal CID matches the v2 spec bytes."
highlight_raw_line "$v2_catalog" "$v2_fram_cid" "v2 content-length-transfer-encoding CID matches the v2 spec bytes."

analysis_with_receipts <<'TEXT'
Two CIDs changed, none were removed, none were added. That is the on-disk fingerprint of an extension-only edge. Script 05 will mint the witnessed migration edge that names both catalogs by their canonical CIDs.
TEXT

next_script "05-mint-migration-witness.sh"
