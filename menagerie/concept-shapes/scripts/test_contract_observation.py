import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_contract_observation as contract_observation


class ContractObservationShapeTests(unittest.TestCase):
    def test_shape_is_parameterized_concept_term_with_mode_slot(self):
        spec = contract_observation.build_shape_spec()

        self.assertEqual(spec["fn_name"], "concept:contract-observation")
        self.assertEqual(spec["formals"], ["callsite_cid", "contract_cid", "mode"])
        self.assertEqual(
            [sort["name"] for sort in spec["formal_sorts"]],
            ["Cid", "Cid", "ContractObservationMode"],
        )
        self.assertEqual(spec["return_sort"]["name"], "ContractObservationResult")
        self.assertEqual(spec["post"]["operator"], "contract-observation")
        self.assertEqual(
            spec["post"]["arity_shape"]["slots"],
            ["callsite_cid", "contract_cid", "mode"],
        )

    def test_object_shape_does_not_carry_observer_effects(self):
        spec = contract_observation.build_shape_spec()

        self.assertEqual(
            spec["effects"],
            {"effects": []},
            "observer effects belong to ObservationWrapperMemento, not the object concept shape",
        )
        modes = {row["mode"]: row for row in spec["observation_modes"]}
        self.assertEqual(
            [effect["name"] for effect in modes["Witness"]["wrapper_effects"]],
            ["IO", "Sign"],
        )
        self.assertEqual(
            [effect["name"] for effect in modes["Emitter"]["wrapper_effects"]],
            ["IO"],
        )
        self.assertEqual(
            [effect["name"] for effect in modes["Gate"]["wrapper_effects"]],
            ["Throw"],
        )
        self.assertEqual(
            [effect["name"] for effect in modes["Monitor"]["wrapper_effects"]],
            ["Reads"],
        )

    def test_modes_declare_explicit_composition_points(self):
        spec = contract_observation.build_shape_spec()
        modes = {row["mode"]: row for row in spec["observation_modes"]}

        self.assertEqual(modes["Witness"]["composition_points"], ["after-return"])
        self.assertIn("before", modes["Gate"]["composition_points"])
        self.assertIn("around", modes["Gate"]["composition_points"])
        self.assertIn("after-throw", modes["Emitter"]["composition_points"])

    def test_generated_payload_is_ascii(self):
        payload = json.dumps(contract_observation.build_shape_spec(), ensure_ascii=False)
        payload.encode("ascii")


if __name__ == "__main__":
    unittest.main()
