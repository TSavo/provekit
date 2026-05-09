#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Show v1 Profile"
explain_then_pause "show the v1 obligations" <<'TEXT'
What ProvekIt is doing here:
The v1 profile names two boundary obligations as prose:
  1. request-smuggling-refusal: the parser refuses messages with ambiguous Content-Length / Transfer-Encoding framing;
  2. content-length-transfer-encoding: the parser determines body length from a single, ordered source.

These are the obligations the v1 profile carries. Their bytes will become property CIDs in the v1 catalog, which will become the fromCatalogCid for the migration edge.

Value ProvekIt adds:
ProvekIt does not parse the prose. It content-addresses it. The bytes of each spec file are the obligation. If the bytes drift, the obligation has drifted, and the catalog CID drifts with it. Section 03 shows how v2 strengthens the same two obligations.
TEXT

section "v1 request-smuggling-refusal.md"
show_text_file "$V1_SMUGGLING_SPEC"

section "v1 content-length-transfer-encoding.md"
show_text_file "$V1_FRAMING_SPEC"

analysis_with_receipts <<'TEXT'
No CIDs yet. These are the obligation bytes the v1 catalog points at. The next script hashes them and shows the resulting fromCatalogCid.
TEXT

next_script "02-mint-v1-catalog.sh"
