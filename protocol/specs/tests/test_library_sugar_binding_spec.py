from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LIFT_SPEC = ROOT / "protocol/specs/2026-04-30-lift-plugin-protocol.md"
BIND_SPEC = ROOT / "protocol/specs/2026-05-13-bind-ir-lift-result.md"


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class LibrarySugarBindingSpecTests(unittest.TestCase):
    def test_lift_layer_cddl_names_library_bindings(self) -> None:
        spec = _read(LIFT_SPEC)

        self.assertIn('lift-layer = "all" / "identify-only" / "library-bindings"', spec)

    def test_bind_ir_spec_defines_library_sugar_binding_entry_in_ir_document(self) -> None:
        spec = _read(BIND_SPEC)

        self.assertIn("library-sugar-binding-entry = {", spec)
        self.assertIn('kind:                         "library-sugar-binding-entry"', spec)
        self.assertIn("target_language:              tstr,", spec)
        self.assertIn("target_library_tag:           tstr,", spec)
        self.assertIn("concept_name:                 tstr,", spec)
        self.assertIn("signature_shape_cid:          cid,", spec)
        self.assertIn("body_source:                  source-locator,", spec)
        self.assertIn("loss_record_contribution:     loss-record-contribution,", spec)
        self.assertIn("term_shape:                   term-shape-doc / null,", spec)
        self.assertIn("term_shape_cid:               cid / null", spec)
        self.assertIn("`ir-document.ir[]`", spec)

    def test_bind_ir_spec_forbids_freeform_body_shape_reason(self) -> None:
        spec = _read(BIND_SPEC)

        self.assertIn("freeform", spec.lower())
        self.assertIn("named retirement", spec.lower())
        self.assertNotIn("reason that the language lifter cannot yet lift body shape", spec)
        self.assertNotIn("explicit reason", spec)

    def test_bind_ir_spec_requires_source_cid_round_trip_and_determinism(self) -> None:
        spec = _read(BIND_SPEC)

        self.assertIn("byte determinism", spec.lower())
        self.assertIn("body_source.source_cid", spec)
        self.assertIn("recompute", spec.lower())
        self.assertIn("host-language source span", spec)

    def test_bind_ir_spec_contains_source_authored_python_example(self) -> None:
        spec = _read(BIND_SPEC)

        self.assertIn("@sugar.bind", spec)
        self.assertIn("import requests", spec)
        self.assertIn("requests.get", spec)
        self.assertIn("not authored as a string template", spec)


if __name__ == "__main__":
    unittest.main()
