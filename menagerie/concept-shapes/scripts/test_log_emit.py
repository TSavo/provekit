import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_log_emit as log_emit


class LogEmitShapeTests(unittest.TestCase):
    def test_shape_is_logger_agnostic_concept_term(self):
        spec = log_emit.build_shape_spec()

        self.assertEqual(spec["fn_name"], "concept:log-emit")
        self.assertEqual(spec["formals"], ["level", "message", "structured_fields"])
        self.assertEqual(
            [sort["name"] for sort in spec["formal_sorts"]],
            ["LogLevel", "String", "StructuredFields"],
        )
        self.assertEqual(spec["return_sort"]["name"], "Unit")
        self.assertEqual(spec["post"]["operator"], "log-emit")
        self.assertEqual(
            spec["post"]["arity_shape"]["slots"],
            ["level", "message", "structured_fields"],
        )

    def test_effect_and_loss_dimensions_are_explicit(self):
        spec = log_emit.build_shape_spec()

        self.assertEqual(spec["effects"], {"effects": [{"kind": "effect-signature", "name": "IO"}]})
        self.assertEqual(
            spec["loss_dimensions"],
            [
                "level-semantics",
                "mdc-context-propagation",
                "sink-buffered-vs-immediate",
                "structured-vs-formatted",
            ],
        )
        self.assertEqual(spec["log_levels"], ["trace", "debug", "info", "warn", "error", "fatal"])

    def test_generated_payload_is_ascii(self):
        payload = json.dumps(log_emit.build_shape_spec(), ensure_ascii=False)
        payload.encode("ascii")


if __name__ == "__main__":
    unittest.main()
