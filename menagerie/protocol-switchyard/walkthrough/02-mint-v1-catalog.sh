#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
v1_catalog="$tmp/from-catalog.json"
v1_smug_cid="$(hash_spec "$V1_SMUGGLING_SPEC")"
v1_fram_cid="$(hash_spec "$V1_FRAMING_SPEC")"
write_v1_catalog "$v1_catalog"

section "Mint v1 Catalog"
explain_then_pause "hash v1 specs and assemble the v1 catalog" <<'TEXT'
What ProvekIt is doing here:
The v1 catalog names the obligations by content. The walkthrough computes blake3-512 over the raw bytes of each v1 spec, drops those CIDs into a `properties` map keyed by obligation name, then serializes the catalog JSON. The CID of the catalog itself is JCS-canonicalized blake3-512 of that serialization, computed by the CLI when it consumes the catalog.

Value ProvekIt adds:
The catalog is now an addressable artifact. Two parties cannot disagree about what v1 means without disagreeing about the CIDs. The fromCatalogCid will be the source of the migration edge in script 05.
TEXT

section "Spec CIDs"
say "request-smuggling-refusal       = $v1_smug_cid"
say "content-length-transfer-encoding = $v1_fram_cid"

section "v1 Catalog JSON"
show_json_file "$v1_catalog"
highlight_raw_line "$v1_catalog" '"version": "v1.0.0"' "v1 carries the v1.0.0 version label."
highlight_raw_line "$v1_catalog" "$v1_smug_cid" "request-smuggling-refusal binds the obligation to its content CID."
highlight_raw_line "$v1_catalog" "$v1_fram_cid" "content-length-transfer-encoding binds the same way."

section "v1 Catalog CID"
v1_catalog_cid="$(hash_spec "$v1_catalog")"
say "raw blake3-512 of catalog file: $v1_catalog_cid"
say "(Note: the CLI uses JCS-canonical blake3-512, so the on-the-wire fromCatalogCid in script 05 will differ from this raw-byte CID.)"

analysis_with_receipts <<'TEXT'
The two property CIDs receipt the obligation bytes. The catalog JSON receipt names those CIDs in a key-sorted properties map. Script 05 will feed this catalog to the CLI and emit the canonical fromCatalogCid that the migration edge uses.
TEXT

next_script "03-show-v2-profile.sh"
