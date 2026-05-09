#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0

import argparse

from in_toto.models._signer import (
    load_crypto_signer_from_pkcs8_file,
    load_public_key_from_file,
)
from in_toto.models.layout import Layout, Step
from in_toto.models.metadata import Metablock


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--layout-key", required=True)
    parser.add_argument("--functionary-pub", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    functionary_pub = load_public_key_from_file(args.functionary_pub)
    step = Step(
        name="safe-json-pack",
        pubkeys=[functionary_pub["keyid"]],
        expected_command=[],
        threshold=1,
    )
    for rel in ["package.json", "index.js", "contracts.ts", "contracts.json"]:
        step.add_material_rule_from_string(f"ALLOW {rel}")
    step.add_material_rule_from_string("DISALLOW *")
    step.add_product_rule_from_string("CREATE package.tgz")
    step.add_product_rule_from_string("DISALLOW *")

    layout = Layout(
        keys={functionary_pub["keyid"]: functionary_pub},
        steps=[step],
        inspect=[],
        expires="2036-05-08T00:00:00Z",
        readme=(
            "Supply Chain Rails native in-toto receipt: the packaging step "
            "created package.tgz from the package manifest, implementation, "
            "and ProvekIt contract source. This layout does not assert that "
            "the package satisfies its behavioral contracts."
        ),
    )
    metadata = Metablock(signed=layout, signatures=[])
    metadata.create_signature(load_crypto_signer_from_pkcs8_file(args.layout_key))
    metadata.dump(args.output)


if __name__ == "__main__":
    main()
