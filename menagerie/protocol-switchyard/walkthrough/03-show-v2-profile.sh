#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Show v2 Profile"
explain_then_pause "show the v2 obligations and what changed" <<'TEXT'
What ProvekIt is doing here:
The v2 profile keeps the same two obligation keys but strengthens them. The framing rule closes more cases unconditionally; the smuggling refusal grows new reason codes. Same names, different bytes.

Value ProvekIt adds:
Because each obligation is content-addressed, "strengthened the obligation" is not a release-note adjective. It is a CID change at the property layer that flows up into a different catalog CID. v1 conformance witnesses do not transfer to v2 by name, by signer, or by version label. They transfer only if the property CIDs they witness still appear in the new catalog.
TEXT

section "v2 request-smuggling-refusal.md"
show_text_file "$V2_SMUGGLING_SPEC"

section "v2 content-length-transfer-encoding.md"
show_text_file "$V2_FRAMING_SPEC"

section "Diff Against v1 (request-smuggling-refusal)"
diff -u --label "v1/request-smuggling-refusal.md" --label "v2/request-smuggling-refusal.md" \
  "$V1_SMUGGLING_SPEC" "$V2_SMUGGLING_SPEC" || true

section "Diff Against v1 (content-length-transfer-encoding)"
diff -u --label "v1/content-length-transfer-encoding.md" --label "v2/content-length-transfer-encoding.md" \
  "$V1_FRAMING_SPEC" "$V2_FRAMING_SPEC" || true

analysis_with_receipts <<'TEXT'
Both obligations are strengthened, not replaced. Both keys carry the same names in v2 as in v1. That is exactly what `extension-only` means in the policy: same rail set, stricter enforcement on each rail.
TEXT

next_script "04-mint-v2-catalog.sh"
