import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import discharge


class DischargeTests(unittest.TestCase):
    def test_x86_hex_literal_normalizes_to_signed_int_shape(self):
        contract = {
            "kind": "function-contract",
            "fnName": "foo",
            "formals": ["edi"],
            "formalSorts": [{"kind": "primitive", "name": "BitVector"}],
            "returnSort": {"kind": "primitive", "name": "MachineState"},
            "pre": {"kind": "atomic", "name": "true", "args": []},
            "post": {
                "kind": "and",
                "operands": [
                    {
                        "kind": "implies",
                        "operands": [
                            {
                                "kind": "not",
                                "operands": [
                                    {
                                        "kind": "atomic",
                                        "name": "=",
                                        "args": [
                                            {"kind": "var", "name": "edi"},
                                            {
                                                "kind": "const",
                                                "value": 0,
                                                "sort": {
                                                    "kind": "primitive",
                                                    "name": "Int",
                                                },
                                            },
                                        ],
                                    }
                                ],
                            },
                            {
                                "kind": "atomic",
                                "name": "=",
                                "args": [
                                    {"kind": "var", "name": "eax_post"},
                                    {"kind": "var", "name": "edi"},
                                ],
                            },
                        ],
                    },
                    {
                        "kind": "implies",
                        "operands": [
                            {
                                "kind": "atomic",
                                "name": "=",
                                "args": [
                                    {"kind": "var", "name": "edi"},
                                    {
                                        "kind": "const",
                                        "value": 0,
                                        "sort": {"kind": "primitive", "name": "Int"},
                                    },
                                ],
                            },
                            {
                                "kind": "atomic",
                                "name": "=",
                                "args": [
                                    {"kind": "var", "name": "eax_post"},
                                    {
                                        "kind": "const",
                                        "value": "0xffffffea",
                                        "sort": {
                                            "kind": "primitive",
                                            "name": "BitVector",
                                        },
                                    },
                                ],
                            },
                        ],
                    },
                ],
            },
            "effects": [],
        }

        normalized = discharge.normalize_to_shape_payload(
            contract,
            {"edi": "arg_0", "eax_post": "ret"},
            {"BitVector": "Int"},
        )

        self.assertEqual(discharge.shape_payload(), normalized)
