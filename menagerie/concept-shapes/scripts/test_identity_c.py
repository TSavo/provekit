#!/usr/bin/env python3
"""
Unit tests for concept:identity → c cell minting.
Verifies byte-stable CID generation and loss-record correctness.
"""

import unittest
import json
import hashlib
from pathlib import Path
from mint_identity import (
    mint_identity_concept,
    mint_identity_c_realization,
    mint_identity_loss_record
)


class TestIdentityCell(unittest.TestCase):

    def test_concept_identity_cid_stable(self):
        """Concept:identity must produce the same CID on repeated runs."""
        cid1, _ = mint_identity_concept()
        cid2, _ = mint_identity_concept()
        self.assertEqual(cid1, cid2, "concept:identity CID not byte-stable")

    def test_realization_c_cid_stable(self):
        """C realization must produce the same CID on repeated runs."""
        cid1, _ = mint_identity_c_realization()
        cid2, _ = mint_identity_c_realization()
        self.assertEqual(cid1, cid2, "C realization CID not byte-stable")

    def test_loss_record_cid_stable(self):
        """Loss record must produce the same CID on repeated runs."""
        cid1, _ = mint_identity_loss_record()
        cid2, _ = mint_identity_loss_record()
        self.assertEqual(cid1, cid2, "Loss record CID not byte-stable")

    def test_loss_record_is_trivial(self):
        """Loss record should indicate no structural divergence."""
        _, loss = mint_identity_loss_record()
        self.assertIsNone(loss['structural_divergence'],
                         "Identity function should have zero loss")

    def test_c_realization_is_macro(self):
        """C realization must be a macro, not a function."""
        _, realization = mint_identity_c_realization()
        self.assertEqual(realization['form'], 'macro',
                        "Identity must be a macro in C")
        self.assertIn('IDENTITY(x)', realization['macro_definition'],
                     "Macro must define IDENTITY(x)")

    def test_c_zero_overhead(self):
        """C realization must be zero-overhead."""
        _, realization = mint_identity_c_realization()
        self.assertTrue(realization['zero_overhead'],
                       "Identity macro should compile away")

    def test_concept_has_postcondition(self):
        """Concept must state its postcondition."""
        _, concept = mint_identity_concept()
        self.assertIn('postcondition', concept)
        self.assertIn('identity(x) == x', concept['postcondition'])

    def test_cid_format_hex(self):
        """All CIDs must be valid hex strings (SHA256)."""
        cids = [
            mint_identity_concept()[0],
            mint_identity_c_realization()[0],
            mint_identity_loss_record()[0]
        ]
        for cid in cids:
            self.assertEqual(len(cid), 64, f"CID {cid} is not 64 hex chars")
            try:
                int(cid, 16)
            except ValueError:
                self.fail(f"CID {cid} is not valid hex")


if __name__ == '__main__':
    unittest.main()
