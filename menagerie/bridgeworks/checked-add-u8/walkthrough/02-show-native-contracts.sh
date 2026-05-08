#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
identities_json="$tmp/identities.json"
identities_stderr="$tmp/identities.stderr"

section "Native Contract Markers"
say "This asks the configured lifter to identify native contract markers without full ProofIR lowering."
explain_then_pause "run identify-only lift" <<'TEXT'
What ProvekIt is doing here:
ProvekIt is running the lift protocol in identify-only mode. The Rust CLI reads .provekit/config.toml, resolves the Bridgeworks surface, starts the configured lifter, and asks for the native contract identities it can see. It is intentionally stopping before full ProofIR lowering.

Value ProvekIt adds:
Identity is a real contract gate. Before behavior can be witnessed, before bridge edges can be trusted, the artifacts must claim the contract names the chain needs. ProvekIt gives that identity step a protocol and a JSON output instead of leaving it as an informal grep through a source tree.

Relationship to the chain:
The final compiler-to-software edge needs checked_add_u8.postcondition as its consequent. If the C file advertises overflow_add_u8.postcondition instead, the edge has nowhere to land. That failure is different from a bad implementation. It is a failed identity match at the software boundary.

What to look for:
After the prompt, the raw JSON should show an identity-document. The highlighted lines should point to the checked_add_u8.postcondition claim and the exact C marker that produced it.
TEXT
print_provekit lift "menagerie/bridgeworks/checked-add-u8" --identify-only --json --quiet
run_provekit_capture "$identities_json" "$identities_stderr" lift "menagerie/bridgeworks/checked-add-u8" --identify-only --json --quiet

section "Full Lifted Identifier JSON With Line Numbers"
show_json_file "$identities_json"

section "Relevant Bit: Raw Identifier Lines"
highlight_raw_line "$identities_json" '"claim": "checked_add_u8.postcondition",' "The software boundary advertises the checked-add contract by name."
highlight_raw_line "$identities_json" '"text": "/* provekit:contract checked_add_u8.postcondition */"' "The identifier came from the native C marker, not a separate summary."

analysis_with_receipts <<'TEXT'
The raw JSON is the receipt. It is not a prose summary saying the contract exists; it is the actual identify-only lift result emitted through provekit lift. The highlighted claim line proves the software boundary advertises checked_add_u8.postcondition, and the highlighted text line proves that identity came from the C marker inside the native source artifact.

That proves the initial claim for this step: ProvekIt can identify the contract names the bridge chain depends on before it tries to prove behavior. If this identity line were missing or renamed, the compiler-to-software bridge would fail before lower ever ran.
TEXT

section "Point"
say "Identification is still delegated through ProvekIt. Full lift is the next step."

next_script "03-lift-to-proofir.sh"
