import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_fully_qualified_path as fqp


class FullyQualifiedPathShapeTests(unittest.TestCase):
    def test_shape_names_single_lossless_path_payload(self):
        spec = fqp.build_shape_spec()

        self.assertEqual(spec["fn_name"], "concept:fully-qualified-path")
        self.assertEqual(spec["formals"], ["path"])
        self.assertEqual([sort["name"] for sort in spec["formal_sorts"]], ["String"])
        self.assertEqual(spec["return_sort"]["name"], "Path")
        self.assertEqual(spec["post"]["operator"], "fully-qualified-path")
        self.assertEqual(spec["post"]["arity_shape"]["slots"], ["path"])

    def test_shape_covers_path_discrimination_roles(self):
        spec = fqp.build_shape_spec()

        self.assertEqual(spec["effects"], {"effects": []})
        self.assertEqual(
            spec["path_roles"],
            ["module", "trait", "associated-item", "crate-root"],
        )

    def test_generated_payload_is_ascii(self):
        payload = json.dumps(fqp.build_shape_spec(), ensure_ascii=False)
        payload.encode("ascii")


if __name__ == "__main__":
    unittest.main()
