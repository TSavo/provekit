#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

section "Map The Package Rails"
explain_then_pause "inspect the exhibit rail map" <<'TEXT'
ProvekIt is not replacing package provenance. It is refusing to confuse provenance with admission. In this exhibit, package-manager roles become separate DAG queries over signer, contract set, lowered witness, .proof bundle, binary bytes, CI input closure, and policy.

The implication is the important part. A malicious maintainer is not blocked from claiming a contract. They are forced to lower that claim into evidence. If they keep the old contract and lie, the witness rail goes red. If they weaken the contract to match behavior, the contract-set rail goes red. If they replay metadata over different bytes, the binary rail goes red. If they reuse old CI evidence, the input-closure rail goes red.

What to look for after the prompt: the specimen names independent rails. Each one is a pin that can be queried without trusting a single giant opaque admission result. Even the `.proof` is just another pinned artifact in the graph.
TEXT

section "Raw Rail Lines"
grep -n -A40 '^rails:' "$EXHIBIT_ROOT/specimen.yaml"

analysis_with_receipts <<'TEXT'
The rail list is the design receipt. It shows that the exhibit is not "npm bad" or "SLSA bad." The system is making separate claims addressable: signer, contract set, witness, proof bundle, binary, CI input closure, and policy.

That is the stronger supply-chain story. A consumer can pin any one of those vectors as a tripwire, or pin the whole vector for admission.
TEXT

next_script "02-admit-baseline.sh"
