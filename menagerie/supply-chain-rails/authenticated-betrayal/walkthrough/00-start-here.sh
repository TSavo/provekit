#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Start Here"
explain_then_pause "start the exhibit" <<'TEXT'
This exhibit is about an npm package that looks normal to ordinary supply-chain checks. The package name is right, the maintainer story is right, the tarball hash is right, `slsa-verifier verify-vsa` accepts the signed SLSA verification summary, and `in-toto-verify` accepts the signed packaging layout and link. That is the point: those systems can prove useful provenance and process facts without proving the package still satisfies the contract its consumers rely on.

ProvekIt adds a different admission question. It asks whether the release extends the previous contract set, whether each preserved contract can be lowered into evidence, whether the observed bytes match the admitted binary CID, whether CI evidence applies to this exact input closure, and whether policy still admits the signer and witness classes. The package manager sees one artifact. ProvekIt sees a vector of rails.

The first package is safe-json@1.4.1, a tiny JSON boundary helper with believable contracts: deterministic parsing, no network effect, no install side effect, and no runtime secret environment read. The second package is safe-json@1.4.2, signed by the same maintainer, but it reads SAFE_JSON_TOKEN on a rare telemetry-shaped input path.
TEXT

section "Claim"
say "SLSA does not catch it."
say "in-toto does not catch it."
say "ProvekIt does."

analysis_with_receipts <<'TEXT'
No command has run yet. The receipt standard for the rest of the walkthrough is simple: every material line comes from provekit output, and every human claim is tied back to a raw line from that output.
TEXT

next_script "01-map-package-rails.sh"
