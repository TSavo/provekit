#!/usr/bin/env python3
"""
Mint concept:identity → c cell.

The identity function is the cleanest possible realization:
  concept:identity<T> is a function type T → T returning its input.
  C realization: #define IDENTITY(x) (x)
  Loss record: empty or trivial — no structural divergence.

This cell demonstrates that projection-distance can be ZERO
when the abstraction and realization are structurally identical.
"""

import json
import hashlib
import sys
from pathlib import Path


def mint_identity_concept():
    """Mint the concept:identity abstraction."""
    concept_payload = {
        "name": "concept:identity",
        "category": "function",
        "signature": "T → T",
        "description": "Identity function returning its input unchanged",
        "polymorphic": True,
        "type_parameter": "T",
        "postcondition": "forall x: T. identity(x) == x"
    }
    concept_json = json.dumps(concept_payload, sort_keys=True, separators=(',', ':'))
    concept_cid = hashlib.sha256(concept_json.encode()).hexdigest()
    return concept_cid, concept_payload


def mint_identity_c_realization():
    """Mint the C macro realization of concept:identity."""
    c_code = """#define IDENTITY(x) (x)"""

    realization_payload = {
        "abstraction": "concept:identity",
        "target_language": "c",
        "form": "macro",
        "macro_definition": c_code,
        "zero_overhead": True,
        "description": "Trivial macro expansion — compiles away completely"
    }
    realization_json = json.dumps(realization_payload, sort_keys=True, separators=(',', ':'))
    realization_cid = hashlib.sha256(realization_json.encode()).hexdigest()
    return realization_cid, realization_payload


def mint_identity_loss_record():
    """Mint the loss record for concept:identity → c."""
    loss_payload = {
        "projection": "concept:identity → c",
        "structural_divergence": None,
        "rationale": "Identity function compiles away in C; no semantic loss"
    }
    loss_json = json.dumps(loss_payload, sort_keys=True, separators=(',', ':'))
    loss_cid = hashlib.sha256(loss_json.encode()).hexdigest()
    return loss_cid, loss_payload


def main():
    concept_cid, concept = mint_identity_concept()
    realization_cid, realization = mint_identity_c_realization()
    loss_cid, loss = mint_identity_loss_record()

    print(f"concept:identity CID:\n{concept_cid}\n")
    print(f"concept_payload:\n{json.dumps(concept, indent=2)}\n")

    print(f"concept:identity → c realization CID:\n{realization_cid}\n")
    print(f"realization_payload:\n{json.dumps(realization, indent=2)}\n")

    print(f"loss_record CID:\n{loss_cid}\n")
    print(f"loss_payload:\n{json.dumps(loss, indent=2)}\n")

    # Wire into index
    index = {
        "concept": {
            "cid": concept_cid,
            "payload": concept
        },
        "realization_c": {
            "cid": realization_cid,
            "payload": realization
        },
        "loss_record": {
            "cid": loss_cid,
            "payload": loss
        }
    }

    # Write the master index
    index_path = Path(__file__).parent.parent / "identity_c_cell.json"
    with open(index_path, 'w') as f:
        json.dump(index, f, indent=2)

    print(f"Wrote index to {index_path}")

    # Return CIDs for verification
    return (concept_cid, realization_cid, loss_cid)


if __name__ == "__main__":
    main()
